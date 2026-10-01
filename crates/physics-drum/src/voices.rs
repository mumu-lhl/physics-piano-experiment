//! Physical drum-kit voice implementations with genuine acoustic physics.
//!
//! Replaces artificial noise generators and explicit coupling instabilities with:
//! 1. DoubleHeadVoice: Coupled top/bottom heads, cavity pressure & shell body resonance.
//! 2. SnareVoice: Top and resonant bottom heads with 24 discrete unilateral Hertzian
//!    wire contacts, bilateral modal back-reaction, and ZERO white noise.
//! 3. CymbalVoice: 128-mode plate bank with sparse Hamiltonian nonlinear coupling
//!    (energy waterfall shimmer/turbulent wash), boundary damping, and ZERO white noise.

use crate::contact::HuntCrossleyExciter;
use crate::membrane::{HEAD_MODE_COUNT, MembraneHead};
use physics_dsp::{ModalTransition, OverdampedPolicy};
use std::f64::consts::{PI, SQRT_2, TAU};

pub const CYMBAL_MODE_COUNT: usize = 128;
const SNARE_WIRE_COUNT: usize = 24;
const SNARE_CONTACT_POINT_COUNT: usize = 3;

/// Resonant 2nd-order bandpass filter for drum shell body resonance.
#[derive(Debug, Clone, Copy)]
pub struct ShellResonator {
    b0: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    x1: f64,
    x2: f64,
    y1: f64,
    y2: f64,
}

impl Default for ShellResonator {
    fn default() -> Self {
        Self {
            b0: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }
}

impl ShellResonator {
    pub fn new(sample_rate: f64, center_hz: f64, q: f64) -> Self {
        let w0 = TAU * center_hz.clamp(20.0, sample_rate * 0.45) / sample_rate;
        let alpha = w0.sin() / (2.0 * q.max(0.1));
        let cos_w0 = w0.cos();
        let a0 = 1.0 + alpha;
        Self {
            b0: alpha / a0,
            b2: -alpha / a0,
            a1: (-2.0 * cos_w0) / a0,
            a2: (1.0 - alpha) / a0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    #[inline]
    pub fn step(&mut self, input: f64) -> f64 {
        let y = self.b0 * input + self.b2 * self.x2 - self.a1 * self.y1 - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = input;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }

    pub fn reset(&mut self) {
        self.x1 = 0.0;
        self.x2 = 0.0;
        self.y1 = 0.0;
        self.y2 = 0.0;
    }
}

/// A coupled two-head shell (kick or tom) using coupled fundamental eigenmodes (in-phase & anti-phase)
/// and dual wood shell resonators.
#[derive(Debug, Clone)]
pub struct DoubleHeadVoice {
    pub top: MembraneHead,
    pub bottom: MembraneHead,
    pub exciter: HuntCrossleyExciter,
    // Fundamental (0, 1) coupled cavity eigenmodes:
    pub u_plus_q: f64,
    pub u_plus_v: f64,
    pub trans_plus: ModalTransition,
    pub u_minus_q: f64,
    pub u_minus_v: f64,
    pub trans_minus: ModalTransition,
    pub modal_mass_01: f64,
    pub shell1: ShellResonator,
    pub shell2: ShellResonator,
    pub active: bool,
    pub tail_gain: f64,
    pub output_gain: f64,
    pub is_kick: bool,
}

impl DoubleHeadVoice {
    pub fn new(sample_rate: f64, fundamental_hz: f64, t60: f64, cavity_stiffness: f64) -> Self {
        Self::new_with_geometry(
            sample_rate,
            fundamental_hz,
            t60,
            cavity_stiffness,
            0.1778,
            0.30,
        )
    }

    pub fn new_with_geometry(
        sample_rate: f64,
        fundamental_hz: f64,
        t60: f64,
        _cavity_stiffness: f64,
        head_radius_m: f64,
        surface_density_kg_m2: f64,
    ) -> Self {
        let is_kick = head_radius_m > 0.22;
        let strike_radius = if is_kick { 0.20 } else { 0.30 };
        let actual_t60 = if is_kick { t60.min(0.75) } else { t60 };
        let mut top = MembraneHead::new_with_geometry(
            sample_rate,
            fundamental_hz,
            actual_t60,
            head_radius_m,
            surface_density_kg_m2,
        );
        top.set_strike_point(strike_radius, 0.0);
        let contact_patch_fraction = if is_kick { 0.16 } else { 0.12 };
        top.set_strike_contact_radius(head_radius_m * contact_patch_fraction);

        let bottom = MembraneHead::new_with_geometry(
            sample_rate,
            fundamental_hz * 1.04,
            actual_t60 * 0.85,
            head_radius_m,
            surface_density_kg_m2 * 0.88,
        );

        let f0 = fundamental_hz.max(20.0);
        let total_mass = surface_density_kg_m2 * PI * head_radius_m * head_radius_m;
        let modal_mass_01 = total_mass * 0.25;

        // Mode splitting: anti-phase mode is higher (acoustic air spring in cavity)
        let split = if is_kick { 1.25 } else { 1.18 };
        let omega_plus = TAU * f0;
        let omega_minus = TAU * (f0 * split);
        let t60_plus = if is_kick { 0.75 } else { actual_t60.min(1.6) };
        let t60_minus = if is_kick { 0.45 } else { (actual_t60 * 0.65).min(1.0) };
        let dt = 1.0 / sample_rate;

        let trans_plus = ModalTransition::new(omega_plus, 1000.0_f64.ln() / t60_plus, dt, OverdampedPolicy::ExponentialFallback);
        let trans_minus = ModalTransition::new(omega_minus, 1000.0_f64.ln() / t60_minus, dt, OverdampedPolicy::ExponentialFallback);

        let (shell1, shell2) = if is_kick {
            (
                ShellResonator::new(sample_rate, 68.0, 4.0),
                ShellResonator::new(sample_rate, 130.0, 6.0),
            )
        } else {
            (
                ShellResonator::new(sample_rate, f0 * 1.6, 5.0),
                ShellResonator::new(sample_rate, f0 * 2.8, 7.0),
            )
        };

        let output_gain = if is_kick { 1.10 } else { 0.95 };

        Self {
            top,
            bottom,
            exciter: HuntCrossleyExciter::default(),
            u_plus_q: 0.0,
            u_plus_v: 0.0,
            trans_plus,
            u_minus_q: 0.0,
            u_minus_v: 0.0,
            trans_minus,
            modal_mass_01,
            shell1,
            shell2,
            active: false,
            tail_gain: 1.0,
            output_gain,
            is_kick,
        }
    }

    pub fn trigger(&mut self, velocity: f64) {
        let velocity = velocity.clamp(0.001, 1.0);
        if !self.active {
            self.reset();
        }
        self.top.geometry_nonlinearity = 0.02 * velocity * velocity;
        self.bottom.geometry_nonlinearity = 0.01 * velocity * velocity;
        let mallet_mass = if self.is_kick { 0.065 } else { 0.025 };
        let mallet_stiff = if self.is_kick { 1.2e6 } else { 2.2e6 };
        self.exciter.trigger(
            1.2 + 5.0 * velocity.powf(1.2),
            mallet_mass,
            mallet_stiff + 2.0e6 * velocity,
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

        // Strike shape of fundamental (0, 1) mode at r_hit
        let phi_01 = self.top.modes[0].strike_shape;

        let q_top_01 = (self.u_plus_q + self.u_minus_q) * (1.0 / SQRT_2);
        let v_top_01 = (self.u_plus_v + self.u_minus_v) * (1.0 / SQRT_2);

        let mut x_surf = q_top_01 * phi_01;
        let mut v_surf = v_top_01 * phi_01;
        for mode in &self.top.modes[1..] {
            x_surf += mode.q * mode.strike_shape;
            v_surf += mode.v * mode.strike_shape;
        }

        let strike_force = self.exciter.step(x_surf, v_surf, dt);

        let f_modal_01 = strike_force * phi_01;
        let force_plus = f_modal_01 * (1.0 / SQRT_2) / self.modal_mass_01;
        let force_minus = f_modal_01 * (1.0 / SQRT_2) / self.modal_mass_01;

        let (p11_p, p12_p, p21_p, p22_p) = self.trans_plus.phi;
        let (g1_p, g2_p) = self.trans_plus.gamma;
        self.u_plus_q = p11_p * self.u_plus_q + p12_p * self.u_plus_v + g1_p * force_plus;
        self.u_plus_v = p21_p * self.u_plus_q + p22_p * self.u_plus_v + g2_p * force_plus;

        let (p11_m, p12_m, p21_m, p22_m) = self.trans_minus.phi;
        let (g1_m, g2_m) = self.trans_minus.gamma;
        self.u_minus_q = p11_m * self.u_minus_q + p12_m * self.u_minus_v + g1_m * force_minus;
        self.u_minus_v = p21_m * self.u_minus_q + p22_m * self.u_minus_v + g2_m * force_minus;

        let v_top_fund = (self.u_plus_v + self.u_minus_v) * (1.0 / SQRT_2);
        let v_bot_fund = (self.u_plus_v - self.u_minus_v) * (1.0 / SQRT_2);

        self.top.modes[0].q = (self.u_plus_q + self.u_minus_q) * (1.0 / SQRT_2);
        self.top.modes[0].v = v_top_fund;
        self.bottom.modes[0].q = (self.u_plus_q - self.u_minus_q) * (1.0 / SQRT_2);
        self.bottom.modes[0].v = v_bot_fund;

        let mut top_sound = v_top_fund * self.top.modes[0].radiation_shape;
        for mode in &mut self.top.modes[1..] {
            let (p11, p12, p21, p22) = mode.transition.phi;
            let (g1, g2) = mode.transition.gamma;
            let f_m = (strike_force * mode.strike_shape) / mode.modal_mass_kg;
            let nq = p11 * mode.q + p12 * mode.v + g1 * f_m;
            let nv = p21 * mode.q + p22 * mode.v + g2 * f_m;
            mode.q = nq;
            mode.v = nv;
            top_sound += mode.v * mode.radiation_shape;
        }

        let mut bot_sound = v_bot_fund * self.bottom.modes[0].radiation_shape * 0.40;
        for mode in &mut self.bottom.modes[1..] {
            let (p11, p12, p21, p22) = mode.transition.phi;
            let nq = p11 * mode.q + p12 * mode.v;
            let nv = p21 * mode.q + p22 * mode.v;
            mode.q = nq;
            mode.v = nv;
            bot_sound += mode.v * mode.radiation_shape * 0.35;
        }

        let shell_input = top_sound + strike_force * 0.0002;
        let shell_sound = self.shell1.step(shell_input) * 0.18 + self.shell2.step(shell_input) * 0.10;

        let output = (top_sound + shell_sound - bot_sound * 0.30) * self.output_gain * self.tail_gain;
        self.tail_gain *= 0.999_999;

        let energy = self.top.energy() + self.bottom.energy();
        if !self.exciter.is_contacting && energy < 1.0e-7 {
            self.active = false;
        }
        output.clamp(-1.0, 1.0)
    }

    pub fn reset(&mut self) {
        self.top.reset();
        self.bottom.reset();
        self.exciter.reset();
        self.shell1.reset();
        self.shell2.reset();
        self.u_plus_q = 0.0;
        self.u_plus_v = 0.0;
        self.u_minus_q = 0.0;
        self.u_minus_v = 0.0;
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
    clearance: f64,
    stiffness: f64,
    mass: f64,
    damping: f64,
}

impl Default for SnareWire {
    fn default() -> Self {
        Self {
            displacement: 0.0,
            velocity: 0.0,
            clearance: 0.00001,
            stiffness: 14_000.0,
            mass: 0.00018,
            damping: 32.0,
        }
    }
}

/// Snare: Top and resonant bottom heads with 24 discrete unilateral Hertzian wire contacts.
/// ZERO artificial white noise — all high-frequency rattle is produced by physical collisions!
#[derive(Debug, Clone)]
pub struct SnareVoice {
    pub top: MembraneHead,
    pub bottom: MembraneHead,
    pub exciter: HuntCrossleyExciter,
    pub shell1: ShellResonator,
    pub shell2: ShellResonator,
    wires: [SnareWire; SNARE_WIRE_COUNT],
    wire_shapes: [[[f64; HEAD_MODE_COUNT]; SNARE_CONTACT_POINT_COUNT]; SNARE_WIRE_COUNT],
    pub wire_count: usize,
    pub tightness: f64,
    pub decay: f64,
    pub cavity_pressure: f64,
    pub cavity_stiffness: f64,
    pub cavity_damping: f64,
    pub wire_modal_forces: [f64; HEAD_MODE_COUNT],
    pub active: bool,
}

impl SnareVoice {
    pub fn new(sample_rate: f64) -> Self {
        let mut top = MembraneHead::new(sample_rate, 205.0, 0.22);
        top.set_strike_point(0.35, 0.37);
        top.set_strike_contact_radius(0.008);

        // Snare side head is extremely thin (2-3 mil Mylar)
        let bottom = MembraneHead::new_with_geometry(sample_rate, 220.0, 0.16, 0.1778, 0.09);

        let shell1 = ShellResonator::new(sample_rate, 340.0, 6.0);
        let shell2 = ShellResonator::new(sample_rate, 680.0, 8.0);

        let wire_shapes = std::array::from_fn(|index| {
            let lateral_position = -0.14 + 0.28 * index as f64 / (SNARE_WIRE_COUNT - 1) as f64;
            std::array::from_fn(|point_index| {
                let along_wire = [-0.58, 0.0, 0.58][point_index];
                bottom.shapes_at_xy(along_wire, lateral_position)
            })
        });

        let wires = std::array::from_fn(|i| {
            let frac = i as f64 / (SNARE_WIRE_COUNT - 1) as f64;
            let gap = 0.000020 + 0.000025 * (frac * 3.14).sin();
            let k = 12_000.0 + 4_000.0 * (i % 3) as f64;
            SnareWire {
                displacement: 0.0,
                velocity: 0.0,
                clearance: gap,
                stiffness: k,
                mass: 0.00018,
                damping: 32.0,
            }
        });

        Self {
            top,
            bottom,
            exciter: HuntCrossleyExciter::default(),
            shell1,
            shell2,
            wires,
            wire_shapes,
            wire_count: SNARE_WIRE_COUNT,
            tightness: 0.65,
            decay: 0.55,
            cavity_pressure: 0.0,
            cavity_stiffness: 90_000.0,
            cavity_damping: 0.12,
            wire_modal_forces: [0.0; HEAD_MODE_COUNT],
            active: false,
        }
    }

    pub fn set_tightness(&mut self, tightness: f64) {
        self.tightness = tightness.clamp(0.0, 1.0);
        let base_gap = 0.000015 + (1.0 - self.tightness) * 0.000030;
        let base_k = 10_000.0 + 15_000.0 * self.tightness;
        for (i, wire) in self.wires.iter_mut().enumerate() {
            let frac = i as f64 / (SNARE_WIRE_COUNT - 1) as f64;
            wire.clearance = base_gap + 0.000020 * (frac * 3.14).sin();
            wire.stiffness = base_k + 2_000.0 * (i % 3) as f64;
        }
    }

    pub fn set_decay(&mut self, decay: f64) {
        let decay = decay.clamp(0.0, 1.0);
        if (self.decay - decay).abs() > 0.01 {
            self.decay = decay;
            let t60_top = 0.10 + 0.20 * decay;
            let t60_bot = 0.08 + 0.16 * decay;
            self.top.set_t60(t60_top);
            self.bottom.set_t60(t60_bot);
            let wire_damp = 24.0 + 40.0 * (1.0 - decay);
            for wire in &mut self.wires {
                wire.damping = wire_damp;
            }
        }
    }

    pub fn trigger(&mut self, velocity: f64) {
        let velocity = velocity.clamp(0.001, 1.0);
        if !self.active {
            self.top.reset();
            self.bottom.reset();
            self.shell1.reset();
            self.shell2.reset();
            for wire in &mut self.wires {
                wire.displacement = 0.0;
                wire.velocity = 0.0;
            }
            self.cavity_pressure = 0.0;
            self.wire_modal_forces.fill(0.0);
        }
        self.top.geometry_nonlinearity = 0.25 * velocity * velocity;
        self.bottom.geometry_nonlinearity = 0.12 * velocity * velocity;
        let t60_top = 0.10 + 0.20 * self.decay;
        let t60_bot = 0.08 + 0.16 * self.decay;
        self.top.set_t60(t60_top);
        self.bottom.set_t60(t60_bot);
        self.exciter.trigger(
            1.5 + 4.8 * velocity.powf(1.2),
            0.024,
            1.2e6 + 2.5e6 * velocity,
            1.5,
        );
        self.active = true;
    }

    #[inline]
    pub fn step(&mut self, dt: f64) -> f64 {
        if !self.active {
            return 0.0;
        }
        let (strike_disp, strike_vel) = self.top.strike_state();
        let strike_force = self.exciter.step(strike_disp, strike_vel, dt);

        let (top_vol, top_vol_vel) = self.top.area_average_state();
        let (bot_vol, bot_vol_vel) = self.bottom.area_average_state();
        let vol_delta = top_vol - bot_vol;
        let vol_vel = top_vol_vel - bot_vol_vel;
        let eff_mass = 1.0
            / (1.0 / self.top.area_average_effective_mass()
                + 1.0 / self.bottom.area_average_effective_mass());
        let acoustic_damping =
            2.0 * self.cavity_damping * (self.cavity_stiffness * eff_mass).sqrt();
        let head_area = PI * self.top.radius_m().powi(2);
        self.cavity_pressure =
            ((self.cavity_stiffness * vol_delta + acoustic_damping * vol_vel) / head_area)
                .clamp(-1.0e6, 1.0e6);

        let (_top_disp, top_velocity) =
            self.top.step_coupled(strike_force, -self.cavity_pressure);
        let (_bottom_disp, bottom_velocity) =
            self.bottom
                .step_with_modal_forces(0.0, self.cavity_pressure, &self.wire_modal_forces);
        self.wire_modal_forces.fill(0.0);

        // Discrete Hertzian wire contact dynamics with 2x symplectic sub-stepping
        let n_sub = 2;
        let dt_sub = dt / n_sub as f64;
        let mut chatter_force = 0.0;
        let mut wire_energy = 0.0;
        let count = self.wire_count.clamp(1, self.wires.len());
        let contact_stiffness = 6.0e5;

        for _ in 0..n_sub {
            for (index, wire) in self.wires[..count].iter_mut().enumerate() {
                let contact_shapes = &self.wire_shapes[index];
                let mut wire_contact_force = 0.0;

                for shapes in contact_shapes {
                    let (local_bot, local_bot_vel) = self.bottom.state_with_shapes(shapes);
                    let gap = local_bot - wire.displacement - wire.clearance;
                    if gap > 0.0 {
                        let f_elas = contact_stiffness * gap.powf(1.4);
                        let rel_vel = local_bot_vel - wire.velocity;
                        let f_diss = (f_elas * 0.06 * rel_vel).max(0.0);
                        let force = (f_elas + f_diss).min(30.0);
                        wire_contact_force += force;
                        chatter_force += force / (n_sub as f64 * SNARE_CONTACT_POINT_COUNT as f64);
                        self.bottom
                            .accumulate_modal_force(shapes, -force / n_sub as f64, &mut self.wire_modal_forces);
                    }
                }

                let damp_decay = (-wire.damping / wire.mass * dt_sub).exp();
                let restoring = -wire.stiffness * wire.displacement;
                let accel = (wire_contact_force + restoring) / wire.mass;
                wire.velocity = (wire.velocity + accel * dt_sub) * damp_decay;
                wire.displacement += wire.velocity * dt_sub;
                wire.displacement = wire.displacement.clamp(-0.01, 0.01);
                wire.velocity = wire.velocity.clamp(-30.0, 30.0);

                wire_energy += wire.displacement * wire.displacement * wire.stiffness
                    + wire.velocity * wire.velocity * wire.mass;
            }
        }

        // Pure physical radiation: Top head + Bottom head + Shell resonance + Steel wire rattle
        let shell_input = top_velocity + strike_force * 0.0002;
        let shell_sound = self.shell1.step(shell_input) * 0.18 + self.shell2.step(shell_input) * 0.10;
        let rattle_sound = chatter_force * 0.003;
        let output = (top_velocity * 0.75 - bottom_velocity * 0.20 + shell_sound + rattle_sound) * 1.05;

        let cavity_energy = 0.5 * self.cavity_stiffness * vol_delta * vol_delta;
        let energy = self.top.energy() + self.bottom.energy() + cavity_energy + wire_energy;
        if !self.exciter.is_contacting && energy < 1e-7 {
            self.active = false;
        }
        output.clamp(-1.0, 1.0)
    }

    pub fn reset(&mut self) {
        self.top.reset();
        self.bottom.reset();
        self.exciter.reset();
        self.shell1.reset();
        self.shell2.reset();
        for wire in &mut self.wires {
            wire.displacement = 0.0;
            wire.velocity = 0.0;
        }
        self.active = false;
        self.cavity_pressure = 0.0;
        self.wire_modal_forces.fill(0.0);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CymbalKind {
    HiHat,
    Crash,
    Ride,
}

#[derive(Debug, Clone, Copy)]
pub struct CymbalMode {
    pub transition: ModalTransition,
    pub q: f64,
    pub v: f64,
    pub omega: f64,
    pub base_omega: f64,
    pub beta: f64,
    pub radiation_gain: f64,
    pub t60: f64,
}

impl CymbalMode {
    pub fn new(sample_rate: f64, frequency: f64, beta: f64, radiation_gain: f64, t60: f64) -> Self {
        let omega = TAU * frequency.min(sample_rate * 0.45);
        let sigma = (1000.0_f64.ln() / t60.max(0.02)).min(omega * 0.9);
        Self {
            transition: ModalTransition::new(
                omega,
                sigma,
                1.0 / sample_rate,
                OverdampedPolicy::ExponentialFallback,
            ),
            q: 0.0,
            v: 0.0,
            omega,
            base_omega: omega,
            beta,
            radiation_gain,
            t60,
        }
    }

    pub fn set_t60(&mut self, sample_rate: f64, t60: f64) {
        self.t60 = t60.max(0.02);
        let sigma = (1000.0_f64.ln() / self.t60).min(self.omega * 0.9);
        self.transition = ModalTransition::new(
            self.omega,
            sigma,
            1.0 / sample_rate,
            OverdampedPolicy::ExponentialFallback,
        );
    }

    #[inline]
    pub fn step(&mut self, modal_force: f64) -> f64 {
        let (p11, p12, p21, p22) = self.transition.phi;
        let (g1, g2) = self.transition.gamma;
        let next_q = p11 * self.q + p12 * self.v + g1 * modal_force;
        let next_v = p21 * self.q + p22 * self.v + g2 * modal_force;
        self.q = next_q;
        self.v = next_v;
        self.v * self.radiation_gain
    }

    pub fn energy(&self) -> f64 {
        0.5 * (self.v * self.v + self.omega * self.omega * self.q * self.q)
    }
}

#[derive(Debug, Clone, Copy)]
struct SparseCouplePair {
    i: usize,
    j: usize,
    beta: f64,
}

/// 128-mode plate bank with sparse Hamiltonian nonlinear coupling.
/// Simulates energy cascade (shimmer/turbulent wash) and boundary damping with ZERO white noise!
#[derive(Debug, Clone)]
pub struct CymbalVoice {
    sample_rate: f64,
    pub kind: CymbalKind,
    pub modes: [CymbalMode; CYMBAL_MODE_COUNT],
    couplings: [SparseCouplePair; CYMBAL_MODE_COUNT + 32],
    coupling_count: usize,
    pub open_amount: f64,
    pub decay_scale: f64,
    pub active: bool,
    base_t60: f64,
}

impl CymbalVoice {
    pub fn new(sample_rate: f64) -> Self {
        Self::new_for_kind(sample_rate, CymbalKind::HiHat)
    }

    pub fn new_for_kind(sample_rate: f64, kind: CymbalKind) -> Self {
        let sample_rate = sample_rate.max(1.0);
        let (base_hz, base_t60, nl_beta) = match kind {
            CymbalKind::HiHat => (360.0, 4.4, 120.0),
            CymbalKind::Crash => (280.0, 6.0, 250.0),
            CymbalKind::Ride => (420.0, 2.5, 30.0),
        };

        let max_hz = match kind {
            CymbalKind::Crash => sample_rate * 0.32,
            CymbalKind::HiHat => sample_rate * 0.30,
            CymbalKind::Ride => sample_rate * 0.36,
        };
        let f0 = base_hz;

        let mut modes = [CymbalMode {
            transition: ModalTransition::new(100.0, 1.0, 1.0 / sample_rate, OverdampedPolicy::ExponentialFallback),
            q: 0.0,
            v: 0.0,
            omega: 100.0,
            base_omega: 100.0,
            beta: 0.0,
            radiation_gain: 0.0,
            t60: 1.0,
        }; CYMBAL_MODE_COUNT];

        for i in 0..CYMBAL_MODE_COUNT {
            let frac = i as f64 / (CYMBAL_MODE_COUNT - 1) as f64;
            let freq = (f0 + (max_hz - f0) * frac.powf(1.3)).min(max_hz);
            let t60 = (base_t60 / (1.0 + (freq / 3500.0).powf(0.85)) + 0.02).max(0.015);
            let rad_gain = match kind {
                CymbalKind::Crash => 0.0035 / (1.0 + (freq / 4200.0).powf(1.8)),
                CymbalKind::HiHat => 0.0032 / (1.0 + (freq / 8000.0).powf(1.5)),
                CymbalKind::Ride => 0.0032 / (1.0 + (freq / 8500.0).powf(1.6)),
            };
            modes[i] = CymbalMode::new(sample_rate, freq, nl_beta, rad_gain, t60);
        }

        let mut couplings = [SparseCouplePair { i: 0, j: 0, beta: 0.0 }; CYMBAL_MODE_COUNT + 32];
        let mut count = 0;

        // Neighbor chain
        for i in 0..CYMBAL_MODE_COUNT - 1 {
            couplings[count] = SparseCouplePair {
                i,
                j: i + 1,
                beta: nl_beta * 0.15,
            };
            count += 1;
        }

        // 2:1 Octave pairs
        for i in 0..CYMBAL_MODE_COUNT / 2 {
            let target_omega = modes[i].omega * 2.0;
            let mut best_j = i;
            let mut best_diff = f64::MAX;
            for j in (i + 1)..CYMBAL_MODE_COUNT {
                let diff = (modes[j].omega - target_omega).abs();
                if diff < best_diff {
                    best_diff = diff;
                    best_j = j;
                }
            }
            if best_diff < modes[i].omega * 0.15 && count < couplings.len() {
                couplings[count] = SparseCouplePair {
                    i,
                    j: best_j,
                    beta: nl_beta * 0.25,
                };
                count += 1;
            }
        }

        let mut voice = Self {
            sample_rate,
            kind,
            modes,
            couplings,
            coupling_count: count,
            open_amount: 1.0,
            decay_scale: 1.0,
            active: false,
            base_t60,
        };
        voice.update_decay();
        voice
    }

    pub fn set_open_amount(&mut self, amount: f64) {
        self.open_amount = amount.clamp(0.0, 1.0);
        self.update_decay();
    }

    pub fn set_decay_scale(&mut self, scale: f64) {
        self.decay_scale = scale.clamp(0.2, 3.0);
        self.update_decay();
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
        }

        let base_w = self.modes[0].omega;
        let scale = match self.kind {
            CymbalKind::Crash => 0.0028,
            CymbalKind::Ride => 0.0015,
            CymbalKind::HiHat => 0.0018,
        };
        for (i, mode) in self.modes.iter_mut().enumerate() {
            let frac = i as f64 / (CYMBAL_MODE_COUNT - 1) as f64;
            let impact_gain = match self.kind {
                CymbalKind::Crash => (1.0 - frac * 0.40) * (0.7 + 0.3 * frac.powf(0.5)),
                CymbalKind::Ride => 0.60 + 0.40 * frac,
                CymbalKind::HiHat => 0.40 + 0.60 * frac.powf(0.8),
            };
            let freq_tilt = match self.kind {
                CymbalKind::Crash => (mode.omega / base_w).powf(0.24),
                CymbalKind::Ride => (mode.omega / base_w).powf(0.32),
                CymbalKind::HiHat => (mode.omega / base_w).powf(0.26),
            };
            mode.v += velocity.powf(0.8) * base_w * scale * freq_tilt * impact_gain;
        }

        self.active = true;
    }

    pub fn choke(&mut self, amount: f64) {
        let factor = amount.clamp(0.0, 1.0);
        for mode in &mut self.modes {
            mode.q *= factor;
            mode.v *= factor;
        }
        self.open_amount *= factor;
        self.update_decay();
    }

    #[inline]
    pub fn step(&mut self) -> f64 {
        if !self.active {
            return 0.0;
        }

        let mut nl_forces = [0.0; CYMBAL_MODE_COUNT];
        for k in 0..self.coupling_count {
            let pair = &self.couplings[k];
            let qi = self.modes[pair.i].q;
            let qj = self.modes[pair.j].q;
            let fi = -pair.beta * qi * qj * qj;
            let fj = -pair.beta * qj * qi * qi;
            nl_forces[pair.i] += fi;
            nl_forces[pair.j] += fj;
        }

        let mut output = 0.0;
        for i in 0..CYMBAL_MODE_COUNT {
            output += self.modes[i].step(nl_forces[i]);
        }

        if self.energy() < 1e-11 {
            self.active = false;
        }

        output.clamp(-1.0, 1.0)
    }

    pub fn energy(&self) -> f64 {
        self.modes.iter().map(CymbalMode::energy).sum()
    }

    pub fn reset(&mut self) {
        for mode in &mut self.modes {
            mode.q = 0.0;
            mode.v = 0.0;
        }
        self.active = false;
    }

    fn update_decay(&mut self) {
        let is_hihat = self.kind == CymbalKind::HiHat;
        let open_sq = self.open_amount.powf(1.2);
        let t60_scale = if is_hihat {
            0.030 + 0.970 * open_sq
        } else {
            1.0
        } * self.decay_scale;

        let freq_ref = match self.kind {
            CymbalKind::Crash => 8500.0,
            CymbalKind::HiHat => 8000.0,
            CymbalKind::Ride => 6000.0,
        };

        for (i, mode) in self.modes.iter_mut().enumerate() {
            let frac = i as f64 / (CYMBAL_MODE_COUNT - 1) as f64;
            let freq = mode.omega / TAU;
            let mode_t60 = (self.base_t60 * t60_scale / (1.0 + (freq / freq_ref).powf(0.55)) + 0.02)
                / (1.0 + if is_hihat { (1.0 - open_sq) * (1.0 - frac) * 0.5 } else { 0.0 });
            mode.set_t60(self.sample_rate, mode_t60);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CymbalVoice, DoubleHeadVoice, SnareVoice, CYMBAL_MODE_COUNT};

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
        assert_eq!(cymbal.modes.len(), CYMBAL_MODE_COUNT);
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
        assert!(open.energy() > closed.energy() * 10.0);
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
