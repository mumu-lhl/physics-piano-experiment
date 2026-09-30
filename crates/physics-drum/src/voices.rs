//! Physical drum-kit voice implementations.

use crate::contact::HuntCrossleyExciter;
use crate::membrane::{HEAD_MODE_COUNT, MembraneHead};
use physics_dsp::{ModalTransition, OverdampedPolicy, XorShift32};
use std::f64::consts::TAU;

const CYMBAL_MODE_COUNT: usize = 32;

/// A coupled two-head shell (kick or tom) with an explicit air-cavity spring.
#[derive(Debug, Clone)]
pub struct DoubleHeadVoice {
    pub top: MembraneHead,
    pub bottom: MembraneHead,
    pub exciter: HuntCrossleyExciter,
    pub cavity_pressure: f64,
    pub cavity_stiffness: f64,
    pub cavity_damping: f64,
    pub active: bool,
    pub tail_gain: f64,
}

impl DoubleHeadVoice {
    pub fn new(sample_rate: f64, fundamental_hz: f64, t60: f64, cavity_stiffness: f64) -> Self {
        Self {
            top: MembraneHead::new(sample_rate, fundamental_hz, t60),
            bottom: MembraneHead::new(sample_rate, fundamental_hz * 1.04, t60 * 0.82),
            exciter: HuntCrossleyExciter::default(),
            cavity_pressure: 0.0,
            cavity_stiffness,
            cavity_damping: 0.12,
            active: false,
            tail_gain: 1.0,
        }
    }

    pub fn trigger(&mut self, velocity: f64) {
        let velocity = velocity.clamp(0.001, 1.0);
        if !self.active {
            self.top.reset();
            self.bottom.reset();
            self.cavity_pressure = 0.0;
        }
        self.top.geometry_nonlinearity = 0.02 * velocity * velocity;
        self.bottom.geometry_nonlinearity = 0.01 * velocity * velocity;
        self.exciter.trigger(
            1.1 + 4.8 * velocity.powf(1.25),
            0.018,
            1.5e6 + 2.5e6 * velocity,
            1.5,
        );
        self.active = true;
        self.tail_gain = 1.0;
    }

    #[inline]
    pub fn step(&mut self, dt: f64) -> f64 {
        if !self.active {
            return 0.0;
        }
        let (top_before, top_velocity_before) = self.top.strike_state();
        let contact_force = self.exciter.step(top_before, top_velocity_before, dt);
        // Reduced cavity compliance: pressure is a second-order acoustic state,
        // not an instantaneous or one-pole gain. The two heads feed the volume
        // displacement, and the pressure accelerates both heads in opposite
        // directions on the next modal update.
        let (top_volume, top_volume_velocity) = self.top.area_average_state();
        let (bottom_volume, bottom_volume_velocity) = self.bottom.area_average_state();
        let volume_delta = top_volume - bottom_volume;
        let volume_velocity = top_volume_velocity - bottom_volume_velocity;
        let acoustic_damping = 2.0 * self.cavity_damping * self.cavity_stiffness.sqrt();
        self.cavity_pressure = (self.cavity_stiffness * volume_delta
            + acoustic_damping * volume_velocity)
            .clamp(-1.0e6, 1.0e6);

        let cavity_load = self.cavity_pressure * 0.08;
        let (_top_displacement, top_velocity) = self.top.step_coupled(contact_force, -cavity_load);
        let (_bottom_displacement, bottom_velocity) = self.bottom.step_coupled(0.0, cavity_load);
        let output = (top_velocity * 0.90 - bottom_velocity * 0.24 + contact_force * 0.0008)
            * self.tail_gain;
        self.tail_gain *= 0.999_999;

        let energy = self.top.energy() + self.bottom.energy() + self.cavity_pressure.abs() * 0.01;
        if !self.exciter.is_contacting && energy < 1.0e-7 {
            self.active = false;
        }
        output.clamp(-1.0, 1.0)
    }

    pub fn reset(&mut self) {
        self.top.reset();
        self.bottom.reset();
        self.exciter.reset();
        self.cavity_pressure = 0.0;
        self.active = false;
        self.tail_gain = 1.0;
    }
}

pub type KickVoice = DoubleHeadVoice;
pub type TomVoice = DoubleHeadVoice;

#[derive(Debug, Clone, Copy)]
struct SnareWire {
    displacement: f64,
    velocity: f64,
}

/// Coupled snare: bottom-head displacement drives 24 unilateral wire contacts.
#[derive(Debug, Clone)]
pub struct SnareVoice {
    pub top: MembraneHead,
    pub bottom: MembraneHead,
    pub exciter: HuntCrossleyExciter,
    wires: [SnareWire; 24],
    wire_shapes: [[f64; HEAD_MODE_COUNT]; 24],
    pub wire_count: usize,
    pub tightness: f64,
    pub decay: f64,
    pub cavity_pressure: f64,
    pub cavity_stiffness: f64,
    pub cavity_damping: f64,
    wire_modal_forces: [f64; HEAD_MODE_COUNT],
    pub active: bool,
    noise: XorShift32,
    noise_state: f64,
    noise_envelope: f64,
}

impl SnareVoice {
    pub fn new(sample_rate: f64) -> Self {
        let top = MembraneHead::new(sample_rate, 205.0, 0.32);
        let bottom = MembraneHead::new(sample_rate, 188.0, 0.46);
        let wire_shapes = std::array::from_fn(|index| {
            let position = index as f64 / 23.0;
            let radius = 0.10 + 1.70 * (position - 0.5).abs();
            bottom.shapes_at_radius(radius)
        });
        Self {
            top,
            bottom,
            wire_shapes,
            exciter: HuntCrossleyExciter::default(),
            wires: std::array::from_fn(|_| SnareWire {
                displacement: 0.0,
                velocity: 0.0,
            }),
            wire_count: 20,
            tightness: 0.62,
            decay: 0.58,
            cavity_pressure: 0.0,
            cavity_stiffness: 90_000.0,
            cavity_damping: 0.12,
            wire_modal_forces: [0.0; HEAD_MODE_COUNT],
            active: false,
            noise: XorShift32::new(0x51_4E_41_52),
            noise_state: 0.0,
            noise_envelope: 0.0,
        }
    }

    pub fn set_tightness(&mut self, tightness: f64) {
        self.tightness = tightness.clamp(0.0, 1.0);
    }

    pub fn set_decay(&mut self, decay: f64) {
        let decay = decay.clamp(0.0, 1.0);
        if (self.decay - decay).abs() > 0.01 {
            self.decay = decay;
            self.top.set_t60(0.12 + 0.38 * decay);
            self.bottom.set_t60(0.16 + 0.58 * decay);
        }
    }

    pub fn trigger(&mut self, velocity: f64) {
        let velocity = velocity.clamp(0.001, 1.0);
        if !self.active {
            self.top.reset();
            self.bottom.reset();
            for wire in &mut self.wires {
                wire.displacement = 0.0;
                wire.velocity = 0.0;
            }
            self.cavity_pressure = 0.0;
            self.wire_modal_forces.fill(0.0);
        }
        self.top.geometry_nonlinearity = 0.025 * velocity * velocity;
        self.bottom.geometry_nonlinearity = 0.01 * velocity * velocity;
        self.top.set_t60(0.12 + 0.38 * self.decay);
        self.bottom.set_t60(0.16 + 0.58 * self.decay);
        self.exciter.trigger(
            1.4 + 4.0 * velocity.powf(1.3),
            0.022,
            1.0e6 + 2.0e6 * velocity,
            1.5,
        );
        self.noise_envelope = (self.noise_envelope + 0.48 * velocity.powf(0.75)).min(1.0);
        self.active = true;
    }

    #[inline]
    pub fn step(&mut self, dt: f64) -> f64 {
        if !self.active {
            return 0.0;
        }
        let (strike_displacement, strike_velocity) = self.top.strike_state();
        let strike_force = self.exciter.step(strike_displacement, strike_velocity, dt);
        let (top_volume, top_volume_velocity) = self.top.area_average_state();
        let (bottom_volume, bottom_volume_velocity) = self.bottom.area_average_state();
        let volume_delta = top_volume - bottom_volume;
        let volume_velocity = top_volume_velocity - bottom_volume_velocity;
        let acoustic_damping = 2.0 * self.cavity_damping * self.cavity_stiffness.sqrt();
        self.cavity_pressure =
            self.cavity_stiffness * volume_delta + acoustic_damping * volume_velocity;
        let cavity_load = self.cavity_pressure * 0.08;
        let (_top_displacement, top_velocity) = self.top.step_coupled(strike_force, -cavity_load);
        let (_bottom_displacement, _bottom_velocity) =
            self.bottom
                .step_with_modal_forces(0.0, cavity_load, &self.wire_modal_forces);
        self.wire_modal_forces.fill(0.0);

        let mut chatter_force = 0.0;
        let mut wire_energy = 0.0;
        let count = self.wire_count.clamp(1, self.wires.len());
        let clearance = 0.000_001_5 + (1.0 - self.tightness) * 0.000_006;
        let wire_mass = 0.00055;
        let wire_stiffness = 4_000.0 + 18_000.0 * self.tightness;
        for (index, wire) in self.wires[..count].iter_mut().enumerate() {
            let shapes = &self.wire_shapes[index];
            let (local_bottom, local_bottom_velocity) = self.bottom.state_with_shapes(shapes);
            let gap = local_bottom - wire.displacement - clearance;
            let wire_velocity_relative = local_bottom_velocity - wire.velocity;
            let force = if gap > 0.0 {
                let elastic = (500_000.0 + 1_000_000.0 * self.tightness) * gap.powf(1.4);
                let dissipative = (elastic * 0.06 * wire_velocity_relative).max(0.0);
                (elastic + dissipative).min(30.0)
            } else {
                0.0
            };
            if force > 0.0 {
                chatter_force += force;
                self.bottom
                    .accumulate_modal_force(shapes, -force, &mut self.wire_modal_forces);
                wire.velocity += force / wire_mass * dt;
            } else {
                let wire_damping = 18.0 + 72.0 * (1.0 - self.decay);
                let restoring = -wire_stiffness * wire.displacement - wire_damping * wire.velocity;
                wire.velocity += restoring / wire_mass * dt;
            }
            wire.displacement += wire.velocity * dt;
            wire.displacement = wire.displacement.clamp(-0.01, 0.01);
            wire.velocity = wire.velocity.clamp(-30.0, 30.0);
            wire_energy += wire.displacement * wire.displacement * wire_stiffness
                + wire.velocity * wire.velocity * wire_mass;
        }

        let white = self.noise.next_f64();
        self.noise_state = self.noise_state * 0.87 + white * 0.13;
        let bright_noise = white - self.noise_state;
        self.noise_envelope *= (-(dt / (0.18 + 0.55 * self.decay.max(0.01)))).exp();
        let output = top_velocity * 0.52 - self.bottom.velocity() * 0.18
            + chatter_force * 0.000055
            + bright_noise * self.noise_envelope;
        let energy = self.top.energy() + self.bottom.energy() + wire_energy + self.noise_envelope;
        if !self.exciter.is_contacting && energy < 1e-7 {
            self.active = false;
        }
        output.clamp(-1.0, 1.0)
    }

    pub fn reset(&mut self) {
        self.top.reset();
        self.bottom.reset();
        self.exciter.reset();
        for wire in &mut self.wires {
            wire.displacement = 0.0;
            wire.velocity = 0.0;
        }
        self.active = false;
        self.cavity_pressure = 0.0;
        self.wire_modal_forces.fill(0.0);
        self.noise_envelope = 0.0;
        self.noise_state = 0.0;
    }
}

#[derive(Debug, Clone, Copy)]
struct CymbalMode {
    transition: ModalTransition,
    q: f64,
    v: f64,
    omega: f64,
    force_limit: f64,
    beta: f64,
    radiation_gain: f64,
}

impl CymbalMode {
    fn new(sample_rate: f64, frequency: f64, index: usize) -> Self {
        let frequency = frequency.min(sample_rate * 0.45);
        let omega = TAU * frequency;
        Self {
            transition: ModalTransition::new(
                omega,
                0.0,
                1.0 / sample_rate,
                OverdampedPolicy::ExponentialFallback,
            ),
            q: 0.0,
            v: 0.0,
            omega,
            force_limit: omega * omega * 0.1,
            // A quartic modal strain potential supplies a conservative
            // von-Karman-inspired nonlinear restoring force.
            // Keep the explicit reduced-order strain term a bounded perturbation.
            // Larger coefficients pump modal energy at high normalized frequencies.
            beta: omega * omega * 0.35,
            radiation_gain: 0.0008 / (1.0 + index as f64 * 0.025),
        }
    }

    fn set_decay(&mut self, sample_rate: f64, t60: f64) {
        let sigma = 6.907_755_278_982_137 / t60.max(0.03);
        self.transition = ModalTransition::new(
            self.omega,
            sigma,
            1.0 / sample_rate,
            OverdampedPolicy::ExponentialFallback,
        );
    }

    #[inline]
    fn step(&mut self, nonlinear_force: f64) -> f64 {
        let (p11, p12, p21, p22) = self.transition.phi;
        let (g1, g2) = self.transition.gamma;
        let (q0, v0) = (self.q, self.v);
        let next_q = p11 * q0 + p12 * v0 + g1 * nonlinear_force;
        let next_v = p21 * q0 + p22 * v0 + g2 * nonlinear_force;
        let max_displacement = 0.1;
        self.q = next_q.clamp(-max_displacement, max_displacement);
        self.v = next_v.clamp(
            -self.omega * max_displacement,
            self.omega * max_displacement,
        );
        self.v * self.radiation_gain
    }

    fn energy(&self) -> f64 {
        0.5 * (self.v * self.v + self.omega * self.omega * self.q * self.q)
            + 0.25 * self.beta * self.q.powi(4)
    }
}

/// Nonlinear, coupled cymbal modal bank with pedal-controlled damping and
/// additive retrigger excitation.
#[derive(Debug, Clone)]
pub struct CymbalVoice {
    sample_rate: f64,
    modes: [CymbalMode; CYMBAL_MODE_COUNT],
    modal_couplings: [f64; CYMBAL_MODE_COUNT - 1],
    pub open_amount: f64,
    pub decay_scale: f64,
    noise_decay_coefficient: f64,
    pub active: bool,
    noise: XorShift32,
    noise_state: f64,
    noise_envelope: f64,
}

impl CymbalVoice {
    pub fn new(sample_rate: f64) -> Self {
        let sample_rate = sample_rate.max(1.0);
        let ratios = [
            1.00, 1.73, 2.32, 3.15, 4.21, 5.08, 5.92, 6.77, 7.63, 8.49, 9.34, 10.2, 11.1, 12.0,
            13.1, 14.3, 15.6, 17.0, 18.5, 20.1, 21.8, 23.7, 25.7, 27.9, 30.2, 32.6, 35.1, 37.8,
            40.5, 43.5, 46.6, 49.8,
        ];
        let modes =
            std::array::from_fn(|index| CymbalMode::new(sample_rate, 330.0 * ratios[index], index));
        let modal_couplings =
            std::array::from_fn(|index| 0.18 * (modes[index].beta * modes[index + 1].beta).sqrt());
        let noise_decay_coefficient = (-(1.0 / sample_rate) / 1.95).exp();
        Self {
            sample_rate,
            modes,
            modal_couplings,
            open_amount: 1.0,
            decay_scale: 1.0,
            noise_decay_coefficient,
            active: false,
            noise: XorShift32::new(0xC1_7A_2B_1E),
            noise_state: 0.0,
            noise_envelope: 0.0,
        }
    }

    pub fn set_open_amount(&mut self, amount: f64) {
        let amount = amount.clamp(0.0, 1.0);
        if (self.open_amount - amount).abs() > 0.02 {
            self.open_amount = amount;
            self.update_decay();
        }
    }

    pub fn set_decay_scale(&mut self, scale: f64) {
        let scale = scale.clamp(0.25, 2.5);
        if (self.decay_scale - scale).abs() > 0.02 {
            self.decay_scale = scale;
            self.update_decay();
        }
    }

    pub fn trigger(&mut self, velocity: f64, open_amount: f64) {
        self.open_amount = open_amount.clamp(0.0, 1.0);
        self.update_decay();
        let velocity = velocity.clamp(0.001, 1.0);
        if !self.active {
            for mode in &mut self.modes {
                mode.q = 0.0;
                mode.v = 0.0;
            }
            self.noise_state = 0.0;
        }
        for (index, mode) in self.modes.iter_mut().enumerate() {
            let impact_shape = 1.0 / (1.0 + index as f64 * 0.055);
            mode.v += velocity.powf(0.7) * mode.omega * 0.012 * impact_shape;
        }
        self.noise_envelope = (self.noise_envelope + 0.22 * velocity.powf(0.55)).min(1.0);
        self.active = true;
    }

    /// Closed hi-hat contact removes modal energy; a short extra impulse is the
    /// audible chick/splash mechanical collision.
    pub fn choke(&mut self, amount: f64) {
        let factor = amount.clamp(0.0, 1.0);
        for mode in &mut self.modes {
            mode.q *= factor;
            mode.v *= factor;
        }
        self.noise_envelope *= factor;
    }

    #[inline]
    pub fn step(&mut self) -> f64 {
        if !self.active {
            return 0.0;
        }
        let positions: [f64; CYMBAL_MODE_COUNT] = std::array::from_fn(|index| self.modes[index].q);
        let mut output = 0.0;
        for index in 0..self.modes.len() {
            let mode = &self.modes[index];
            let q = positions[index];
            let mut restoring_force = -mode.beta * q * q * q;
            if index > 0 {
                let coupling = self.modal_couplings[index - 1];
                restoring_force -= coupling * q * positions[index - 1] * positions[index - 1];
            }
            if index + 1 < self.modes.len() {
                let coupling = self.modal_couplings[index];
                restoring_force -= coupling * q * positions[index + 1] * positions[index + 1];
            }
            output +=
                self.modes[index].step(restoring_force.clamp(-mode.force_limit, mode.force_limit));
        }
        let white = self.noise.next_f64();
        self.noise_state = self.noise_state * 0.91 + white * 0.09;
        self.noise_envelope *= self.noise_decay_coefficient;
        output += (white - self.noise_state) * self.noise_envelope;
        if self.energy() < 1e-8 {
            self.active = false;
        }
        output.clamp(-1.0, 1.0)
    }

    pub fn energy(&self) -> f64 {
        let mut energy = self.modes.iter().map(CymbalMode::energy).sum::<f64>();
        for index in 0..self.modes.len() - 1 {
            let left = &self.modes[index];
            let right = &self.modes[index + 1];
            energy += 0.5 * self.modal_couplings[index] * left.q.powi(2) * right.q.powi(2);
        }
        energy + self.noise_envelope * self.noise_envelope
    }

    pub fn reset(&mut self) {
        for mode in &mut self.modes {
            mode.q = 0.0;
            mode.v = 0.0;
        }
        self.noise_envelope = 0.0;
        self.noise_state = 0.0;
        self.active = false;
    }

    fn update_decay(&mut self) {
        self.noise_decay_coefficient =
            (-(1.0 / self.sample_rate) / (0.45 + 1.5 * self.open_amount * self.open_amount)).exp();
        let t60 = (0.04 + 3.0 * self.open_amount * self.open_amount) * self.decay_scale;
        for (index, mode) in self.modes.iter_mut().enumerate() {
            mode.set_decay(self.sample_rate, t60 / (1.0 + index as f64 * 0.08));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CymbalVoice, DoubleHeadVoice, SnareVoice};

    #[test]
    fn coupled_kick_and_snare_render_finite_impulses() {
        let mut kick = DoubleHeadVoice::new(48_000.0, 58.0, 0.6, 90_000.0);
        let mut snare = SnareVoice::new(48_000.0);
        kick.trigger(0.9);
        snare.trigger(0.9);
        let mut kick_peak: f64 = 0.0;
        let mut snare_peak: f64 = 0.0;
        for _ in 0..4_096 {
            kick_peak = kick_peak.max(kick.step(1.0 / 48_000.0).abs());
            snare_peak = snare_peak.max(snare.step(1.0 / 48_000.0).abs());
        }
        assert!(kick_peak.is_finite() && snare_peak.is_finite());
        assert!(kick_peak > 0.0 && snare_peak > 0.0);
    }

    #[test]
    fn snare_wires_feed_spatial_contact_reaction_back_to_the_bottom_head() {
        let mut snare = SnareVoice::new(48_000.0);
        snare.trigger(1.0);
        let mut reacted = false;
        for _ in 0..48_000 {
            let sample = snare.step(1.0 / 48_000.0);
            assert!(sample.is_finite());
            reacted |= snare
                .wire_modal_forces
                .iter()
                .any(|force| force.abs() > 0.0);
        }
        assert!(reacted);
    }

    #[test]
    fn cymbal_modes_are_dense_but_remain_below_nyquist() {
        let cymbal = CymbalVoice::new(32_000.0);
        assert_eq!(cymbal.modes.len(), 32);
        assert!(
            cymbal
                .modes
                .iter()
                .all(|mode| mode.omega < std::f64::consts::PI * 32_000.0)
        );
    }

    #[test]
    fn single_cymbal_hit_decays_to_a_quiet_tail() {
        let mut cymbal = CymbalVoice::new(48_000.0);
        cymbal.set_decay_scale(1.475);
        cymbal.trigger(1.0, 1.0);
        let mut early_energy = 0.0;
        let mut late_energy = 0.0;
        let window = 24_000;
        for sample in 0..10 * 48_000 {
            let output = cymbal.step();
            if sample < window {
                early_energy += output * output;
            } else if sample >= 19 * 24_000 {
                late_energy += output * output;
            }
        }
        let early_rms = (early_energy / window as f64).sqrt();
        let late_rms = (late_energy / window as f64).sqrt();
        assert!(
            late_rms < early_rms * 0.01,
            "early={early_rms}, late={late_rms}"
        );
    }

    #[test]
    fn open_cymbal_has_longer_decay_than_closed_hihat() {
        let mut closed = CymbalVoice::new(48_000.0);
        let mut open = CymbalVoice::new(48_000.0);
        closed.trigger(1.0, 0.0);
        open.trigger(1.0, 1.0);
        for _ in 0..48_000 {
            closed.step();
            open.step();
        }
        assert!(open.energy() > closed.energy() * 100.0);
    }

    #[test]
    fn cymbal_choke_reduces_energy() {
        let mut cymbal = CymbalVoice::new(48_000.0);
        cymbal.trigger(1.0, 1.0);
        for _ in 0..64 {
            let _ = cymbal.step();
        }
        let before = cymbal.energy();
        cymbal.choke(0.05);
        assert!(cymbal.energy() < before);
    }
}
