//! Dual-polarization stiff string modal engine specialized for 6-string guitar.
//! Supports dynamic fretting, geometric tension modulation, palm muting, and pitch bending.

use crate::core::pluck::PluckExciter;
use crate::params::GuitarStringParams;
use physics_dsp::{ModalTransition, OverdampedPolicy};
use std::f64::consts::PI;

#[repr(C, align(64))]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
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
    /// Continuous legato slide state
    pub slide_active: bool,
    pub slide_target_fret: u8,
    pub slide_start_length: f64,
    pub slide_target_length: f64,
    pub slide_samples_total: usize,
    pub slide_samples_left: usize,
    pub slide_friction_noise: f64,
    pub is_sleeping: bool,
    silence_counter: usize,
}

impl GuitarString {
    pub fn new(params: GuitarStringParams, sample_rate: f64) -> Self {
        let dt = 1.0 / sample_rate;
        let scale_length = params.scale_length;
        let mut s = Self {
            effective_length: scale_length,
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
            slide_active: false,
            slide_target_fret: 0,
            slide_start_length: scale_length,
            slide_target_length: scale_length,
            slide_samples_total: 0,
            slide_samples_left: 0,
            slide_friction_noise: 0.0,
            is_sleeping: false,
            silence_counter: 0,
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
        let e = self.params.youngs_modulus;
        let i_area = self.params.moment_of_inertia;
        let b = (PI.powi(3) * e * i_area) / (4.0 * tension * length.powi(2));

        // The published gauge/tension values are approximate and do not always
        // reproduce the documented standard tuning when used as T and mu
        // directly. Calibrate the fundamental to the string's specified fret
        // frequency, while retaining the physical stiffness ratio and bend.
        let target_f0 = self.current_f0 * bend_factor.sqrt();
        let omega_0 = 2.0 * PI * target_f0 / (1.0 + b).sqrt();
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
            let mut sigma_m = self.params.sigma0 + self.params.sigma1 * (m_f * PI / length).powi(2);
            if self.palm_mute_depth > 0.0 {
                // Multi-zone Kelvin-Voigt viscoelastic palm absorber (15~25mm from the bridge saddle)
                let palm_width = 0.022; // 22mm palm contact width from bridge saddle
                let w_ratio = (palm_width / length).min(0.25);
                // Spatial coverage: integral_0^W phi_m^2(x) dx = W/L - sin(2 m pi W/L) / (2 m pi)
                let spatial_coverage = (w_ratio - (2.0 * m_f * PI * w_ratio).sin() / (2.0 * m_f * PI)).max(0.0005);
                // Flesh viscoelastic strain-rate absorption (Kelvin-Voigt eta * d/dt)
                let viscoelastic_factor = 1.0 + 0.00028 * omega_base;
                let palm_damping = self.palm_mute_depth * 2500.0 * spatial_coverage * viscoelastic_factor;
                sigma_m += palm_damping;
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
            let transition_t = ModalTransition::new(
                omega_m_t,
                sigma_m,
                self.dt,
                OverdampedPolicy::ExponentialFallback,
            );
            let transition_p = ModalTransition::new(
                omega_m_p,
                sigma_m_p,
                self.dt,
                OverdampedPolicy::ExponentialFallback,
            );

            self.phi_t.push(transition_t.phi);
            self.gamma_t.push(transition_t.gamma);
            self.omega_t.push(omega_m_t);

            self.phi_p.push(transition_p.phi);
            self.gamma_p.push(transition_p.gamma);
            self.omega_p.push(omega_m_p);

            // Mode shape spatial gradient at bridge pin x = L: d/dx phi_m(L) = sqrt(2/L) * (m*pi/L) * (-1)^m
            let bridge_grad =
                (2.0 / length).sqrt() * (m_f * PI / length) * if m % 2 == 0 { 1.0 } else { -1.0 };
            self.bridge_phi.push(bridge_grad);
        }

        self.num_modes = active_modes;
        self.state_t
            .resize(active_modes, ModalState { q: 0.0, v: 0.0 });
        self.state_p
            .resize(active_modes, ModalState { q: 0.0, v: 0.0 });
    }

    /// Plucks the string with given exciter, pluck position, and velocity.
    pub fn pluck(&mut self, exciter: &PluckExciter, pluck_pos_ratio: f64, velocity: f64) {
        let length = self.effective_length;
        let f0 = self.current_f0;
        let num_modes = self.num_modes;
        exciter.for_each_initial_modal_displacement(
            length,
            f0,
            pluck_pos_ratio,
            velocity,
            num_modes,
            |mode, q_t, q_p, v_t| {
                // Plectrum contact damping on vibrating string:
                // Touching a vibrating string with a pick or finger partially arrests prior motion
                // rather than purely summing displacement, preventing unbounded energy accumulation
                self.state_t[mode].q = self.state_t[mode].q * 0.25 + q_t;
                self.state_t[mode].v = v_t;
                self.state_p[mode].q = self.state_p[mode].q * 0.25 + q_p;
                self.state_p[mode].v = 0.0;
            },
        );
        self.is_held = true;
        self.is_releasing = false;
        self.is_sleeping = false;
        self.silence_counter = 0;
    }

    /// Transitions between frets on the vibrating string without re-initializing modal oscillators.
    /// Projects existing modal coordinates onto the new vibrating scale length L_eff,
    /// preserving stored vibrational energy, and injects hammer-on/pull-off transients.
    pub fn legato_fret(&mut self, new_fret: u8, velocity: f64) {
        if new_fret == self.current_fret {
            return;
        }

        let is_hammer_on = new_fret > self.current_fret;
        let old_length = self.effective_length;
        let old_modes = self.num_modes;
        let old_state_t = self.state_t.clone();
        let old_state_p = self.state_p.clone();

        // Configure new fret
        self.current_fret = new_fret;
        self.effective_length = self.params.effective_length_at_fret(new_fret);
        self.current_f0 = self.params.frequency_at_fret(new_fret);
        self.recalculate_modal_operators();

        let new_length = self.effective_length;
        let l_min = old_length.min(new_length);
        let new_modes = self.num_modes;

        // Analytical modal projection: C_{k,m} = (2 / sqrt(L1 * L2)) * integral_0^Lmin sin(m*pi*x/L1) sin(k*pi*x/L2) dx
        let norm = 2.0 / (old_length * new_length).sqrt();
        for k in 0..new_modes {
            let k_f = (k + 1) as f64;
            let beta = k_f * PI / new_length;
            let mut q_proj_t = 0.0;
            let mut v_proj_t = 0.0;
            let mut q_proj_p = 0.0;
            let mut v_proj_p = 0.0;

            for m in 0..old_modes {
                let m_f = (m + 1) as f64;
                let alpha = m_f * PI / old_length;
                let diff = (alpha - beta).abs();
                let sum = alpha + beta;
                let i_km = if diff < 1e-7 {
                    0.5 * (l_min - (sum * l_min).sin() / sum)
                } else {
                    0.5 * (((diff * l_min).sin() / (alpha - beta)) - ((sum * l_min).sin() / sum))
                };
                let c_km = norm * i_km;
                q_proj_t += c_km * old_state_t[m].q;
                v_proj_t += c_km * old_state_t[m].v;
                q_proj_p += c_km * old_state_p[m].q;
                v_proj_p += c_km * old_state_p[m].v;
            }

            // Articulation transient:
            if is_hammer_on {
                // Localized fret-strike metallic impact at the stopping fret boundary
                let hammer_impulse = 0.00025 * velocity;
                let sign = if (k + 1) % 2 == 0 { 1.0 } else { -1.0 };
                let modal_impact = hammer_impulse * sign * (k_f / new_modes as f64).sqrt();
                v_proj_t += modal_impact;
            } else {
                // Finger-pad release step for pull-off at old fret position
                let pull_transient = 0.00012 * velocity * (k_f * PI * old_length / new_length).sin();
                q_proj_t += pull_transient;
            }

            self.state_t[k] = ModalState { q: q_proj_t, v: v_proj_t };
            self.state_p[k] = ModalState { q: q_proj_p, v: v_proj_p };
        }

        self.is_held = true;
        self.is_releasing = false;
        self.is_sleeping = false;
        self.silence_counter = 0;
    }

    /// Triggers a natural harmonic at a specified node (2 = 12th fret, 3 = 7th fret, 4 = 5th fret, 5 = 4th fret).
    pub fn trigger_natural_harmonic(
        &mut self,
        exciter: &PluckExciter,
        node: u8,
        velocity: f64,
    ) {
        self.harmonic_node = node;
        self.recalculate_modal_operators();
        let node_ratio = 1.0 / (node as f64);
        let pluck_pos = (node_ratio * 0.5).clamp(0.05, 0.45);
        self.pluck(exciter, pluck_pos, velocity);

        let n_f = node as f64;
        for m in 0..self.num_modes {
            let m_f = (m + 1) as f64;
            let antinode_distance = (m_f * PI / n_f).sin().abs();
            if antinode_distance > 0.05 {
                // Suppress non-harmonic overtone modes
                self.state_t[m].q *= 0.04;
                self.state_t[m].v *= 0.04;
                self.state_p[m].q *= 0.04;
                self.state_p[m].v *= 0.04;
            }
        }
    }

    /// Triggers an aggressive rock/metal pinch harmonic (pick excitation + immediate thumb node damping).
    pub fn trigger_pinch_harmonic(
        &mut self,
        exciter: &PluckExciter,
        node_ratio: f64,
        velocity: f64,
    ) {
        self.harmonic_node = 0;
        self.recalculate_modal_operators();
        let node_ratio = node_ratio.clamp(0.08, 0.35);
        self.pluck(exciter, node_ratio, velocity * 1.15);

        for m in 0..self.num_modes {
            let m_f = (m + 1) as f64;
            let pinch_factor = if m_f <= 2.0 {
                0.005 // Heavily suppress low fundamental & 2nd harmonic under thumb contact
            } else {
                let antinode = (m_f * PI * node_ratio).sin().abs();
                (1.0 - 0.85 * antinode).clamp(0.12, 1.40)
            };
            self.state_t[m].q *= pinch_factor;
            self.state_t[m].v *= pinch_factor;
            self.state_p[m].q *= pinch_factor;
            self.state_p[m].v *= pinch_factor;
        }
    }

    /// Triggers a percussive tap harmonic at a specified fret above the fretted note.
    pub fn trigger_tap_harmonic(&mut self, tap_node_fret: u8, velocity: f64) {
        let tap_ratio = 2.0_f64.powf(-(tap_node_fret as f64) / 12.0);
        let tap_impulse = 0.00045 * velocity;
        for m in 0..self.num_modes {
            let m_f = (m + 1) as f64;
            let mode_amp = (m_f * PI * tap_ratio).sin();
            self.state_t[m].v += tap_impulse * mode_amp;
        }
        self.is_held = true;
        self.is_releasing = false;
        self.is_sleeping = false;
        self.silence_counter = 0;
    }

    /// Initiates a continuous legato slide from current fret to target_fret over duration_ms.
    pub fn start_slide(&mut self, target_fret: u8, duration_ms: f64) {
        if target_fret == self.current_fret {
            return;
        }
        self.slide_active = true;
        self.is_sleeping = false;
        self.silence_counter = 0;
        self.slide_target_fret = target_fret;
        self.slide_start_length = self.effective_length;
        self.slide_target_length = self.params.effective_length_at_fret(target_fret);
        let samples = (self.sample_rate * (duration_ms / 1000.0).max(0.01)).round() as usize;
        self.slide_samples_total = samples.max(1);
        self.slide_samples_left = self.slide_samples_total;
        self.slide_friction_noise = 0.0;
    }

    /// Advances the modal oscillators by 1 audio sample (dt).
    /// Returns the bridge vertical force (Newtons) and parallel force (Newtons).
    #[inline(always)]
    pub fn step(&mut self) -> (f64, f64) {
        if self.is_sleeping {
            return (0.0, 0.0);
        }
        // Continuous legato slide progress
        if self.slide_active {
            if self.slide_samples_left > 0 {
                let progress = 1.0 - (self.slide_samples_left as f64 / self.slide_samples_total as f64);
                self.slide_samples_left -= 1;
                let smooth_p = 0.5 * (1.0 - (PI * progress).cos());
                self.effective_length = self.slide_start_length
                    + (self.slide_target_length - self.slide_start_length) * smooth_p;
                let f0_start = self.params.frequency_at_fret(self.current_fret);
                let f0_target = self.params.frequency_at_fret(self.slide_target_fret);
                self.current_f0 = f0_start * (f0_target / f0_start).powf(smooth_p);

                // Slight fret friction dissipation on string during slide
                let slide_speed = (self.slide_target_length - self.slide_start_length).abs()
                    / (self.slide_samples_total as f64 * self.dt);
                for m in 0..self.num_modes {
                    let m_f = (m + 1) as f64;
                    let friction_damping = 1.0 - (0.000006 * slide_speed * m_f).clamp(0.0, 0.002);
                    self.state_t[m].v *= friction_damping;
                    self.state_p[m].v *= friction_damping;
                }
            } else {
                self.slide_active = false;
                self.set_fret(self.slide_target_fret);
            }
        }

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
            let tension_factor =
                rel_delta_t * (self.params.tension / self.params.linear_density) * self.dt;
            for m in 0..self.num_modes {
                let m_f = (m + 1) as f64;
                let k_sq = (m_f * PI / self.effective_length).powi(2);
                let delta_v = tension_factor * k_sq;
                self.state_t[m].v -= delta_v * self.state_t[m].q;
                self.state_p[m].v -= delta_v * self.state_p[m].q;
            }
        }

        // Dynamic silence culling: when string is not held/sliding and modal energy is infinitesimal
        if !self.is_held && !self.slide_active {
            let mut total_modal_sq = 0.0_f64;
            for m in 0..self.num_modes {
                total_modal_sq += self.state_t[m].q * self.state_t[m].q
                    + self.state_t[m].v * self.state_t[m].v
                    + self.state_p[m].q * self.state_p[m].q
                    + self.state_p[m].v * self.state_p[m].v;
            }
            if total_modal_sq < 1e-15 {
                self.silence_counter += 1;
                if self.silence_counter > 64 {
                    self.is_sleeping = true;
                    for m in 0..self.num_modes {
                        self.state_t[m] = ModalState::default();
                        self.state_p[m] = ModalState::default();
                    }
                }
            } else {
                self.silence_counter = 0;
            }
        } else {
            self.silence_counter = 0;
        }

        (force_bridge_t, force_bridge_p)
    }

    /// Injects bridge vibration into string modes to enable sympathetic resonance between strings.
    #[inline(always)]
    pub fn inject_bridge_motion(&mut self, bridge_velocity: f64, coupling: f64) {
        if coupling <= 0.0 || bridge_velocity.abs() < 1e-12 {
            return;
        }
        if bridge_velocity.abs() > 1e-6 {
            self.is_sleeping = false;
        }
        let max_m = self.num_modes.min(16);
        for m in 0..max_m {
            let impulse = -coupling * bridge_velocity * self.bridge_phi[m] * self.dt;
            self.state_t[m].v += impulse;
        }
    }

    /// Injects external acoustic sound pressure (e.g. from amplifier cabinet speakers)
    /// into string modes, reproducing authentic electric guitar Larsen effect singing feedback.
    #[inline(always)]
    pub fn inject_acoustic_pressure(&mut self, pressure: f64, coupling: f64) {
        if coupling <= 0.0 || pressure.abs() < 1e-12 {
            return;
        }
        if pressure.abs() > 1e-5 {
            self.is_sleeping = false;
        }
        let max_m = self.num_modes.min(10);
        let x_spk = 0.35;
        let dt = self.dt;
        for m in 0..max_m {
            let m_f = (m + 1) as f64;
            let spatial_factor = (m_f * PI * x_spk).sin();
            let impulse = coupling * pressure * spatial_factor * dt * 1800.0;
            self.state_t[m].v += impulse;
        }
    }

    /// Evaluates total mechanical energy in string (Joules).
    pub fn total_energy(&self) -> f64 {
        let mut energy = 0.0;
        for m in 0..self.num_modes {
            energy += 0.5
                * (self.state_t[m].v.powi(2) + self.omega_t[m].powi(2) * self.state_t[m].q.powi(2));
            energy += 0.5
                * (self.state_p[m].v.powi(2) + self.omega_p[m].powi(2) * self.state_p[m].q.powi(2));
        }
        energy
    }

    /// Releases the string (switches to active finger release muting).
    pub fn release(&mut self) {
        self.is_held = false;
        self.is_releasing = true;
    }
}
