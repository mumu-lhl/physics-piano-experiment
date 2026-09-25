//! Euler-Bernoulli Damped Stiff String Engine with Dual Polarization in Rust.

use std::f64::consts::PI;
use crate::params::StringPhysicalParams;

#[repr(C, align(64))]
#[derive(Debug, Clone)]
pub struct ModalState {
    pub q: f64,
    pub v: f64,
}

#[repr(C, align(64))]
#[derive(Debug, Clone)]
pub struct StiffStringModal {
    pub params: StringPhysicalParams,
    pub sample_rate: f64,
    pub dt: f64,

    pub length: f64,
    pub radius: f64,
    pub density: f64,
    pub youngs_modulus: f64,
    pub tension: f64,
    pub sigma0: f64,
    pub sigma1: f64,
    pub strike_x: f64,

    pub mu: f64,
    pub omega_0: f64,
    pub b_factor: f64,
    pub num_modes: usize,

    // Precomputed coefficients
    pub n_modes: Vec<f64>,
    pub n_modes_sq: Vec<f64>,
    pub phi_h: Vec<f64>,
    pub bridge_coeff: Vec<f64>,
    pub force_scale: f64,
    pub geom_tension_coeff: f64,

    // Transition matrices for Vertical (T) and Horizontal (P)
    // Phi: [2, 2] per mode flattened as (phi11, phi12, phi21, phi22)
    pub phi_t: Vec<(f64, f64, f64, f64)>,
    pub gamma_t: Vec<(f64, f64)>,
    pub omega_t: Vec<f64>,

    pub phi_p: Vec<(f64, f64, f64, f64)>,
    pub gamma_p: Vec<(f64, f64)>,
    pub omega_p: Vec<f64>,

    // Dynamic states
    pub state_t: Vec<ModalState>,
    pub state_p: Vec<ModalState>,
    pub current_delta_t: f64,

    // Damper
    pub damper_active: bool,
    pub damper_decay_mult: f64,
    pub damper_depth: f64,
    pub has_damper: bool,
    pub target_damper_depth: f64,
    pub current_damper_depth: f64,
    pub damper_drop_rate: f64,
    pub damper_lift_rate: f64,
    pub damper_modal_rates: Vec<f64>,

    // Microtonal expression tuning (cents)
    pub tuning_offset_cents: f64,

    // Inharmonicity stiffness scale [0.1 ~ 5.0]
    pub inharmonicity_scale: f64,
}

impl StiffStringModal {
    pub fn new(params: StringPhysicalParams, sample_rate: f64) -> Self {
        let dt = 1.0 / sample_rate;
        let length = params.length;
        let radius = params.radius;
        let density = params.density;
        let youngs_modulus = params.youngs_modulus;
        let tension = params.tension;
        let sigma0 = params.sigma0;
        let sigma1 = params.sigma1;
        let strike_x = params.strike_ratio * length;

        let area = PI * radius.powi(2);
        let mu = density * area;
        let omega_0 = (PI / length) * (tension / mu).sqrt();
        let b_factor = (PI.powi(3) * youngs_modulus * radius.powi(4)) / (4.0 * tension * length.powi(2));

        // Truncate modes at 0.95 * Nyquist to prevent ultrasonic aliasing
        let max_omega = 0.95 * PI * sample_rate;
        let mut n_modes = Vec::new();
        for n in 1..=params.num_modes {
            let n_f = n as f64;
            let omega = n_f * omega_0 * (1.0 + b_factor * n_f.powi(2)).sqrt();
            if omega <= max_omega {
                n_modes.push(n_f);
            }
        }
        if n_modes.is_empty() {
            n_modes.push(1.0);
        }
        let m = n_modes.len();

        // Finite felt width spatial sinc windowing
        let f0 = (1.0 / (2.0 * length)) * (tension / mu).sqrt();
        let norm_k = ((f0 - 27.5) / (4186.0 - 27.5)).clamp(0.0, 1.0);
        let w_h = 0.020 - (0.010 * norm_k);

        let mut phi_h = Vec::with_capacity(m);
        let mut bridge_coeff = Vec::with_capacity(m);

        for &n in &n_modes {
            let sinc_val = {
                let x = PI * (n * w_h) / (2.0 * length);
                if x.abs() < 1e-9 { 1.0 } else { x.sin() / x }
            };
            let phi = (n * PI * strike_x / length).sin() * sinc_val;
            phi_h.push(phi);

            let sign = if (n as i32 - 1) % 2 == 0 { 1.0 } else { -1.0 };
            let b_c = tension * sign * (n * PI / length);
            bridge_coeff.push(b_c);
        }

        let force_scale = 2.0 / (mu * length);
        let geom_tension_coeff = (youngs_modulus * area * PI.powi(2)) / (4.0 * length.powi(2));

        // Viscoelastic felt damping rates (DOCX §68-71)
        // Bass strings have higher momentum/mass and decay slightly slower (~350ms T60)
        // Tenor strings decay in ~220ms T60, while higher modes decay faster under wool felt.
        let reg_factor = 0.55 + 0.45 * (length / 0.62).min(2.5);
        let base_rate = 36.0 / reg_factor;
        let mut damper_modal_rates = Vec::with_capacity(m);
        for &n in &n_modes {
            // Damper head rests at ~13% of string length with finite felt width
            let spatial = 0.70 + 0.30 * (n * PI * 0.13).sin().powi(2);
            let freq_factor = 1.0 + 0.22 * (n - 1.0).min(10.0);
            damper_modal_rates.push(base_rate * spatial * freq_factor);
        }

        // 25ms smoothing time constant for felt drop; 6ms for felt lift
        let damper_drop_rate = 1.0 - (-dt / 0.025).exp();
        let damper_lift_rate = 1.0 - (-dt / 0.006).exp();

        let n_modes_sq = n_modes.iter().map(|&n| n * n).collect();

        let mut string = Self {
            params,
            sample_rate,
            dt,
            length,
            radius,
            density,
            youngs_modulus,
            tension,
            sigma0,
            sigma1,
            strike_x,
            mu,
            omega_0,
            b_factor,
            num_modes: m,
            n_modes,
            n_modes_sq,
            phi_h,
            bridge_coeff,
            force_scale,
            geom_tension_coeff,
            phi_t: Vec::new(),
            gamma_t: Vec::new(),
            omega_t: Vec::new(),
            phi_p: Vec::new(),
            gamma_p: Vec::new(),
            omega_p: Vec::new(),
            state_t: vec![ModalState { q: 0.0, v: 0.0 }; m],
            state_p: vec![ModalState { q: 0.0, v: 0.0 }; m],
            current_delta_t: 0.0,
            damper_active: true,
            damper_decay_mult: 1.0,
            damper_depth: 1.0,
            has_damper: true,
            target_damper_depth: 1.0,
            current_damper_depth: 1.0,
            damper_drop_rate,
            damper_lift_rate,
            damper_modal_rates,
            tuning_offset_cents: 0.0,
            inharmonicity_scale: 1.0,
        };

        string.recompute_transition_matrices();
        string
    }

    pub fn set_tuning_offset(&mut self, cents: f64) {
        if (cents - self.tuning_offset_cents).abs() > 1e-4 {
            self.tuning_offset_cents = cents;
            self.recompute_transition_matrices();
        }
    }

    pub fn set_inharmonicity_scale(&mut self, scale: f64) {
        let s = scale.clamp(0.1, 5.0);
        if (s - self.inharmonicity_scale).abs() > 1e-4 {
            self.inharmonicity_scale = s;
            self.recompute_transition_matrices();
        }
    }

    pub fn recompute_transition_matrices(&mut self) {
        let freq_ratio = 2.0f64.powf(self.tuning_offset_cents / 1200.0);
        let eff_t0 = self.tension * freq_ratio.powi(2);
        let eff_omega_0 = (PI / self.length) * (eff_t0 / self.mu).sqrt();
        let eff_b = ((PI.powi(3) * self.youngs_modulus * self.radius.powi(4)) / (4.0 * eff_t0 * self.length.powi(2))) * self.inharmonicity_scale;

        self.phi_t.clear();
        self.gamma_t.clear();
        self.omega_t.clear();
        self.phi_p.clear();
        self.gamma_p.clear();
        self.omega_p.clear();

        for &n in &self.n_modes {
            let omega_t = n * eff_omega_0 * (1.0 + eff_b * n.powi(2)).sqrt();
            let gamma_t = self.sigma0 + self.sigma1 * (n * PI / self.length).powi(2);

            let omega_p = omega_t * (1.0 + self.params.polarization_mistuning);
            let gamma_p = self.sigma0 * 0.22 + self.sigma1 * 0.30 * (n * PI / self.length).powi(2);

            let (pt, gt) = Self::compute_discrete_transition(omega_t, gamma_t, self.dt);
            let (pp, gp) = Self::compute_discrete_transition(omega_p, gamma_p, self.dt);

            self.omega_t.push(omega_t);
            self.gamma_t.push(gt);
            self.phi_t.push(pt);

            self.omega_p.push(omega_p);
            self.gamma_p.push(gp);
            self.phi_p.push(pp);
        }
    }

    #[inline]
    fn compute_discrete_transition(omega: f64, gamma: f64, dt: f64) -> ((f64, f64, f64, f64), (f64, f64)) {
        let omega_d = (omega.powi(2) - gamma.powi(2)).max(1e-6).sqrt();
        let decay = (-gamma * dt).exp();
        let sin_wd = (omega_d * dt).sin();
        let cos_wd = (omega_d * dt).cos();

        let phi_11 = decay * (cos_wd + (gamma / omega_d) * sin_wd);
        let phi_12 = decay * (sin_wd / omega_d);
        let phi_21 = -decay * ((omega.powi(2) / omega_d) * sin_wd);
        let phi_22 = decay * (cos_wd - (gamma / omega_d) * sin_wd);

        let gamma_1 = (1.0 / omega.powi(2)) * (1.0 - decay * (cos_wd + (gamma / omega_d) * sin_wd));
        let gamma_2 = phi_12;

        ((phi_11, phi_12, phi_21, phi_22), (gamma_1, gamma_2))
    }

    pub fn set_damper(&mut self, active: bool, depth: f64) {
        self.damper_active = active;
        if !self.has_damper {
            self.damper_depth = 0.0;
            self.target_damper_depth = 0.0;
            self.damper_decay_mult = 1.0;
            return;
        }
        self.damper_depth = depth.clamp(0.0, 1.0);
        self.target_damper_depth = if active { self.damper_depth } else { 0.0 };
        self.damper_decay_mult = 1.0 + (if active { 15.0 * self.damper_depth } else { 0.0 });
    }

    #[inline]
    pub fn get_strike_displacement_and_velocity(&self) -> (f64, f64) {
        let mut u_h = 0.0;
        let mut v_h = 0.0;
        for i in 0..self.num_modes {
            u_h += self.state_t[i].q * self.phi_h[i];
            v_h += self.state_t[i].v * self.phi_h[i];
        }
        (u_h, v_h)
    }

    pub fn reset(&mut self) {
        for state in &mut self.state_t {
            state.q = 0.0;
            state.v = 0.0;
        }
        for state in &mut self.state_p {
            state.q = 0.0;
            state.v = 0.0;
        }
        self.current_delta_t = 0.0;
        if self.has_damper {
            self.current_damper_depth = 1.0;
            self.target_damper_depth = 1.0;
            self.damper_active = true;
        } else {
            self.current_damper_depth = 0.0;
            self.target_damper_depth = 0.0;
            self.damper_active = false;
        }
    }

    #[inline]
    pub fn get_energy(&self) -> f64 {
        let mut energy = 0.0;
        let factor = 0.5 * self.mu * self.length;
        for i in 0..self.num_modes {
            energy += factor * (self.state_t[i].v.powi(2) + self.omega_t[i].powi(2) * self.state_t[i].q.powi(2));
            energy += factor * (self.state_p[i].v.powi(2) + self.omega_p[i].powi(2) * self.state_p[i].q.powi(2));
        }
        energy
    }

    #[inline]
    pub fn get_bridge_forces(&self) -> (f64, f64, f64) {
        let mut fb_t = 0.0;
        let mut fb_p = 0.0;
        for i in 0..self.num_modes {
            fb_t += self.state_t[i].q * self.bridge_coeff[i];
            fb_p += self.state_p[i].q * self.bridge_coeff[i];
        }
        (fb_t, fb_p, self.current_delta_t)
    }

    #[inline]
    pub fn step(&mut self, f_hammer: f64, f_coupling_t: f64, f_coupling_p: f64) -> (f64, f64, f64) {
        let f_hammer_scaled = self.force_scale * f_hammer;
        let f_ext_t = self.force_scale * f_coupling_t;
        let f_ext_p = self.force_scale * f_coupling_p;

        let mut modal_strain_sum = 0.0;
        let mut fb_t = 0.0;
        let mut fb_p = 0.0;

        // Smooth continuous damper depth tracking (DOCX §70-71)
        // Felt compresses smoothly onto string over 20-30ms, eliminating hard step transitions
        if (self.target_damper_depth - self.current_damper_depth).abs() > 1e-6 {
            let alpha = if self.target_damper_depth > self.current_damper_depth {
                self.damper_drop_rate
            } else {
                self.damper_lift_rate
            };
            self.current_damper_depth += (self.target_damper_depth - self.current_damper_depth) * alpha;
        } else {
            self.current_damper_depth = self.target_damper_depth;
        }

        let is_damping = self.current_damper_depth > 1e-4;
        let damper_depth_dt = self.current_damper_depth * self.dt;

        let has_hammer = f_hammer_scaled > 0.0;

        if is_damping {
            for i in 0..self.num_modes {
                let f_t = if has_hammer { f_hammer_scaled * self.phi_h[i] + f_ext_t } else { f_ext_t };
                let f_p = f_ext_p;

                let (p11_t, p12_t, p21_t, p22_t) = self.phi_t[i];
                let (g1_t, g2_t) = self.gamma_t[i];

                let q_t = self.state_t[i].q;
                let v_t = self.state_t[i].v;

                let mut new_q_t = p11_t * q_t + p12_t * v_t + g1_t * f_t;
                let mut new_v_t = p21_t * q_t + p22_t * v_t + g2_t * f_t;

                let (p11_p, p12_p, p21_p, p22_p) = self.phi_p[i];
                let (g1_p, g2_p) = self.gamma_p[i];

                let q_p = self.state_p[i].q;
                let v_p = self.state_p[i].v;

                let mut new_q_p = p11_p * q_p + p12_p * v_p + g1_p * f_p;
                let mut new_v_p = p21_p * q_p + p22_p * v_p + g2_p * f_p;

                let rate = self.damper_modal_rates[i];
                let x = rate * damper_depth_dt;
                let mode_damper_factor = (1.0 - x + 0.5 * x * x).max(0.0);
                new_q_t *= mode_damper_factor;
                new_v_t *= mode_damper_factor;
                new_q_p *= mode_damper_factor;
                new_v_p *= mode_damper_factor;

                self.state_t[i].q = new_q_t;
                self.state_t[i].v = new_v_t;
                self.state_p[i].q = new_q_p;
                self.state_p[i].v = new_v_p;

                let bc = self.bridge_coeff[i];
                fb_t += new_q_t * bc;
                fb_p += new_q_p * bc;

                modal_strain_sum += self.n_modes_sq[i] * (new_q_t * new_q_t + new_q_p * new_q_p);
            }
        } else {
            for i in 0..self.num_modes {
                let f_t = if has_hammer { f_hammer_scaled * self.phi_h[i] + f_ext_t } else { f_ext_t };
                let f_p = f_ext_p;

                let (p11_t, p12_t, p21_t, p22_t) = self.phi_t[i];
                let (g1_t, g2_t) = self.gamma_t[i];

                let q_t = self.state_t[i].q;
                let v_t = self.state_t[i].v;

                let new_q_t = p11_t * q_t + p12_t * v_t + g1_t * f_t;
                let new_v_t = p21_t * q_t + p22_t * v_t + g2_t * f_t;

                let (p11_p, p12_p, p21_p, p22_p) = self.phi_p[i];
                let (g1_p, g2_p) = self.gamma_p[i];

                let q_p = self.state_p[i].q;
                let v_p = self.state_p[i].v;

                let new_q_p = p11_p * q_p + p12_p * v_p + g1_p * f_p;
                let new_v_p = p21_p * q_p + p22_p * v_p + g2_p * f_p;

                self.state_t[i].q = new_q_t;
                self.state_t[i].v = new_v_t;
                self.state_p[i].q = new_q_p;
                self.state_p[i].v = new_v_p;

                let bc = self.bridge_coeff[i];
                fb_t += new_q_t * bc;
                fb_p += new_q_p * bc;

                modal_strain_sum += self.n_modes_sq[i] * (new_q_t * new_q_t + new_q_p * new_q_p);
            }
        }

        self.current_delta_t = self.geom_tension_coeff * modal_strain_sum;
        (fb_t, fb_p, self.current_delta_t)
    }
}
