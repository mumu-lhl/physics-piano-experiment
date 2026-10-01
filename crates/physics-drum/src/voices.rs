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

pub const CYMBAL_MODE_COUNT: usize = 256;
pub const MAX_CYMBAL_NEIGHBORS: usize = 4;

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
    dc_state: f64,
    dc_prev_in: f64,
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

        let mut bottom = MembraneHead::new_with_geometry(
            sample_rate,
            fundamental_hz * 1.04,
            actual_t60 * 0.85,
            head_radius_m,
            surface_density_kg_m2 * 0.88,
        );

        if is_kick {
            for mode in &mut top.modes[1..] {
                let freq = mode.frequency;
                let kick_t60 = (0.50 / (1.0 + (freq / 110.0).powf(1.8)) + 0.015).max(0.012);
                mode.set_t60(sample_rate, kick_t60);
                mode.radiation_shape *= 0.25 / (1.0 + mode.angular_order as f64 * 0.8 + (mode.radial_root / 2.4048) * 0.4);
            }
            for mode in &mut bottom.modes[1..] {
                let freq = mode.frequency;
                let kick_t60 = (0.40 / (1.0 + (freq / 110.0).powf(1.8)) + 0.012).max(0.010);
                mode.set_t60(sample_rate, kick_t60);
                mode.radiation_shape *= 0.20 / (1.0 + mode.angular_order as f64 * 0.8 + (mode.radial_root / 2.4048) * 0.4);
            }
        }

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

        let output_gain = if is_kick { 0.22 } else { 0.20 };

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
            dc_state: 0.0,
            dc_prev_in: 0.0,
        }
    }

    pub fn trigger(&mut self, velocity: f64) {
        let velocity = velocity.clamp(0.001, 1.0);
        if !self.active {
            self.reset();
        }
        self.top.geometry_nonlinearity = 0.0;
        self.bottom.geometry_nonlinearity = 0.0;
        let mallet_mass = if self.is_kick { 0.065 } else { 0.025 };
        let mallet_stiff = if self.is_kick { 1.2e6 } else { 2.2e6 };
        let strike_vel = if self.is_kick {
            5.0 * velocity.powf(1.15)
        } else {
            5.2 * velocity.powf(1.2)
        };
        self.exciter.trigger(
            strike_vel,
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

        let shell_input = top_sound + strike_force * 0.0001;
        let shell_sound = self.shell1.step(shell_input) * 0.18 + self.shell2.step(shell_input) * 0.10;

        let raw_output = (top_sound + shell_sound - bot_sound * 0.30) * self.output_gain * self.tail_gain;
        self.tail_gain *= 0.999_999;

        // 1st-order DC blocking filter at ~22 Hz
        let dc_alpha = (-TAU * 22.0 * dt).exp();
        let output = dc_alpha * (self.dc_state + raw_output - self.dc_prev_in);
        self.dc_prev_in = raw_output;
        self.dc_state = output;

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
        self.dc_state = 0.0;
        self.dc_prev_in = 0.0;
        self.active = false;
        self.tail_gain = 1.0;
    }
}

pub type KickVoice = DoubleHeadVoice;
pub type TomVoice = DoubleHeadVoice;

const SNARE_WIRE_COUNT: usize = 16;

#[derive(Debug, Clone, Copy)]
struct SnareWire {
    displacement: f64,
    velocity: f64,
    gap: f64,
    stiffness: f64,
    mass: f64,
    damping: f64,
    contact_k: f64,
}

impl Default for SnareWire {
    fn default() -> Self {
        Self {
            displacement: 0.0,
            velocity: 0.0,
            gap: 0.00003,
            stiffness: 14_000.0,
            mass: 0.00025,
            damping: 18.0,
            contact_k: 8.0e5,
        }
    }
}

/// Snare: Top and resonant bottom heads with 16 discrete unilateral Hertzian wire contacts
/// and analytical (u+, u-) cavity eigenmodes.
/// ZERO artificial white noise — all high-frequency rattle is produced by physical collisions!
#[derive(Debug, Clone)]
pub struct SnareVoice {
    sample_rate: f64,
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
    wires: [SnareWire; SNARE_WIRE_COUNT],
    wire_shapes: [[f64; HEAD_MODE_COUNT]; SNARE_WIRE_COUNT],
    pub tightness: f64,
    pub decay: f64,
    pub wire_modal_forces: [f64; HEAD_MODE_COUNT],
    pub active: bool,
    wire_filter_lp: f64,
    wire_filter_hp: f64,
    wire_filter_prev: f64,
    dc_state: f64,
    dc_prev_in: f64,
}

impl SnareVoice {
    pub fn new(sample_rate: f64) -> Self {
        let sample_rate = sample_rate.max(1.0);
        let radius_m = 0.1778; // 14-inch snare
        let f0_top = 210.0;    // Batter head fundamental ~210 Hz
        let f0_bottom = 260.0; // Ultra-thin snare side tuned higher ~260 Hz
        let dt = 1.0 / sample_rate;

        // Top head mass: coated head (~0.28 kg/m^2)
        let mut top = MembraneHead::new_with_geometry(sample_rate, f0_top, 0.35, radius_m, 0.28);
        top.set_strike_point(0.35, 0.0);
        top.set_strike_contact_radius(radius_m * 0.08);

        // Calibrate top head higher mode damping
        for mode in &mut top.modes[1..] {
            let freq = mode.frequency;
            let mode_t60 = (0.70 / (1.0 + (freq / 350.0).powf(1.6)) + 0.02).max(0.015);
            mode.set_t60(sample_rate, mode_t60);
        }

        // Bottom head mass: ultra-thin snare-side head (~0.09 kg/m^2)
        let mut bottom = MembraneHead::new_with_geometry(sample_rate, f0_bottom, 0.25, radius_m, 0.09);
        for mode in &mut bottom.modes[1..] {
            let freq = mode.frequency;
            let mode_t60 = (0.50 / (1.0 + (freq / 450.0).powf(1.5)) + 0.015).max(0.010);
            mode.set_t60(sample_rate, mode_t60);
        }

        let total_top_mass = 0.28 * PI * radius_m * radius_m;
        let modal_mass_01 = total_top_mass * 0.25;

        // Coupled cavity eigenmodes for fundamental (0, 1):
        let omega_plus = TAU * f0_top;
        let omega_minus = TAU * (f0_top * 1.28);
        let trans_plus = ModalTransition::new(omega_plus, 1000.0_f64.ln() / 0.55, dt, OverdampedPolicy::ExponentialFallback);
        let trans_minus = ModalTransition::new(omega_minus, 1000.0_f64.ln() / 0.35, dt, OverdampedPolicy::ExponentialFallback);

        let shell1 = ShellResonator::new(sample_rate, 340.0, 6.0);
        let shell2 = ShellResonator::new(sample_rate, 680.0, 8.0);

        // 16 wire contact positions along diameter: x from -0.80 to +0.80, y = 0
        let wire_shapes = std::array::from_fn(|i| {
            let pos_x = -0.80 + 1.60 * (i as f64) / 15.0;
            bottom.shapes_at_xy(pos_x, 0.0)
        });

        let wires = std::array::from_fn(|i| {
            let norm_i = (i as f64 - 7.5) / 7.5;
            let gap = (0.000030 + 0.000040 * norm_i * norm_i) * (1.0 + 0.2 * (i as f64 * 1.7).sin());
            SnareWire {
                displacement: 0.0,
                velocity: 0.0,
                gap,
                mass: 0.00025,
                stiffness: 14_000.0 + 2_000.0 * (i as f64 * 2.3).cos(),
                damping: 18.0,
                contact_k: 8.0e5,
            }
        });

        Self {
            sample_rate,
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
            wires,
            wire_shapes,
            tightness: 0.65,
            decay: 0.55,
            wire_modal_forces: [0.0; HEAD_MODE_COUNT],
            active: false,
            wire_filter_lp: 0.0,
            wire_filter_hp: 0.0,
            wire_filter_prev: 0.0,
            dc_state: 0.0,
            dc_prev_in: 0.0,
        }
    }

    pub fn set_tightness(&mut self, tightness: f64) {
        self.tightness = tightness.clamp(0.0, 1.0);
        let base_gap = 0.000015 + (1.0 - self.tightness) * 0.000030;
        let base_k = 10_000.0 + 15_000.0 * self.tightness;
        for (i, wire) in self.wires.iter_mut().enumerate() {
            let norm_i = (i as f64 - 7.5) / 7.5;
            wire.gap = base_gap + 0.000020 * norm_i * norm_i;
            wire.stiffness = base_k + 2_000.0 * (i as f64 * 2.3).cos();
        }
    }

    pub fn set_decay(&mut self, decay: f64) {
        let decay = decay.clamp(0.0, 1.0);
        if (self.decay - decay).abs() > 0.01 {
            self.decay = decay;
            let t60_p = 0.25 + 0.50 * decay;
            let t60_m = 0.15 + 0.35 * decay;
            let dt = 1.0 / self.sample_rate;
            self.trans_plus = ModalTransition::new(TAU * 210.0, 1000.0_f64.ln() / t60_p, dt, OverdampedPolicy::ExponentialFallback);
            self.trans_minus = ModalTransition::new(TAU * (210.0 * 1.28), 1000.0_f64.ln() / t60_m, dt, OverdampedPolicy::ExponentialFallback);

            let wire_damp = 12.0 + 24.0 * (1.0 - decay);
            for wire in &mut self.wires {
                wire.damping = wire_damp;
            }
        }
    }

    pub fn trigger(&mut self, velocity: f64) {
        let velocity = velocity.clamp(0.001, 1.0);
        if !self.active {
            self.reset();
        }
        self.top.geometry_nonlinearity = 0.0;
        self.bottom.geometry_nonlinearity = 0.0;
        let strike_vel = 5.2 * velocity.powf(1.2);
        self.exciter.trigger(
            strike_vel,
            0.022,
            1.2e7 + 2.5e6 * velocity,
            1.5,
        );
        self.active = true;
    }

    #[inline]
    pub fn step(&mut self, dt: f64) -> f64 {
        if !self.active {
            return 0.0;
        }

        // 1. Top head strike point displacement & velocity
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

        // 2. Advance fundamental coupled cavity eigenmodes
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
        let q_bot_fund = (self.u_plus_q - self.u_minus_q) * (1.0 / SQRT_2);
        let v_bot_fund = (self.u_plus_v - self.u_minus_v) * (1.0 / SQRT_2);

        self.top.modes[0].q = (self.u_plus_q + self.u_minus_q) * (1.0 / SQRT_2);
        self.top.modes[0].v = v_top_fund;
        self.bottom.modes[0].q = q_bot_fund;
        self.bottom.modes[0].v = v_bot_fund;

        // 3. Advance top head higher modes
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

        // 4. Snare wire contacts at 16 points along bottom head
        self.wire_modal_forces.fill(0.0);
        let mut snare_rattle_acoustic = 0.0;

        for (w, wire) in self.wires.iter_mut().enumerate() {
            let shapes = &self.wire_shapes[w];
            let (w_bot, v_bot) = self.bottom.state_with_shapes(shapes);
            let delta = w_bot - wire.displacement - wire.gap;

            let contact_force = if delta > 0.0 {
                let v_rel = v_bot - wire.velocity;
                let elastic = wire.contact_k * delta.powf(1.4);
                let dissipative = 0.08 * elastic * v_rel;
                (elastic + dissipative).max(0.0).min(30.0)
            } else {
                0.0
            };

            let restoring = -wire.stiffness * wire.displacement - wire.damping * wire.velocity;
            wire.velocity += (contact_force + restoring) / wire.mass * dt;
            wire.displacement += wire.velocity * dt;
            wire.displacement = wire.displacement.clamp(-0.005, 0.005);
            wire.velocity = wire.velocity.clamp(-20.0, 20.0);

            snare_rattle_acoustic += contact_force * 0.00035;

            // Bilateral back-reaction onto bottom head modes
            self.bottom.accumulate_modal_force(shapes, -contact_force, &mut self.wire_modal_forces);
        }

        // 5. Advance bottom head higher modes with wire back-reaction
        let mut bot_sound = v_bot_fund * self.bottom.modes[0].radiation_shape * 0.40;
        for (idx, mode) in self.bottom.modes[1..].iter_mut().enumerate() {
            let (p11, p12, p21, p22) = mode.transition.phi;
            let (g1, g2) = mode.transition.gamma;
            let f_m = self.wire_modal_forces[idx + 1] / mode.modal_mass_kg;
            let nq = p11 * mode.q + p12 * mode.v + g1 * f_m;
            let nv = p21 * mode.q + p22 * mode.v + g2 * f_m;
            mode.q = nq;
            mode.v = nv;
            bot_sound += mode.v * mode.radiation_shape * 0.35;
        }

        // 6. Shell resonance
        let shell_input = top_sound + strike_force * 0.0001;
        let shell_sound = self.shell1.step(shell_input) * 0.20 + self.shell2.step(shell_input) * 0.12;

        // 7. Wire acoustic radiation filter (800 Hz HP + 7 kHz LP)
        let wire_raw = snare_rattle_acoustic;
        self.wire_filter_lp += 0.45 * (wire_raw - self.wire_filter_lp);
        self.wire_filter_hp = 0.88 * (self.wire_filter_hp + self.wire_filter_lp - self.wire_filter_prev);
        self.wire_filter_prev = self.wire_filter_lp;
        let rattle_sound = self.wire_filter_hp;

        let raw_output = (top_sound + shell_sound + bot_sound * 0.35 + rattle_sound) * 0.22;

        // 8. DC Blocker (~20 Hz HP)
        let dc_alpha = (-TAU * 20.0 * dt).exp();
        let output = dc_alpha * (self.dc_state + raw_output - self.dc_prev_in);
        self.dc_prev_in = raw_output;
        self.dc_state = output;

        let energy = self.top.energy() + self.bottom.energy();
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
        self.u_plus_q = 0.0;
        self.u_plus_v = 0.0;
        self.u_minus_q = 0.0;
        self.u_minus_v = 0.0;
        for wire in &mut self.wires {
            wire.displacement = 0.0;
            wire.velocity = 0.0;
        }
        self.wire_filter_lp = 0.0;
        self.wire_filter_hp = 0.0;
        self.wire_filter_prev = 0.0;
        self.dc_state = 0.0;
        self.dc_prev_in = 0.0;
        self.active = false;
        self.wire_modal_forces.fill(0.0);
    }

    pub fn energy(&self) -> f64 {
        self.top.energy() + self.bottom.energy()
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
    pub t60: f64,
    pub modal_mass: f64,
    pub strike_gain: f64,
    pub radiation_gain: f64,
    pub neighbor_count: usize,
    pub neighbors: [usize; MAX_CYMBAL_NEIGHBORS],
    pub coupling_weights: [f64; MAX_CYMBAL_NEIGHBORS],
}

impl CymbalMode {
    pub fn new(
        sample_rate: f64,
        frequency: f64,
        modal_mass: f64,
        strike_gain: f64,
        radiation_gain: f64,
        t60: f64,
    ) -> Self {
        let omega = TAU * frequency.min(sample_rate * 0.44);
        let sigma = (1000.0_f64.ln() / t60.max(0.008)).min(omega * 0.9);
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
            t60,
            modal_mass,
            strike_gain,
            radiation_gain,
            neighbor_count: 0,
            neighbors: [0; MAX_CYMBAL_NEIGHBORS],
            coupling_weights: [0.0; MAX_CYMBAL_NEIGHBORS],
        }
    }

    #[inline(always)]
    pub fn energy(&self) -> f64 {
        0.5 * (self.v * self.v + self.omega * self.omega * self.q * self.q) * self.modal_mass
    }
}

/// 256-mode bronze plate bank with sparse Hamiltonian nonlinear coupling.
/// Replicates the physical bronze plate dispersion law, micro-detuned inharmonics,
/// finite mallet contact duration, and acoustic radiation filtering with ZERO white noise!
#[derive(Debug, Clone)]
pub struct CymbalVoice {
    sample_rate: f64,
    pub kind: CymbalKind,
    pub modes: Box<[CymbalMode; CYMBAL_MODE_COUNT]>,
    pub exciter: HuntCrossleyExciter,
    pub open_amount: f64,
    pub decay_scale: f64,
    pub active: bool,
    base_t60: f64,
    nonlinear_forces: [f64; CYMBAL_MODE_COUNT],
    hp_state: f64,
    hp_prev_in: f64,
    output_scale: f64,
}

impl CymbalVoice {
    pub fn new(sample_rate: f64) -> Self {
        Self::new_for_kind(sample_rate, CymbalKind::HiHat)
    }

    pub fn new_for_kind(sample_rate: f64, kind: CymbalKind) -> Self {
        let sample_rate = sample_rate.max(1.0);
        let (base_hz, base_t60, coupling_strength, output_scale) = match kind {
            CymbalKind::HiHat => (380.0, 2.5, 3.0e4, 0.40),
            CymbalKind::Crash => (280.0, 5.5, 4.5e4, 0.38),
            CymbalKind::Ride => (440.0, 4.0, 2.0e4, 0.35),
        };

        let dummy_mode = CymbalMode::new(sample_rate, base_hz, 0.0025, 1.0, 0.01, base_t60);
        let mut modes: Box<[CymbalMode; CYMBAL_MODE_COUNT]> =
            vec![dummy_mode; CYMBAL_MODE_COUNT].into_boxed_slice().try_into().unwrap();

        let effective_t60 = if kind == CymbalKind::HiHat {
            0.06 + (base_t60 - 0.06) * 1.0
        } else {
            base_t60
        };

        for i in 0..CYMBAL_MODE_COUNT {
            let norm = i as f64 / (CYMBAL_MODE_COUNT - 1) as f64;
            // Dispersion law of circular bronze plate with inharmonic micro-detuning
            let ratio = 1.0 + 8.5 * norm.powf(1.1) + 42.0 * norm.powf(1.6);
            let inharmonic_detune = 1.0 + 0.015 * (i as f64 * 3.7).sin();
            let freq = (base_hz * ratio * inharmonic_detune).min(sample_rate * 0.44);

            let t60 = (effective_t60 / (1.0 + (freq / 3500.0).powf(1.2)) + 0.015).max(0.008);
            let modal_mass = 0.0025 * (1.0 + 0.5 * norm);
            let strike_gain = 1.0 / (1.0 + (freq / 2500.0).powf(1.4));
            let radiation_gain = (freq / 1000.0).clamp(0.4, 2.8) / (CYMBAL_MODE_COUNT as f64).sqrt();

            modes[i] = CymbalMode::new(sample_rate, freq, modal_mass, strike_gain, radiation_gain, t60);
        }

        // Construct sparse coupling graph:
        // 1. Immediate frequency neighbors
        // 2. 2:1 internal resonance partner
        for i in 0..CYMBAL_MODE_COUNT {
            let mut neighbors = [0; MAX_CYMBAL_NEIGHBORS];
            let mut weights = [0.0; MAX_CYMBAL_NEIGHBORS];
            let mut count = 0;

            if i + 1 < CYMBAL_MODE_COUNT && count < MAX_CYMBAL_NEIGHBORS {
                neighbors[count] = i + 1;
                weights[count] = coupling_strength;
                count += 1;
            }
            if i > 0 && count < MAX_CYMBAL_NEIGHBORS {
                neighbors[count] = i - 1;
                weights[count] = coupling_strength;
                count += 1;
            }
            if i + 3 < CYMBAL_MODE_COUNT && count < MAX_CYMBAL_NEIGHBORS {
                neighbors[count] = i + 3;
                weights[count] = coupling_strength * 0.4;
                count += 1;
            }

            let target_omega = modes[i].omega * 2.0;
            if target_omega < sample_rate * 0.44 * TAU && count < MAX_CYMBAL_NEIGHBORS {
                let mut best_partner = None;
                let mut best_diff = f64::INFINITY;
                for j in (i + 1)..CYMBAL_MODE_COUNT {
                    let diff = (modes[j].omega - target_omega).abs();
                    if diff < best_diff {
                        best_diff = diff;
                        best_partner = Some(j);
                    }
                }
                if let Some(partner) = best_partner {
                    if best_diff < modes[i].omega * 0.15 {
                        neighbors[count] = partner;
                        weights[count] = coupling_strength * 0.7;
                        count += 1;
                    }
                }
            }

            modes[i].neighbor_count = count;
            modes[i].neighbors = neighbors;
            modes[i].coupling_weights = weights;
        }

        let mut voice = Self {
            sample_rate,
            kind,
            modes,
            exciter: HuntCrossleyExciter::default(),
            open_amount: 1.0,
            decay_scale: 1.0,
            active: false,
            base_t60,
            nonlinear_forces: [0.0; CYMBAL_MODE_COUNT],
            hp_state: 0.0,
            hp_prev_in: 0.0,
            output_scale,
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
            for mode in self.modes.iter_mut() {
                mode.q = 0.0;
                mode.v = 0.0;
            }
        }

        let strike_vel = (5.5 * velocity.powf(1.2)) * match self.kind {
            CymbalKind::Crash => 1.0,
            CymbalKind::Ride => 0.85,
            CymbalKind::HiHat => 0.80,
        };
        let (mass, stiffness) = match self.kind {
            CymbalKind::Crash => (0.022, 1.4e7),
            CymbalKind::Ride => (0.018, 2.2e7),
            CymbalKind::HiHat => (0.016, 1.8e7),
        };
        self.exciter.trigger(strike_vel, mass, stiffness, 1.5);
        self.active = true;
    }

    pub fn choke(&mut self, amount: f64) {
        let factor = amount.clamp(0.0, 1.0);
        for mode in self.modes.iter_mut() {
            mode.q *= factor * 0.15;
            mode.v *= factor * 0.15;
        }
        self.open_amount *= factor;
        self.update_decay();
    }

    #[inline]
    pub fn step(&mut self) -> f64 {
        if !self.active {
            return 0.0;
        }

        let dt = 1.0 / self.sample_rate;

        // 1. Strike surface displacement & velocity
        let mut x_surf = 0.0;
        let mut v_surf = 0.0;
        for mode in self.modes.iter() {
            x_surf += mode.q * mode.strike_gain;
            v_surf += mode.v * mode.strike_gain;
        }

        // Mallet exciter contact force
        let strike_force = self.exciter.step(x_surf, v_surf, dt);

        // 2. Conservative Hamiltonian sparse nonlinear coupling:
        // F_nl,i = - q_i * sum_{j in N(i)} beta_ij * q_j^2
        self.nonlinear_forces.fill(0.0);
        for i in 0..CYMBAL_MODE_COUNT {
            let q_i = self.modes[i].q;
            let count = self.modes[i].neighbor_count;
            let mut stiffness_shift = 0.0;
            for k in 0..count {
                let j = self.modes[i].neighbors[k];
                let weight = self.modes[i].coupling_weights[k];
                let q_j = self.modes[j].q;
                stiffness_shift += weight * (q_j * q_j);
            }
            self.nonlinear_forces[i] = -q_i * stiffness_shift;
        }

        // 3. Advance modal state
        let mut sound_out = 0.0;
        for (i, mode) in self.modes.iter_mut().enumerate() {
            let (p11, p12, p21, p22) = mode.transition.phi;
            let (g1, g2) = mode.transition.gamma;
            let total_force = strike_force * mode.strike_gain + self.nonlinear_forces[i];
            let force_over_m = total_force / mode.modal_mass;

            let next_q = p11 * mode.q + p12 * mode.v + g1 * force_over_m;
            let next_v = p21 * mode.q + p22 * mode.v + g2 * force_over_m;

            mode.q = next_q;
            mode.v = next_v;

            sound_out += mode.v * mode.radiation_gain;
        }

        // 4. High-pass radiation filter (320 Hz) to eliminate sub-audio IMD products
        let hp_alpha = (-TAU * 320.0 * dt).exp();
        let hp_out = hp_alpha * (self.hp_state + sound_out - self.hp_prev_in);
        self.hp_prev_in = sound_out;
        self.hp_state = hp_out;

        if !self.exciter.is_contacting && self.energy() < 1e-11 {
            self.active = false;
        }

        (hp_out * self.output_scale).clamp(-1.0, 1.0)
    }

    pub fn energy(&self) -> f64 {
        self.modes.iter().map(CymbalMode::energy).sum()
    }

    pub fn reset(&mut self) {
        for mode in self.modes.iter_mut() {
            mode.q = 0.0;
            mode.v = 0.0;
        }
        self.exciter.reset();
        self.hp_state = 0.0;
        self.hp_prev_in = 0.0;
        self.active = false;
    }

    fn update_decay(&mut self) {
        let is_hihat = self.kind == CymbalKind::HiHat;
        let open_sq = self.open_amount.powf(1.8);
        let effective_t60 = if is_hihat {
            0.06 + (self.base_t60 - 0.06) * open_sq
        } else {
            self.base_t60
        } * self.decay_scale;

        let dt = 1.0 / self.sample_rate;
        for mode in self.modes.iter_mut() {
            let freq = mode.omega / TAU;
            let t60 = (effective_t60 / (1.0 + (freq / 3500.0).powf(1.2)) + 0.015).max(0.008);
            let sigma = (1000.0_f64.ln() / t60).min(mode.omega * 0.9);
            mode.t60 = t60;
            mode.transition = ModalTransition::new(
                mode.omega,
                sigma,
                dt,
                OverdampedPolicy::ExponentialFallback,
            );
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
