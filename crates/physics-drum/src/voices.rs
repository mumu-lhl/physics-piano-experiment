//! Physical drum-kit voice implementations.

use crate::contact::HuntCrossleyExciter;
use crate::membrane::MembraneHead;
use physics_dsp::XorShift32;
use std::f64::consts::TAU;

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
        self.top.reset();
        self.bottom.reset();
        self.cavity_pressure = 0.0;
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
        let top_before = self.top.displacement();
        let top_velocity_before = self.top.velocity();
        let contact_force = self.exciter.step(top_before, top_velocity_before, dt);
        let pressure_target =
            self.cavity_stiffness * (self.top.displacement() - self.bottom.displacement());
        self.cavity_pressure +=
            (pressure_target - self.cavity_pressure) * (dt * 180.0).clamp(0.0, 1.0);
        self.cavity_pressure *= (1.0 - self.cavity_damping * dt).max(0.0);

        let top_force = contact_force - self.cavity_pressure * 0.015;
        let bottom_force = self.cavity_pressure * 0.015;
        let (_top_displacement, top_velocity) = self.top.step(top_force);
        let (_bottom_displacement, bottom_velocity) = self.bottom.step(bottom_force);
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
    pub wire_count: usize,
    pub tightness: f64,
    pub decay: f64,
    pub active: bool,
    noise: XorShift32,
    noise_state: f64,
    noise_envelope: f64,
}

impl SnareVoice {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            top: MembraneHead::new(sample_rate, 205.0, 0.32),
            bottom: MembraneHead::new(sample_rate, 188.0, 0.46),
            exciter: HuntCrossleyExciter::default(),
            wires: [SnareWire {
                displacement: 0.0,
                velocity: 0.0,
            }; 24],
            wire_count: 20,
            tightness: 0.62,
            decay: 0.58,
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
        self.decay = decay.clamp(0.0, 1.0);
    }

    pub fn trigger(&mut self, velocity: f64) {
        let velocity = velocity.clamp(0.001, 1.0);
        self.top.reset();
        self.bottom.reset();
        for wire in &mut self.wires {
            wire.displacement = 0.0;
            wire.velocity = 0.0;
        }
        self.top.geometry_nonlinearity = 0.025 * velocity * velocity;
        self.bottom.geometry_nonlinearity = 0.01 * velocity * velocity;
        self.exciter.trigger(
            1.4 + 4.0 * velocity.powf(1.3),
            0.022,
            1.0e6 + 2.0e6 * velocity,
            1.5,
        );
        self.noise_envelope = 0.48 * velocity.powf(0.75);
        self.active = true;
    }

    #[inline]
    pub fn step(&mut self, dt: f64) -> f64 {
        if !self.active {
            return 0.0;
        }
        let strike_force = self
            .exciter
            .step(self.top.displacement(), self.top.velocity(), dt);
        let (top_displacement, top_velocity) = self.top.step(strike_force);
        let bottom_drive = (top_displacement - self.bottom.displacement()) * 0.32;
        let (bottom_displacement, bottom_velocity) = self.bottom.step(bottom_drive);

        let mut chatter_force = 0.0;
        let mut wire_energy = 0.0;
        let count = self.wire_count.clamp(1, self.wires.len());
        let clearance = 0.000_001_5 + (1.0 - self.tightness) * 0.000_006;
        let wire_mass = 0.00055;
        let wire_stiffness = 4_000.0 + 18_000.0 * self.tightness;
        for (index, wire) in self.wires[..count].iter_mut().enumerate() {
            let local_bottom = bottom_displacement * (0.85 + 0.15 * index as f64 / count as f64);
            let gap = local_bottom - wire.displacement - clearance;
            let wire_velocity_relative = bottom_velocity - wire.velocity;
            let force = if gap > 0.0 {
                let elastic = (500_000.0 + 1_000_000.0 * self.tightness) * gap.powf(1.4);
                let dissipative = (elastic * 0.06 * wire_velocity_relative).max(0.0);
                (elastic + dissipative).min(30.0)
            } else {
                0.0
            };
            if force > 0.0 {
                chatter_force += force;
                wire.velocity += force / wire_mass * dt;
            } else {
                let restoring = -wire_stiffness * wire.displacement - 38.0 * wire.velocity;
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
        let output = top_velocity * 0.52 - bottom_velocity * 0.18
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
        self.noise_envelope = 0.0;
        self.noise_state = 0.0;
    }
}

#[derive(Debug, Clone, Copy)]
struct CymbalMode {
    phase: f64,
    frequency: f64,
    amplitude: f64,
    decay_per_sample: f64,
    feedback: f64,
    previous: f64,
}

impl CymbalMode {
    fn new(sample_rate: f64, frequency: f64, t60: f64, feedback: f64) -> Self {
        Self {
            phase: 0.0,
            frequency,
            amplitude: 0.0,
            decay_per_sample: (-6.907_755_278_982_137 / (t60.max(0.03) * sample_rate)).exp(),
            feedback,
            previous: 0.0,
        }
    }

    #[inline]
    fn step(&mut self, sample_rate: f64, excitation: f64) -> f64 {
        let frequency_mod = 1.0 + self.feedback * self.previous * 0.12;
        self.phase = (self.phase + TAU * self.frequency * frequency_mod / sample_rate) % TAU;
        self.amplitude *= self.decay_per_sample;
        let output = self.amplitude * (self.phase + self.feedback * self.previous).sin();
        self.previous = output;
        self.amplitude += excitation;
        output
    }
}

/// Nonlinear cymbal bank with delayed high-frequency energy cascade and pedal
/// controlled modal damping.
#[derive(Debug, Clone)]
pub struct CymbalVoice {
    sample_rate: f64,
    modes: [CymbalMode; 24],
    pub open_amount: f64,
    pub decay_scale: f64,
    pub active: bool,
    noise: XorShift32,
    noise_state: f64,
    noise_envelope: f64,
    cascade: f64,
}

impl CymbalVoice {
    pub fn new(sample_rate: f64) -> Self {
        let sample_rate = sample_rate.max(1.0);
        let ratios = [
            1.00, 1.73, 2.32, 3.15, 4.21, 5.08, 5.92, 6.77, 7.63, 8.49, 9.34, 10.2, 11.1, 12.0,
            13.1, 14.3, 15.6, 17.0, 18.5, 20.1, 21.8, 23.7, 25.7, 27.9,
        ];
        let modes = std::array::from_fn(|index| {
            let t60 = 3.1 / (1.0 + index as f64 * 0.09);
            CymbalMode::new(
                sample_rate,
                330.0 * ratios[index],
                t60,
                0.05 + index as f64 * 0.002,
            )
        });
        Self {
            sample_rate,
            modes,
            open_amount: 1.0,
            decay_scale: 1.0,
            active: false,
            noise: XorShift32::new(0xC1_7A_2B_1E),
            noise_state: 0.0,
            noise_envelope: 0.0,
            cascade: 0.0,
        }
    }

    pub fn set_open_amount(&mut self, amount: f64) {
        let amount = amount.clamp(0.0, 1.0);
        if (self.open_amount - amount).abs() > 1e-5 {
            self.open_amount = amount;
            self.update_decay();
        }
    }

    pub fn set_decay_scale(&mut self, scale: f64) {
        let scale = scale.clamp(0.25, 2.5);
        if (self.decay_scale - scale).abs() > 1e-5 {
            self.decay_scale = scale;
            self.update_decay();
        }
    }

    pub fn trigger(&mut self, velocity: f64, open_amount: f64) {
        self.open_amount = open_amount.clamp(0.0, 1.0);
        self.update_decay();
        let velocity = velocity.clamp(0.001, 1.0);
        for (index, mode) in self.modes.iter_mut().enumerate() {
            mode.phase = 0.0;
            mode.previous = 0.0;
            mode.amplitude = velocity.powf(0.7) * (0.020 / (1.0 + index as f64 * 0.055));
        }
        self.noise_envelope = 0.22 * velocity.powf(0.55);
        self.cascade = velocity * 0.0004;
        self.active = true;
    }

    /// Closed hi-hat contact removes modal energy; a short extra impulse is the
    /// audible chick/splash mechanical collision.
    pub fn choke(&mut self, amount: f64) {
        let factor = amount.clamp(0.0, 1.0);
        for mode in &mut self.modes {
            mode.amplitude *= factor;
        }
        self.noise_envelope *= factor;
        self.cascade *= factor;
    }

    #[inline]
    pub fn step(&mut self) -> f64 {
        if !self.active {
            return 0.0;
        }
        let low_energy = self.modes[0].amplitude.abs();
        self.cascade = self.cascade * 0.9995 + low_energy * 0.00008;
        let mut output = 0.0;
        for (index, mode) in self.modes.iter_mut().enumerate() {
            let high_transfer = if index >= 6 {
                self.cascade * (index as f64 - 5.0) * 0.0006
            } else {
                0.0
            };
            output += mode.step(self.sample_rate, high_transfer);
        }
        let white = self.noise.next_f64();
        self.noise_state = self.noise_state * 0.91 + white * 0.09;
        self.noise_envelope *=
            (-(1.0 / self.sample_rate / (0.45 + 1.5 * self.open_amount * self.open_amount))).exp();
        output += (white - self.noise_state) * self.noise_envelope;
        if self.energy() < 1e-8 {
            self.active = false;
        }
        output.clamp(-1.0, 1.0)
    }

    pub fn energy(&self) -> f64 {
        self.modes
            .iter()
            .map(|mode| mode.amplitude * mode.amplitude)
            .sum::<f64>()
            + self.noise_envelope * self.noise_envelope
    }

    pub fn reset(&mut self) {
        for mode in &mut self.modes {
            mode.amplitude = 0.0;
            mode.previous = 0.0;
            mode.phase = 0.0;
        }
        self.noise_envelope = 0.0;
        self.cascade = 0.0;
        self.active = false;
    }

    fn update_decay(&mut self) {
        let t60 = (0.04 + 3.0 * self.open_amount * self.open_amount) * self.decay_scale;
        for (index, mode) in self.modes.iter_mut().enumerate() {
            mode.decay_per_sample = (-6.907_755_278_982_137
                / (t60 / (1.0 + index as f64 * 0.08) * self.sample_rate))
                .exp();
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
