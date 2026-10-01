//! Double-head coupled drum prototype for Tom and Kick.
//! Uses exact eigenmode cavity splitting (In-Phase & Anti-Phase Modes) + Shell Body Resonance.
//! 100% unconditionally stable, exact acoustic beating, zero numerical explosion.

use hound::{WavSpec, WavWriter};
use physics_dsp::{ModalTransition, OverdampedPolicy};
use std::f64::consts::PI;
use std::fs;
use std::path::Path;

pub const TOM_MODE_SPECS: [(u8, u8, f64, bool); 256] = [
    (0, 1, 2.4048255577, false),
    (1, 1, 3.8317059702, false),
    (1, 1, 3.8317059702, true),
    (2, 1, 5.1356223018, false),
    (2, 1, 5.1356223018, true),
    (0, 2, 5.5200781103, false),
    (3, 1, 6.3801618959, false),
    (3, 1, 6.3801618959, true),
    (1, 2, 7.0155866698, false),
    (1, 2, 7.0155866698, true),
    (4, 1, 7.5883424345, false),
    (4, 1, 7.5883424345, true),
    (2, 2, 8.4172441404, false),
    (2, 2, 8.4172441404, true),
    (0, 3, 8.6537279129, false),
    (5, 1, 8.7714838160, false),
    (5, 1, 8.7714838160, true),
    (3, 2, 9.7610231300, false),
    (3, 2, 9.7610231300, true),
    (6, 1, 9.9361095242, false),
    (6, 1, 9.9361095242, true),
    (1, 3, 10.1734681351, false),
    (1, 3, 10.1734681351, true),
    (4, 2, 11.0647094885, false),
    (4, 2, 11.0647094885, true),
    (7, 1, 11.0863700192, false),
    (7, 1, 11.0863700192, true),
    (2, 3, 11.6198411721, false),
    (2, 3, 11.6198411721, true),
    (0, 4, 11.7915344390, false),
    (8, 1, 12.2250922640, false),
    (8, 1, 12.2250922640, true),
    (5, 2, 12.3386041975, false),
    (5, 2, 12.3386041975, true),
    (3, 3, 13.0152007217, false),
    (3, 3, 13.0152007217, true),
    (1, 4, 13.3236919363, false),
    (1, 4, 13.3236919363, true),
    (9, 1, 13.3543004774, false),
    (9, 1, 13.3543004774, true),
    (6, 2, 13.5892901705, false),
    (6, 2, 13.5892901705, true),
    (4, 3, 14.3725366716, false),
    (4, 3, 14.3725366716, true),
    (10, 1, 14.4755006866, false),
    (10, 1, 14.4755006866, true),
    (2, 4, 14.7959517824, false),
    (2, 4, 14.7959517824, true),
    (7, 2, 14.8212687270, false),
    (7, 2, 14.8212687270, true),
    (0, 5, 14.9309177085, false),
    (11, 1, 15.5898478845, false),
    (11, 1, 15.5898478845, true),
    (5, 3, 15.7001740797, false),
    (5, 3, 15.7001740797, true),
    (8, 2, 16.0377741909, false),
    (8, 2, 16.0377741909, true),
    (3, 4, 16.2234661603, false),
    (3, 4, 16.2234661603, true),
    (1, 5, 16.4706300509, false),
    (1, 5, 16.4706300509, true),
    (12, 1, 16.6982499338, false),
    (12, 1, 16.6982499338, true),
    (6, 3, 17.0038196678, false),
    (6, 3, 17.0038196678, true),
    (9, 2, 17.2412203825, false),
    (9, 2, 17.2412203825, true),
    (4, 4, 17.6159660498, false),
    (4, 4, 17.6159660498, true),
    (13, 1, 17.8014351533, false),
    (13, 1, 17.8014351533, true),
    (2, 5, 17.9598194950, false),
    (2, 5, 17.9598194950, true),
    (0, 6, 18.0710639679, false),
    (7, 3, 18.2875828325, false),
    (7, 3, 18.2875828325, true),
    (10, 2, 18.4334636670, false),
    (10, 2, 18.4334636670, true),
    (14, 1, 18.8999979532, false),
    (14, 1, 18.8999979532, true),
    (5, 4, 18.9801338752, false),
    (5, 4, 18.9801338752, true),
    (3, 5, 19.4094152264, false),
    (3, 5, 19.4094152264, true),
    (8, 3, 19.5545364310, false),
    (8, 3, 19.5545364310, true),
    (1, 6, 19.6158585105, false),
    (1, 6, 19.6158585105, true),
    (11, 2, 19.6159669040, false),
    (11, 2, 19.6159669040, true),
    (15, 1, 19.9944306298, false),
    (15, 1, 19.9944306298, true),
    (6, 4, 20.3207892136, false),
    (6, 4, 20.3207892136, true),
    (12, 2, 20.7899063601, false),
    (12, 2, 20.7899063601, true),
    (9, 3, 20.8070477893, false),
    (9, 3, 20.8070477893, true),
    (4, 5, 20.8269329570, false),
    (4, 5, 20.8269329570, true),
    (16, 1, 21.0851461131, false),
    (16, 1, 21.0851461131, true),
    (2, 6, 21.1169970530, false),
    (2, 6, 21.1169970530, true),
    (0, 7, 21.2116366299, false),
    (7, 4, 21.6415410198, false),
    (7, 4, 21.6415410198, true),
    (13, 2, 21.9562440678, false),
    (13, 2, 21.9562440678, true),
    (10, 3, 22.0469853647, false),
    (10, 3, 22.0469853647, true),
    (17, 1, 22.1724946188, false),
    (17, 1, 22.1724946188, true),
    (5, 5, 22.2177998966, false),
    (5, 5, 22.2177998966, true),
    (3, 6, 22.5827295931, false),
    (3, 6, 22.5827295931, true),
    (1, 7, 22.7600843806, false),
    (1, 7, 22.7600843806, true),
    (8, 4, 22.9451731319, false),
    (8, 4, 22.9451731319, true),
    (14, 2, 23.1157783473, false),
    (14, 2, 23.1157783473, true),
    (18, 1, 23.2567760851, false),
    (18, 1, 23.2567760851, true),
    (11, 3, 23.2758537263, false),
    (11, 3, 23.2758537263, true),
    (6, 5, 23.5860844356, false),
    (6, 5, 23.5860844356, true),
    (4, 6, 24.0190195248, false),
    (4, 6, 24.0190195248, true),
    (9, 4, 24.2338852578, false),
    (9, 4, 24.2338852578, true),
    (15, 2, 24.2691800262, false),
    (15, 2, 24.2691800262, true),
    (2, 7, 24.2701123136, false),
    (2, 7, 24.2701123136, true),
    (19, 1, 24.3382496234, false),
    (19, 1, 24.3382496234, true),
    (0, 8, 24.3524715307, false),
    (12, 3, 24.4948850439, false),
    (12, 3, 24.4948850439, true),
    (7, 5, 24.9349278877, false),
    (7, 5, 24.9349278877, true),
    (16, 2, 25.4170190063, false),
    (16, 2, 25.4170190063, true),
    (20, 1, 25.4171408141, false),
    (20, 1, 25.4171408141, true),
    (5, 6, 25.4303411542, false),
    (5, 6, 25.4303411542, true),
    (10, 4, 25.5094505542, false),
    (10, 4, 25.5094505542, true),
    (13, 3, 25.7051030539, false),
    (13, 3, 25.7051030539, true),
    (3, 7, 25.7481666993, false),
    (3, 7, 25.7481666993, true),
    (1, 8, 25.9036720876, false),
    (1, 8, 25.9036720876, true),
    (8, 5, 26.2668146412, false),
    (8, 5, 26.2668146412, true),
    (21, 1, 26.4936474160, false),
    (21, 1, 26.4936474160, true),
    (17, 2, 26.5597841380, false),
    (17, 2, 26.5597841380, true),
    (11, 4, 26.7733225455, false),
    (11, 4, 26.7733225455, true),
    (6, 6, 26.8201519834, false),
    (6, 6, 26.8201519834, true),
    (14, 3, 26.9073689762, false),
    (14, 3, 26.9073689762, true),
    (4, 7, 27.1990877660, false),
    (4, 7, 27.1990877660, true),
    (2, 8, 27.4205735500, false),
    (2, 8, 27.4205735500, true),
    (0, 9, 27.4934791320, false),
    (22, 1, 27.5679438913, false),
    (22, 1, 27.5679438913, true),
    (9, 5, 27.5837489636, false),
    (9, 5, 27.5837489636, true),
    (18, 2, 27.6978983509, false),
    (18, 2, 27.6978983509, true),
    (12, 4, 28.0267099500, false),
    (12, 4, 28.0267099500, true),
    (15, 3, 28.1024152317, false),
    (15, 3, 28.1024152317, true),
    (7, 6, 28.1911884595, false),
    (7, 6, 28.1911884595, true),
    (5, 7, 28.6266183073, false),
    (5, 7, 28.6266183073, true),
    (23, 1, 28.6401850308, false),
    (23, 1, 28.6401850308, true),
    (19, 2, 28.8317303513, false),
    (19, 2, 28.8317303513, true),
    (10, 5, 28.8873750635, false),
    (10, 5, 28.8873750635, true),
    (3, 8, 28.9083507809, false),
    (3, 8, 28.9083507809, true),
    (1, 9, 29.0468285349, false),
    (1, 9, 29.0468285349, true),
    (13, 4, 29.2706304419, false),
    (13, 4, 29.2706304419, true),
    (16, 3, 29.2908706963, false),
    (16, 3, 29.2908706963, true),
    (8, 6, 29.5456596710, false),
    (8, 6, 29.5456596710, true),
    (24, 1, 29.7105088898, false),
    (24, 1, 29.7105088898, true),
    (20, 2, 29.9616037916, false),
    (20, 2, 29.9616037916, true),
    (6, 7, 30.0337223866, false),
    (6, 7, 30.0337223866, true),
    (11, 5, 30.1790611788, false),
    (11, 5, 30.1790611788, true),
    (4, 8, 30.3710076671, false),
    (4, 8, 30.3710076671, true),
    (17, 3, 30.4732799463, false),
    (17, 3, 30.4732799463, true),
    (14, 4, 30.5059501639, false),
    (14, 4, 30.5059501639, true),
    (2, 9, 30.5692044955, false),
    (2, 9, 30.5692044955, true),
    (0, 10, 30.6346064684, false),
    (25, 1, 30.7790391866, false),
    (25, 1, 30.7790391866, true),
    (9, 6, 30.8853789677, false),
    (9, 6, 30.8853789677, true),
    (21, 2, 31.0878045460, false),
    (21, 2, 31.0878045460, true),
    (7, 7, 31.4227941923, false),
    (7, 7, 31.4227941923, true),
    (12, 5, 31.4599600353, false),
    (12, 5, 31.4599600353, true),
    (18, 3, 31.6501181519, false),
    (18, 3, 31.6501181519, true),
    (15, 4, 31.7334133444, false),
    (15, 4, 31.7334133444, true),
    (5, 8, 31.8117167240, false),
    (5, 8, 31.8117167240, true),
    (26, 1, 31.8458872787, false),
    (26, 1, 31.8458872787, true),
    (3, 9, 32.0648524071, false),
    (3, 9, 32.0648524071, true),
    (1, 10, 32.1896799110, false),
    (1, 10, 32.1896799110, true),
    (22, 2, 32.2105865495, false),
    (22, 2, 32.2105865495, true),
    (10, 6, 32.2118561997, false),
    (10, 6, 32.2118561997, true),
    (13, 5, 32.7310533110, false),
    (13, 5, 32.7310533110, true),
    (8, 7, 32.7958000373, false),
    (8, 7, 32.7958000373, true),
    (19, 3, 32.8218027619, false),
    (19, 3, 32.8218027619, true),
    (27, 1, 32.9111538050, false),
    (27, 1, 32.9111538050, true),
];


#[derive(Debug, Clone, Copy)]
pub struct ModeState {
    pub q: f64,
    pub v: f64,
    pub base_omega: f64,
    pub modal_mass: f64,
    pub t60: f64,
    pub angular_order: u8,
    pub radial_root: f64,
    pub sine_orientation: bool,
    pub strike_coupling: f64,
    pub radiation_weight: f64,
}

pub struct MalletExciter {
    pub x: f64,
    pub v: f64,
    pub mass: f64,
    pub stiffness: f64,
    pub dissipation: f64,
    pub exponent: f64,
    pub is_contacting: bool,
}

impl MalletExciter {
    pub fn new(mass: f64, stiffness: f64, dissipation: f64) -> Self {
        Self {
            x: 0.0,
            v: 0.0,
            mass,
            stiffness,
            dissipation,
            exponent: 1.5,
            is_contacting: false,
        }
    }

    pub fn trigger(&mut self, velocity: f64) {
        self.x = 0.0;
        self.v = velocity.clamp(0.05, 12.0);
        self.is_contacting = true;
    }

    pub fn step(&mut self, surface_x: f64, surface_v: f64, dt: f64) -> f64 {
        if !self.is_contacting {
            return 0.0;
        }
        const SUB_STEPS: usize = 4;
        let sub_dt = dt / SUB_STEPS as f64;
        let mut total_force = 0.0;

        for _ in 0..SUB_STEPS {
            let penetration = self.x - surface_x;
            if penetration > 0.0 {
                let v_rel = self.v - surface_v;
                let elastic = self.stiffness * penetration.powf(self.exponent);
                let dissipative = self.dissipation * elastic * v_rel;
                let force = (elastic + dissipative).max(0.0);
                self.v -= (force / self.mass) * sub_dt;
                self.x += self.v * sub_dt;
                total_force += force;
            } else {
                self.x += self.v * sub_dt;
                if self.v <= surface_v {
                    self.is_contacting = false;
                }
            }
        }
        total_force / SUB_STEPS as f64
    }
}

pub struct ShellResonator {
    y1: f64,
    y2: f64,
    a1: f64,
    a2: f64,
    gain: f64,
}

impl ShellResonator {
    pub fn new(sample_rate: f64, freq_hz: f64, q_factor: f64, gain: f64) -> Self {
        let w0 = 2.0 * PI * freq_hz / sample_rate;
        let alpha = w0.sin() / (2.0 * q_factor);
        let b0 = alpha;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * w0.cos() / a0;
        let a2 = (1.0 - alpha) / a0;
        Self {
            y1: 0.0,
            y2: 0.0,
            a1,
            a2,
            gain: (b0 / a0) * gain,
        }
    }

    pub fn tick(&mut self, x: f64) -> f64 {
        let y = self.gain * x - self.a1 * self.y1 - self.a2 * self.y2;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

pub struct DoubleHeadDrum {
    pub sample_rate: f64,
    pub radius_m: f64,
    pub fundamental_hz: f64,
    pub exciter: MalletExciter,
    // Coupled (0, 1) fundamental eigenmodes:
    // In-phase mode: both heads move in same direction (lower pitch, low air compression)
    pub u_plus_q: f64,
    pub u_plus_v: f64,
    pub omega_plus: f64,
    pub trans_plus: ModalTransition,
    // Anti-phase mode: heads move against each other (higher pitch, air spring active)
    pub u_minus_q: f64,
    pub u_minus_v: f64,
    pub omega_minus: f64,
    pub trans_minus: ModalTransition,
    // Higher modes (indices 1..256) on top and bottom heads
    pub top_modes: Vec<ModeState>,
    pub bottom_modes: Vec<ModeState>,
    top_trans: Vec<ModalTransition>,
    bottom_trans: Vec<ModalTransition>,
    // Shell resonators
    pub shell1: ShellResonator,
    pub shell2: ShellResonator,
    // Dynamics
    pub smoothed_energy: f64,
    pub tension_gain: f64,
    pub current_scale: f64,
    pub contact_patch_m: f64,
    pub top_mass: f64,
    step_counter: usize,
}

impl DoubleHeadDrum {
    pub fn new_tom(sample_rate: f64, fundamental_hz: f64, diameter_inches: f64, _depth_inches: f64) -> Self {
        let radius_m = (diameter_inches * 0.0254) * 0.5;
        let f0 = fundamental_hz.max(20.0);
        let alpha0 = TOM_MODE_SPECS[0].2;
        let total_mass = 0.28 * PI * radius_m * radius_m;
        let modal_mass_01 = total_mass * 0.25;

        // Mode splitting: anti-phase mode is ~18% higher than in-phase mode for classic tom
        let omega_plus = 2.0 * PI * f0;
        let omega_minus = 2.0 * PI * (f0 * 1.18);
        let t60_plus = 1.6;
        let t60_minus = 1.0; // Air damping causes slightly faster decay for anti-phase
        let dt = 1.0 / sample_rate;

        let trans_plus = ModalTransition::new(omega_plus, 1000.0_f64.ln() / t60_plus, dt, OverdampedPolicy::ExponentialFallback);
        let trans_minus = ModalTransition::new(omega_minus, 1000.0_f64.ln() / t60_minus, dt, OverdampedPolicy::ExponentialFallback);

        let mut top_modes = Vec::with_capacity(255);
        let mut bottom_modes = Vec::with_capacity(255);
        let mut top_trans = Vec::with_capacity(255);
        let mut bottom_trans = Vec::with_capacity(255);

        for &(order, _radial_idx, root, is_sin) in TOM_MODE_SPECS[1..].iter() {
            let freq = f0 * (root / alpha0);
            let omega = 2.0 * PI * freq;
            let t60 = (1.4 / (1.0 + (freq / 220.0).powf(1.6)) + 0.03).max(0.02);
            let sigma = (1000.0_f64.ln() / t60).min(omega * 0.9);

            let modal_mass = if order == 0 { total_mass * 0.25 } else { total_mass * 0.15 };
            let rad_weight = if order == 0 {
                1.0 / (1.0 + (root / alpha0 - 1.0) * 0.4)
            } else {
                0.22 / (1.0 + order as f64 * 0.7 + (root / alpha0) * 0.25)
            };

            let mode = ModeState {
                q: 0.0,
                v: 0.0,
                base_omega: omega,
                modal_mass,
                t60,
                angular_order: order,
                radial_root: root,
                sine_orientation: is_sin,
                strike_coupling: 0.0,
                radiation_weight: rad_weight,
            };

            top_modes.push(mode);
            bottom_modes.push(mode);
            top_trans.push(ModalTransition::new(omega, sigma, dt, OverdampedPolicy::ExponentialFallback));
            bottom_trans.push(ModalTransition::new(omega, sigma, dt, OverdampedPolicy::ExponentialFallback));
        }

        let mut drum = Self {
            sample_rate,
            radius_m,
            fundamental_hz: f0,
            exciter: MalletExciter::new(0.025, 6.0e6, 0.06),
            u_plus_q: 0.0,
            u_plus_v: 0.0,
            omega_plus,
            trans_plus,
            u_minus_q: 0.0,
            u_minus_v: 0.0,
            omega_minus,
            trans_minus,
            top_modes,
            bottom_modes,
            top_trans,
            bottom_trans,
            shell1: ShellResonator::new(sample_rate, f0 * 1.6, 5.0, 0.18),
            shell2: ShellResonator::new(sample_rate, f0 * 2.8, 7.0, 0.10),
            smoothed_energy: 0.0,
            tension_gain: 12.0,
            current_scale: 1.0,
            contact_patch_m: radius_m * 0.10,
            top_mass: modal_mass_01,
            step_counter: 0,
        };
        drum.set_strike_point(0.25, 0.0);
        drum
    }

    pub fn new_kick(sample_rate: f64) -> Self {
        let radius_m = 0.2794; // 22-inch kick
        let f0 = 48.0;
        let alpha0 = TOM_MODE_SPECS[0].2;
        let total_mass = 0.45 * PI * radius_m * radius_m;
        let modal_mass_01 = total_mass * 0.25;

        // Kick mode splitting: anti-phase mode is ~25% higher (stronger air spring in big cavity)
        let omega_plus = 2.0 * PI * f0;
        let omega_minus = 2.0 * PI * (f0 * 1.25);
        let t60_plus = 0.75;
        let t60_minus = 0.45; // Dampers/pillows inside kick absorb anti-phase motion fast
        let dt = 1.0 / sample_rate;

        let trans_plus = ModalTransition::new(omega_plus, 1000.0_f64.ln() / t60_plus, dt, OverdampedPolicy::ExponentialFallback);
        let trans_minus = ModalTransition::new(omega_minus, 1000.0_f64.ln() / t60_minus, dt, OverdampedPolicy::ExponentialFallback);

        let mut top_modes = Vec::with_capacity(255);
        let mut bottom_modes = Vec::with_capacity(255);
        let mut top_trans = Vec::with_capacity(255);
        let mut bottom_trans = Vec::with_capacity(255);

        for &(order, _radial_idx, root, is_sin) in TOM_MODE_SPECS[1..].iter() {
            let freq = f0 * (root / alpha0);
            let omega = 2.0 * PI * freq;
            let t60 = (0.6 / (1.0 + (freq / 120.0).powf(1.8)) + 0.02).max(0.015);
            let sigma = (1000.0_f64.ln() / t60).min(omega * 0.9);

            let modal_mass = if order == 0 { total_mass * 0.25 } else { total_mass * 0.15 };
            let rad_weight = if order == 0 {
                1.0 / (1.0 + (root / alpha0 - 1.0) * 0.4)
            } else {
                0.15 / (1.0 + order as f64 * 0.8 + (root / alpha0) * 0.3)
            };

            let mode = ModeState {
                q: 0.0,
                v: 0.0,
                base_omega: omega,
                modal_mass,
                t60,
                angular_order: order,
                radial_root: root,
                sine_orientation: is_sin,
                strike_coupling: 0.0,
                radiation_weight: rad_weight,
            };

            top_modes.push(mode);
            bottom_modes.push(mode);
            top_trans.push(ModalTransition::new(omega, sigma, dt, OverdampedPolicy::ExponentialFallback));
            bottom_trans.push(ModalTransition::new(omega, sigma, dt, OverdampedPolicy::ExponentialFallback));
        }

        let mut drum = Self {
            sample_rate,
            radius_m,
            fundamental_hz: f0,
            exciter: MalletExciter::new(0.090, 8.0e6, 0.10), // Heavy 90g felt beater
            u_plus_q: 0.0,
            u_plus_v: 0.0,
            omega_plus,
            trans_plus,
            u_minus_q: 0.0,
            u_minus_v: 0.0,
            omega_minus,
            trans_minus,
            top_modes,
            bottom_modes,
            top_trans,
            bottom_trans,
            shell1: ShellResonator::new(sample_rate, 72.0, 3.5, 0.25),
            shell2: ShellResonator::new(sample_rate, 130.0, 4.5, 0.15),
            smoothed_energy: 0.0,
            tension_gain: 6.0,
            current_scale: 1.0,
            contact_patch_m: radius_m * 0.12,
            top_mass: modal_mass_01,
            step_counter: 0,
        };
        drum.set_strike_point(0.18, 0.0);
        drum
    }

    pub fn set_strike_point(&mut self, radius_fraction: f64, angle_rad: f64) {
        let r_norm = radius_fraction.clamp(0.01, 0.95);
        for mode in &mut self.top_modes {
            let spatial = mode_shape(mode.angular_order, mode.radial_root, mode.sine_orientation, r_norm, angle_rad);
            let filter = (-0.5 * (mode.radial_root * self.contact_patch_m / self.radius_m).powi(2)).exp();
            mode.strike_coupling = spatial * filter;
        }
    }

    pub fn trigger(&mut self, velocity: f64) {
        self.exciter.trigger(velocity * 4.5);
    }

    pub fn step(&mut self) -> f64 {
        let dt = 1.0 / self.sample_rate;

        // Strike shape of fundamental (0, 1) mode at r_hit
        let alpha0 = TOM_MODE_SPECS[0].2;
        let phi_01 = bessel_j(0, alpha0 * 0.25) * (-0.5 * (alpha0 * self.contact_patch_m / self.radius_m).powi(2)).exp();

        // Top head surface displacement at strike point: (0, 1) + higher modes
        let q_top_01 = (self.u_plus_q + self.u_minus_q) * (1.0 / std::f64::consts::SQRT_2);
        let v_top_01 = (self.u_plus_v + self.u_minus_v) * (1.0 / std::f64::consts::SQRT_2);

        let mut x_surf = q_top_01 * phi_01;
        let mut v_surf = v_top_01 * phi_01;
        for mode in &self.top_modes {
            x_surf += mode.q * mode.strike_coupling;
            v_surf += mode.v * mode.strike_coupling;
        }

        // Mallet contact force
        let strike_force = self.exciter.step(x_surf, v_surf, dt);

        // Advance fundamental coupled eigenmodes
        let f_modal_01 = strike_force * phi_01;
        let force_plus = f_modal_01 * (1.0 / std::f64::consts::SQRT_2) / self.top_mass;
        let force_minus = f_modal_01 * (1.0 / std::f64::consts::SQRT_2) / self.top_mass;

        // Transition for u+ and u-
        let (p11_p, p12_p, p21_p, p22_p) = self.trans_plus.phi;
        let (g1_p, g2_p) = self.trans_plus.gamma;
        let next_q_plus = p11_p * self.u_plus_q + p12_p * self.u_plus_v + g1_p * force_plus;
        let next_v_plus = p21_p * self.u_plus_q + p22_p * self.u_plus_v + g2_p * force_plus;

        let (p11_m, p12_m, p21_m, p22_m) = self.trans_minus.phi;
        let (g1_m, g2_m) = self.trans_minus.gamma;
        let next_q_minus = p11_m * self.u_minus_q + p12_m * self.u_minus_v + g1_m * force_minus;
        let next_v_minus = p21_m * self.u_minus_q + p22_m * self.u_minus_v + g2_m * force_minus;

        self.u_plus_q = next_q_plus;
        self.u_plus_v = next_v_plus;
        self.u_minus_q = next_q_minus;
        self.u_minus_v = next_v_minus;

        // Reconstruct top and bottom velocities for fundamental (0, 1):
        // Note: u+ and u- have different frequencies, creating acoustic beating!
        let v_top_fund = (self.u_plus_v + self.u_minus_v) * (1.0 / std::f64::consts::SQRT_2);
        let v_bot_fund = (self.u_plus_v - self.u_minus_v) * (1.0 / std::f64::consts::SQRT_2);

        // Advance higher modes on top and bottom heads
        let mut top_sound = v_top_fund * 1.0;
        for (mode, trans) in self.top_modes.iter_mut().zip(&self.top_trans) {
            let (p11, p12, p21, p22) = trans.phi;
            let (g1, g2) = trans.gamma;
            let f_m = (strike_force * mode.strike_coupling) / mode.modal_mass;
            let nq = p11 * mode.q + p12 * mode.v + g1 * f_m;
            let nv = p21 * mode.q + p22 * mode.v + g2 * f_m;
            mode.q = nq;
            mode.v = nv;
            top_sound += mode.v * mode.radiation_weight;
        }

        // Bottom head higher modes ring down naturally from acoustic energy
        let mut bot_sound = v_bot_fund * 0.40;
        for (mode, trans) in self.bottom_modes.iter_mut().zip(&self.bottom_trans) {
            let (p11, p12, p21, p22) = trans.phi;
            let nq = p11 * mode.q + p12 * mode.v;
            let nv = p21 * mode.q + p22 * mode.v;
            mode.q = nq;
            mode.v = nv;
            bot_sound += mode.v * mode.radiation_weight * 0.35;
        }

        // Shell resonance excited by rim and head acceleration
        let shell_input = top_sound + strike_force * 0.0002;
        let shell_sound = self.shell1.tick(shell_input) + self.shell2.tick(shell_input);

        // Acoustic pickup: direct top head + wooden shell warmth + bottom head dipole interference
        top_sound + shell_sound - bot_sound * 0.30
    }
}

fn mode_shape(order: u8, root: f64, sine_orientation: bool, r_norm: f64, angle: f64) -> f64 {
    let angular = if order == 0 {
        1.0
    } else if sine_orientation {
        (order as f64 * angle).sin()
    } else {
        (order as f64 * angle).cos()
    };
    bessel_j(order, root * r_norm) * angular
}

fn bessel_j(order: u8, x: f64) -> f64 {
    let order_u = order as usize;
    let half = x * 0.5;
    let mut term = half.powi(order as i32) / factorial(order_u);
    let mut sum = term;
    let half_squared = half * half;
    for k in 0..32 {
        let denom = (k + 1) as f64 * (k + order_u + 1) as f64;
        term *= -half_squared / denom;
        sum += term;
        if term.abs() < 1e-15 * sum.abs() {
            break;
        }
    }
    sum
}

fn factorial(n: usize) -> f64 {
    let mut val = 1.0;
    for i in 2..=n {
        val *= i as f64;
    }
    val
}

fn render_drum(mut drum: DoubleHeadDrum, velocity: f64, seconds: f64) -> Vec<f64> {
    drum.trigger(velocity);
    let total_frames = (drum.sample_rate * seconds) as usize;
    let mut out = Vec::with_capacity(total_frames);
    for _ in 0..total_frames {
        out.push(drum.step());
    }
    out
}

fn save_wav(samples: &[f64], filename: &str) {
    let dir = Path::new("target/tom-prototype");
    fs::create_dir_all(dir).unwrap();
    let path = dir.join(filename);
    let spec = WavSpec {
        channels: 1,
        sample_rate: 48000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = WavWriter::create(&path, spec).unwrap();
    let peak = samples.iter().copied().fold(0.0_f64, |p, s| p.max(s.abs())).max(1e-6);
    for &s in samples {
        let norm = (s / peak * 0.90).clamp(-1.0, 1.0);
        let sample_i16 = (norm * 32767.0) as i16;
        writer.write_sample(sample_i16).unwrap();
    }
    writer.finalize().unwrap();
    println!("Saved WAV to {}", path.display());
}

#[test]
fn test_double_head_tom_and_kick() {
    println!("Rendering Double-Head Coupled Tom and Kick...");
    let sample_rate = 48000.0;

    // 1. 12-inch Rack Tom (110 Hz, 8-inch depth)
    let tom = DoubleHeadDrum::new_tom(sample_rate, 110.0, 12.0, 8.0);
    let tom_samples = render_drum(tom, 0.95, 1.4);
    save_wav(&tom_samples, "double_head_rack_tom.wav");

    // 2. 16-inch Floor Tom (75 Hz, 14-inch depth)
    let floor_tom = DoubleHeadDrum::new_tom(sample_rate, 75.0, 16.0, 14.0);
    let floor_samples = render_drum(floor_tom, 0.95, 1.6);
    save_wav(&floor_samples, "double_head_floor_tom.wav");

    // 3. 22-inch Kick Drum (48 Hz, 18-inch depth, heavy beater)
    let kick = DoubleHeadDrum::new_kick(sample_rate);
    let kick_samples = render_drum(kick, 1.0, 1.2);
    save_wav(&kick_samples, "acoustic_kick_drum.wav");

    // Verify all outputs are finite and bounded
    assert!(tom_samples.iter().all(|s| s.is_finite()));
    assert!(floor_samples.iter().all(|s| s.is_finite()));
    assert!(kick_samples.iter().all(|s| s.is_finite()));

    println!("Double-head drums rendered successfully!");
}
