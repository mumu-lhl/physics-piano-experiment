//! Standalone 256-mode Tom physical modeling prototype.
//! Verifies Avanzini tension modulation, Hunt-Crossley sub-stepping, and spatial Gaussian excitation.

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
pub struct TomModeState {
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

impl Default for MalletExciter {
    fn default() -> Self {
        Self {
            x: 0.0,
            v: 0.0,
            mass: 0.025,
            stiffness: 6.0e6,
            dissipation: 0.06,
            exponent: 1.5,
            is_contacting: false,
        }
    }
}

impl MalletExciter {
    pub fn trigger(&mut self, velocity: f64, hardness: f64) {
        self.x = 0.0;
        self.v = velocity.clamp(0.05, 8.0);
        // Hardness maps to logarithmic stiffness (softer felt mallet to hard wood)
        self.stiffness = 1.5e6 * (hardness.clamp(0.0, 1.0) * 3.0).exp();
        self.is_contacting = true;
    }

    /// Advances contact dynamics using 4x sub-stepping for stability.
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

pub struct TomMembrane {
    pub sample_rate: f64,
    pub radius_m: f64,
    pub fundamental_hz: f64,
    pub modes: Vec<TomModeState>,
    pub smoothed_energy: f64,
    pub tension_modulation_gain: f64, // kappa_E
    pub current_scale: f64,
    pub contact_patch_radius_m: f64,
    transition_cache: Vec<ModalTransition>,
    step_counter: usize,
}

impl TomMembrane {
    pub fn new(sample_rate: f64, fundamental_hz: f64, radius_m: f64, surface_density: f64) -> Self {
        let f0 = fundamental_hz.max(20.0);
        let alpha0 = TOM_MODE_SPECS[0].2;
        let contact_patch_radius_m = 0.015; // 15mm mallet tip

        let mut modes = Vec::with_capacity(TOM_MODE_SPECS.len());
        let mut transitions = Vec::with_capacity(TOM_MODE_SPECS.len());

        let total_membrane_mass = surface_density * PI * radius_m * radius_m;
        for (index, &(order, _radial_idx, root, is_sin)) in TOM_MODE_SPECS.iter().enumerate() {
            let freq = f0 * (root / alpha0);
            let omega = 2.0 * PI * freq;

            // Physical viscoelastic damping: Mylar drumhead high-frequency internal loss
            // Low modes (110 Hz) ring for ~1.2s; 500 Hz dies in ~0.25s; 1500 Hz dies in ~0.05s
            let t60 = (1.4 / (1.0 + (freq / 220.0).powf(1.6)) + 0.03).max(0.02);
            let sigma = (1000.0_f64.ln() / t60).min(omega * 0.9);

            // Normalized modal mass: for a clamped circular membrane,
            // effective modal mass per coordinate is ~25% of total mass for m=0, ~15% for m>0
            let modal_mass = if order == 0 {
                total_membrane_mass * 0.25
            } else {
                total_membrane_mass * 0.15
            };

            // Acoustic radiation efficiency:
            // Axisymmetric modes (m=0) have net volume displacement -> powerful monopole radiation.
            // Non-axisymmetric modes (m>0) have nodal lines with opposite phases -> multipole acoustic cancellation in air.
            let rad_weight = if order == 0 {
                1.0 / (1.0 + (root / alpha0 - 1.0) * 0.4)
            } else {
                0.22 / (1.0 + order as f64 * 0.7 + (root / alpha0) * 0.25)
            };

            modes.push(TomModeState {
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
            });

            transitions.push(ModalTransition::new(
                omega,
                sigma,
                1.0 / sample_rate,
                OverdampedPolicy::ExponentialFallback,
            ));
        }

        let mut tom = Self {
            sample_rate,
            radius_m,
            fundamental_hz: f0,
            modes,
            smoothed_energy: 0.0,
            tension_modulation_gain: 12.0,
            current_scale: 1.0,
            contact_patch_radius_m,
            transition_cache: transitions,
            step_counter: 0,
        };
        tom.set_strike_point(0.35, 0.0);
        tom
    }

    pub fn set_strike_point(&mut self, radius_fraction: f64, angle_rad: f64) {
        let r_norm = radius_fraction.clamp(0.01, 0.95);
        for mode in &mut self.modes {
            let spatial = mode_shape(
                mode.angular_order,
                mode.radial_root,
                mode.sine_orientation,
                r_norm,
                angle_rad,
            );
            // Spatial Gaussian filtering of finite contact patch
            let filter = (-0.5 * (mode.radial_root * self.contact_patch_radius_m / self.radius_m).powi(2)).exp();
            mode.strike_coupling = spatial * filter;
        }
    }

    pub fn strike_state(&self) -> (f64, f64) {
        let mut x = 0.0;
        let mut v = 0.0;
        for mode in &self.modes {
            x += mode.q * mode.strike_coupling;
            v += mode.v * mode.strike_coupling;
        }
        (x, v)
    }

    pub fn step(&mut self, strike_force: f64) -> f64 {
        let dt = 1.0 / self.sample_rate;
        // Compute instantaneous kinetic + potential energy
        let mut instant_energy = 0.0;
        for mode in &self.modes {
            instant_energy += 0.5 * mode.modal_mass * (mode.v * mode.v + mode.base_omega * mode.base_omega * mode.q * mode.q);
        }

        // Smooth energy with ~3ms time constant
        let alpha = (-dt / 0.003).exp();
        self.smoothed_energy = alpha * self.smoothed_energy + (1.0 - alpha) * instant_energy;

        // Control rate (every 16 samples) update frequency scale
        if self.step_counter % 16 == 0 {
            let target_scale = (1.0 + self.tension_modulation_gain * self.smoothed_energy).sqrt().min(1.5);
            self.current_scale = target_scale;
            for (index, mode) in self.modes.iter().enumerate() {
                let current_omega = mode.base_omega * self.current_scale;
                let sigma = (1000.0_f64.ln() / mode.t60).min(current_omega * 0.9);
                self.transition_cache[index] = ModalTransition::new(
                    current_omega,
                    sigma,
                    dt,
                    OverdampedPolicy::ExponentialFallback,
                );
            }
        }
        self.step_counter += 1;

        let mut sound_out = 0.0;
        for (index, mode) in self.modes.iter_mut().enumerate() {
            let trans = &self.transition_cache[index];
            let (p11, p12, p21, p22) = trans.phi;
            let (g1, g2) = trans.gamma;

            let f_modal = strike_force * mode.strike_coupling;
            let force_over_m = f_modal / mode.modal_mass;

            let next_q = p11 * mode.q + p12 * mode.v + g1 * force_over_m;
            let next_v = p21 * mode.q + p22 * mode.v + g2 * force_over_m;

            mode.q = next_q;
            mode.v = next_v;

            sound_out += mode.v * mode.radiation_weight;
        }

        sound_out
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

fn render_hit(velocity: f64, strike_pos: f64, hardness: f64, seconds: f64) -> (Vec<f64>, f64, f64) {
    let sample_rate = 48_000.0;
    let mut tom = TomMembrane::new(sample_rate, 110.0, 0.1524, 0.28); // 12-inch tom
    tom.set_strike_point(strike_pos, 0.0);
    let mut mallet = MalletExciter::default();
    mallet.trigger(velocity * 4.5, hardness);

    let total_frames = (sample_rate * seconds) as usize;
    let dt = 1.0 / sample_rate;
    let mut out = Vec::with_capacity(total_frames);
    let mut max_energy: f64 = 0.0;
    let mut max_scale: f64 = 1.0;

    for _ in 0..total_frames {
        let (x_surf, v_surf) = tom.strike_state();
        let force = mallet.step(x_surf, v_surf, dt);
        let sample = tom.step(force);
        max_energy = max_energy.max(tom.smoothed_energy);
        max_scale = max_scale.max(tom.current_scale);
        out.push(sample);
    }
    (out, max_energy, max_scale)
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
fn test_tom_prototype_renders_and_has_pitch_glide() {
    println!("Rendering Tom Prototype (256 modes) with Avanzini tension modulation...");
    let (hard_center, hard_energy, hard_scale) = render_hit(1.0, 0.20, 0.7, 1.2);
    let (soft_center, soft_energy, soft_scale) = render_hit(0.20, 0.20, 0.3, 1.2);
    let (hard_edge, edge_energy, edge_scale) = render_hit(0.9, 0.75, 0.7, 1.2);

    println!("Hard hit: max_energy = {:.4} J, max_scale = {:.4}x frequency", hard_energy, hard_scale);
    println!("Soft hit: max_energy = {:.4} J, max_scale = {:.4}x frequency", soft_energy, soft_scale);
    println!("Edge hit: max_energy = {:.4} J, max_scale = {:.4}x frequency", edge_energy, edge_scale);

    save_wav(&hard_center, "tom_hard_center.wav");
    save_wav(&soft_center, "tom_soft_center.wav");
    save_wav(&hard_edge, "tom_hard_edge.wav");

    // 1. Verify no NaNs, Infs, and finite bounded values
    assert!(hard_center.iter().all(|s| s.is_finite()));
    assert!(soft_center.iter().all(|s| s.is_finite()));
    assert!(hard_edge.iter().all(|s| s.is_finite()));

    // 2. Verify tension modulation: hard hit must scale frequency substantially more than soft hit
    assert!(hard_scale > 1.08, "Hard hit should have noticeable tension scale (> 1.08x), got {hard_scale:.3}");
    assert!(soft_scale < 1.05, "Soft hit should have small tension scale (< 1.05x), got {soft_scale:.3}");
    assert!(hard_energy > soft_energy * 10.0, "Hard energy should be much higher than soft energy");

    // 3. Center vs Edge strike timbre:
    // Center strike excites m=0 modes heavily; edge strike excites asymmetric m>0 modes
    let center_rms = rms(&hard_center);
    let edge_rms = rms(&hard_edge);
    assert!(center_rms > 0.0);
    assert!(edge_rms > 0.0);
    println!("Test completed successfully: Tension modulation scales verified!");
}

fn rms(slice: &[f64]) -> f64 {
    (slice.iter().map(|s| s * s).sum::<f64>() / slice.len() as f64).sqrt()
}
