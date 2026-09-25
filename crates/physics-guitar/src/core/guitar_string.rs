//! Dual-polarization stiff string modal engine specialized for 6-string guitar.
//! Supports dynamic fretting, geometric tension modulation, palm muting, and pitch bending.

use std::f64::consts::PI;
use crate::params::GuitarStringParams;
use crate::core::pluck::PluckExciter;

#[repr(C, align(64))]
#[derive(Debug, Clone)]
pub struct ModalState {
    pub q: f64,
    pub v: f64,
}

#[derive(Debug, Clone)]
pub struct GuitarString {
    pub params: GuitarStringParams,
    pub sample_rate: f64,
    pub dt: f64,

    /// Currently active fret (0 = open string, 1..=24)
    pub current_fret: u8,
    /// Effective vibrating length L_eff (meters)
    pub effective_length: f64,
    /// Fundamental frequency at current fret
    pub current_f0: f64,
    /// Number of active modal oscillators
    pub num_modes: usize,

    // Transition matrices for Vertical (T) and Horizontal (P)
    // Phi: flattened (phi11, phi12, phi21, phi22)
    pub phi_t: Vec<(f64, f64, f64, f64)>,
    pub gamma_t: Vec<(f64, f64)>,
    pub omega_t: Vec<f64>,

    pub phi_p: Vec<(f64, f64, f64, f64)>,
    pub gamma_p: Vec<(f64, f64)>,
    pub omega_p: Vec<f64>,

    // Dynamic modal states
    pub state_t: Vec<ModalState>,
    pub state_p: Vec<ModalState>,

    // Spatial mode shapes at bridge x = L
    pub bridge_phi: Vec<f64>,

    // Non-linear tension modulation
    pub current_delta_t: f64,
    pub geom_tension_coeff: f64,

    // Articulations
    /// Palm mute depth [0.0 = open ring, 1.0 = heavy palm mute]
    pub palm_mute_depth: f64,
    /// Pitch bend offset (semitones, e.g. +2.0 for whole-step bend)
    pub pitch_bend_semitones: f64,
    /// Whether natural harmonic node is active (0 = none, 2 = 12th fret octave, 3 = 7th fret, 4 = 5th fret)
    pub harmonic_node: u8,
    /// Fret buzz sensitivity [0.0 = clean/disabled, 1.0 = heavy metallic buzz on hard plucks]
    pub fret_buzz_sensitivity: f64,
    /// Key pressed status (true while note is held, false on release)
    pub is_held: bool,
    /// Active finger release muting (true while finger is damping after key release)
    pub is_releasing: bool,
}

impl GuitarString {
    pub fn new(params: GuitarStringParams, sample_rate: f64) -> Self {
        let dt = 1.0 / sample_rate;
        let mut s = Self {
            effective_length: params.scale_length,
            current_f0: params.open_f0,
            num_modes: params.num_modes,
            params,
            sample_rate,
            dt,
            current_fret: 0,
            phi_t: Vec::new(),
            gamma_t: Vec::new(),
            omega_t: Vec::new(),
            phi_p: Vec::new(),
            gamma_p: Vec::new(),
            omega_p: Vec::new(),
            state_t: Vec::new(),
            state_p: Vec::new(),
            bridge_phi: Vec::new(),
            current_delta_t: 0.0,
            geom_tension_coeff: 0.0,
            palm_mute_depth: 0.0,
            pitch_bend_semitones: 0.0,
            harmonic_node: 0,
            fret_buzz_sensitivity: 0.35,
            is_held: false,
            is_releasing: false,
        };
        s.recalculate_modal_operators();
        s
    }

    /// Sets the active fret position (0 to 24) and pitch bend, re-calculating modal operators.
    pub fn set_fret(&mut self, fret: u8) {
        self.current_fret = fret;
        self.effective_length = self.params.effective_length_at_fret(fret);
        self.current_f0 = self.params.frequency_at_fret(fret);
        self.recalculate_modal_operators();
    }

    /// Sets pitch bend in semitones (e.g. +2.0 for whole step bend).
    pub fn set_pitch_bend(&mut self, semitones: f64) {
        self.pitch_bend_semitones = semitones;
        self.recalculate_modal_operators();
    }

    /// Re-evaluates analytical modal frequencies, inharmonicity, and discrete state transition operators.
    pub fn recalculate_modal_operators(&mut self) {
        let length = self.effective_length;
        // Apply pitch bend to tension: f ~ sqrt(T), so T = T0 * 2^(2 * semitones / 12)
        let bend_factor = 2.0f64.powf(2.0 * self.pitch_bend_semitones / 12.0);
        let tension = self.params.tension * bend_factor;
        let mu = self.params.linear_density;
        let e = self.params.youngs_modulus;
        let i_area = self.params.moment_of_inertia;
        let b = (PI.powi(3) * e * i_area) / (4.0 * tension * length.powi(2));

        let omega_0 = (PI / length) * (tension / mu).sqrt();
        let area = PI * (self.params.diameter / 2.0).powi(2);
        self.geom_tension_coeff = (e * area) / (4.0 * length.powi(2));

        let max_omega = 0.95 * PI * self.sample_rate;
        let mut active_modes = 0;

        self.phi_t.clear();
        self.gamma_t.clear();
        self.omega_t.clear();
        self.phi_p.clear();
        self.gamma_p.clear();
        self.omega_p.clear();
        self.bridge_phi.clear();

        for m in 1..=self.params.num_modes {
            let m_f = m as f64;
            let omega_base = m_f * omega_0 * (1.0 + b * m_f.powi(2)).sqrt();
            if omega_base > max_omega {
                break;
            }
            active_modes += 1;

            // Micro-detuning between Vertical (T) and Horizontal (P) polarizations
            // typical bridge anisotropy introduces ~0.25% frequency split
            let omega_m_t = omega_base;
            let omega_m_p = omega_base * 1.0025;

            // Damping calculation: sigma = sigma0 + sigma1 * (m * pi / L)^2
            // plus palm mute additional damping: Delta C = gamma_palm * (m / N)^1.5
            let mut sigma_m = self.params.sigma0 + self.params.sigma1 * (m_f * PI / length).powi(2);
            if self.palm_mute_depth > 0.0 {
                sigma_m += self.palm_mute_depth * 80.0 * (1.0 + 0.15 * m_f);
            }

            // Natural harmonic selective damping (docx Chapter 3)
            if self.harmonic_node > 0 {
                let k = self.harmonic_node as f64;
                // Delta alpha_m = R / (rho * A * L) * sin^2(m_f * pi / k)
                let suppression = (m_f * PI / k).sin().powi(2);
                sigma_m += 180.0 * suppression;
            }

            // Horizontal (P) in-plane polarization encounters significantly lower soundboard radiation loss
            // (~40% of vertical out-of-plane damping), creating the characteristic acoustic two-stage decay.
            let sigma_m_p = sigma_m * 0.40;

            // Discrete state-space operators via matrix exponential for mode m
            let (phi_t, gamma_t) = Self::compute_discrete_operators(omega_m_t, sigma_m, self.dt);
            let (phi_p, gamma_p) = Self::compute_discrete_operators(omega_m_p, sigma_m_p, self.dt);

            self.phi_t.push(phi_t);
            self.gamma_t.push(gamma_t);
            self.omega_t.push(omega_m_t);

            self.phi_p.push(phi_p);
            self.gamma_p.push(gamma_p);
            self.omega_p.push(omega_m_p);

            // Mode shape spatial gradient at bridge pin x = L: d/dx phi_m(L) = sqrt(2/L) * (m*pi/L) * (-1)^m
            let bridge_grad = (2.0 / length).sqrt() * (m_f * PI / length) * if m % 2 == 0 { 1.0 } else { -1.0 };
            self.bridge_phi.push(bridge_grad);
        }

        self.num_modes = active_modes;
        self.state_t.resize(active_modes, ModalState { q: 0.0, v: 0.0 });
        self.state_p.resize(active_modes, ModalState { q: 0.0, v: 0.0 });
    }

    /// Discrete transition matrix via analytical matrix exponential:
    /// [q(n+1); v(n+1)] = Phi * [q(n); v(n)] + Gamma * F(n)
    fn compute_discrete_operators(omega: f64, sigma: f64, dt: f64) -> ((f64, f64, f64, f64), (f64, f64)) {
        let omega_d_sq = omega.powi(2) - sigma.powi(2);
        let decay = (-sigma * dt).exp();

        if omega_d_sq > 0.0 {
            let omega_d = omega_d_sq.sqrt();
            let cos_d = (omega_d * dt).cos();
            let sin_d = (omega_d * dt).sin();

            let phi11 = decay * (cos_d + (sigma / omega_d) * sin_d);
            let phi12 = decay * (sin_d / omega_d);
            let phi21 = -decay * (omega.powi(2) / omega_d) * sin_d;
            let phi22 = decay * (cos_d - (sigma / omega_d) * sin_d);

            let gamma1 = (1.0 - phi11) / omega.powi(2);
            let gamma2 = -phi21 / omega.powi(2);

            ((phi11, phi12, phi21, phi22), (gamma1, gamma2))
        } else {
            // Overdamped fallback
            let phi11 = decay;
            let phi12 = dt * decay;
            let phi21 = 0.0;
            let phi22 = decay;
            ((phi11, phi12, phi21, phi22), (0.0, dt))
        }
    }

    /// Plucks the string with given exciter, pluck position, and velocity.
    pub fn pluck(&mut self, exciter: &PluckExciter, pluck_pos_ratio: f64, velocity: f64) {
        let (q_t, q_p, v_t) = exciter.compute_initial_modal_displacements(
            self.effective_length,
            self.current_f0,
            pluck_pos_ratio,
            velocity,
            self.num_modes,
        );

        for m in 0..self.num_modes {
            self.state_t[m].q += q_t[m];
            self.state_t[m].v = v_t[m];

            self.state_p[m].q += q_p[m];
            self.state_p[m].v = 0.0;
        }
        self.is_held = true;
        self.is_releasing = false;
    }

    /// Advances the modal oscillators by 1 audio sample (dt).
    /// Returns the bridge vertical force (Newtons) and parallel force (Newtons).
    #[inline(always)]
    pub fn step(&mut self) -> (f64, f64) {
        let mut force_bridge_t = 0.0;
        let mut force_bridge_p = 0.0;
        let mut modal_sq_sum = 0.0;

        for m in 0..self.num_modes {
            let m_f = (m + 1) as f64;
            let wave_num = m_f * PI / self.effective_length;

            // Vertical (T) state step
            let st = &mut self.state_t[m];
            let pt = self.phi_t[m];
            let q_next_t = pt.0 * st.q + pt.1 * st.v;
            let v_next_t = pt.2 * st.q + pt.3 * st.v;
            st.q = q_next_t;
            st.v = v_next_t;

            // Horizontal (P) state step
            let sp = &mut self.state_p[m];
            let pp = self.phi_p[m];
            let q_next_p = pp.0 * sp.q + pp.1 * sp.v;
            let v_next_p = pp.2 * sp.q + pp.3 * sp.v;
            sp.q = q_next_p;
            sp.v = v_next_p;

            // Bridge shear force accumulation: F_bridge = T0 * d/dx phi(L)
            let grad = self.bridge_phi[m];
            force_bridge_t += self.params.tension * st.q * grad;
            force_bridge_p += self.params.tension * sp.q * grad;

            // Accumulate square wave numbers for geometric tension modulation
            modal_sq_sum += wave_num.powi(2) * (st.q.powi(2) + sp.q.powi(2));
        }

        // Signorini unilateral fret collision / Fret Buzz (docx Chapter 2)
        if self.fret_buzz_sensitivity > 0.05 {
            let x_buzz = 0.03 * self.effective_length;
            let mut u_buzz = 0.0;
            for m in 0..self.num_modes.min(10) {
                let m_f = (m + 1) as f64;
                u_buzz += self.state_t[m].q * (m_f * PI * x_buzz / self.effective_length).sin();
            }

            let clearance = 0.0012 * (1.2 - self.fret_buzz_sensitivity * 0.6);
            if u_buzz.abs() > clearance {
                let excess = u_buzz.abs() - clearance;
                let restitution_damping = 1.0 - (excess * 150.0).clamp(0.0, 0.35);
                for m in 0..self.num_modes {
                    self.state_t[m].v *= restitution_damping;
                }
            }
        }

        // Active release damping when note is explicitly released (~150ms finger mute)
        if self.is_releasing {
            let release_damping = 0.997_f64;
            let mut total_amp = 0.0_f64;
            for m in 0..self.num_modes {
                self.state_t[m].q *= release_damping;
                self.state_t[m].v *= release_damping;
                self.state_p[m].q *= release_damping;
                self.state_p[m].v *= release_damping;
                total_amp += self.state_t[m].q.abs() + self.state_p[m].q.abs();
            }
            // Clamp to zero to eliminate denormals and zero out energy cleanly
            if total_amp < 1e-7 {
                for m in 0..self.num_modes {
                    self.state_t[m].q = 0.0;
                    self.state_t[m].v = 0.0;
                    self.state_p[m].q = 0.0;
                    self.state_p[m].v = 0.0;
                }
                self.is_releasing = false;
            }
        }

        // Geometric tension modulation: Delta T = (E * A / 4L^2) * sum(k_m^2 * q_m^2)
        self.current_delta_t = self.geom_tension_coeff * modal_sq_sum;

        // Non-linear Kirchhoff-Carrier dynamic tension restoring force (twang & attack pitch drift)
        // Clamped to 0.04 (max ~35 cents pitch drift on attack) for absolute numerical stability
        let rel_delta_t = (self.current_delta_t / self.params.tension).clamp(0.0, 0.04);
        if rel_delta_t > 1e-6 {
            let tension_factor = rel_delta_t * (self.params.tension / self.params.linear_density) * self.dt;
            for m in 0..self.num_modes {
                let m_f = (m + 1) as f64;
                let k_sq = (m_f * PI / self.effective_length).powi(2);
                let delta_v = tension_factor * k_sq;
                self.state_t[m].v -= delta_v * self.state_t[m].q;
                self.state_p[m].v -= delta_v * self.state_p[m].q;
            }
        }

        (force_bridge_t, force_bridge_p)
    }

    /// Injects bridge vibration into string modes to enable sympathetic resonance between strings.
    #[inline(always)]
    pub fn inject_bridge_motion(&mut self, bridge_velocity: f64, coupling: f64) {
        if coupling <= 0.0 || bridge_velocity.abs() < 1e-12 {
            return;
        }
        let max_m = self.num_modes.min(16);
        for m in 0..max_m {
            let impulse = -coupling * bridge_velocity * self.bridge_phi[m] * self.dt;
            self.state_t[m].v += impulse;
        }
    }

    /// Evaluates total mechanical energy in string (Joules).
    pub fn total_energy(&self) -> f64 {
        let mut energy = 0.0;
        for m in 0..self.num_modes {
            energy += 0.5 * (self.state_t[m].v.powi(2) + self.omega_t[m].powi(2) * self.state_t[m].q.powi(2));
            energy += 0.5 * (self.state_p[m].v.powi(2) + self.omega_p[m].powi(2) * self.state_p[m].q.powi(2));
        }
        energy
    }

    /// Releases the string (switches to active finger release muting).
    pub fn release(&mut self) {
        self.is_held = false;
        self.is_releasing = true;
    }
}
