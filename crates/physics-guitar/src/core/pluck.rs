//! Plectrum and Finger Pluck Dynamics based on analytical modal projection,
//! finite contact width spatial filtering, and release time mechanical impedance.

use std::f64::consts::PI;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PluckStyle {
    /// Soft finger flesh contact (warm, round tone, longer release time)
    FingerFlesh,
    /// Hard celluloid / tortex plectrum or fingernail (bright, snappy attack)
    Plectrum,
}

#[derive(Debug, Clone)]
pub struct PluckExciter {
    pub style: PluckStyle,
    /// Effective contact half-width b_w (meters)
    pub half_width: f64,
    /// Dynamic snap-off release time tau_rel (seconds)
    pub release_time: f64,
    /// Pluck angle relative to soundboard normal (radians)
    pub angle_rad: f64,
}

impl PluckExciter {
    pub fn new(style: PluckStyle) -> Self {
        let (half_width, release_time, angle_rad) = match style {
            PluckStyle::FingerFlesh => (0.0050, 0.0025, PI / 4.0), // 5.0 mm flesh width, 2.5ms release, 45 degrees
            PluckStyle::Plectrum => (0.0015, 0.0006, PI / 6.0), // 1.5 mm celluloid/tortex pick contact, 0.6ms release, 30 degrees
        };
        Self {
            style,
            half_width,
            release_time,
            angle_rad,
        }
    }

    /// Projects each mode's initial state directly to `visit_mode` without allocating.
    /// `mode_index` is zero-based and the callback receives `(q_T, q_P, v_T)`.
    pub fn for_each_initial_modal_displacement(
        &self,
        length: f64,
        f0: f64,
        pluck_pos_ratio: f64,
        velocity: f64,
        num_modes: usize,
        mut visit_mode: impl FnMut(usize, f64, f64, f64),
    ) {
        let x0 = pluck_pos_ratio.clamp(0.04, 0.45) * length;
        let y0 = 0.0005 + velocity * 0.0035;
        let cos_theta = self.angle_rad.cos();
        let sin_theta = self.angle_rad.sin();
        let snap_velocity = match self.style {
            PluckStyle::Plectrum => velocity * 0.08,
            PluckStyle::FingerFlesh => velocity * 0.015,
        };

        for mode_index in 0..num_modes {
            let m = (mode_index + 1) as f64;
            let sin_term = (m * PI * x0 / length).sin();
            let denom = PI.powi(2) * m.powi(2) * x0 * (length - x0);
            let q_ideal = (2.0 / length).sqrt() * (y0 * length.powi(2) / denom) * sin_term;
            let alpha = m * PI * self.half_width / length;
            let sinc_term = if alpha.abs() < 1e-6 {
                1.0
            } else {
                alpha.sin() / alpha
            };
            let omega_m = m * 2.0 * PI * f0;
            let release_filter = 1.0 / (1.0 + (omega_m * self.release_time).powi(2)).sqrt();
            let q_filtered = q_ideal * sinc_term * release_filter;
            let q_t = q_filtered * cos_theta;
            let q_p = q_filtered * sin_theta;
            let v_t = (2.0 / length).sqrt() * snap_velocity * sin_term * release_filter * cos_theta;
            visit_mode(mode_index, q_t, q_p, v_t);
        }
    }

    /// Computes modal initial states into owned vectors for offline analysis and tests.
    /// Audio-thread code should use [`Self::for_each_initial_modal_displacement`] instead.
    pub fn compute_initial_modal_displacements(
        &self,
        length: f64,
        f0: f64,
        pluck_pos_ratio: f64,
        velocity: f64,
        num_modes: usize,
    ) -> (Vec<f64>, Vec<f64>, Vec<f64>) {
        let mut q_t = Vec::with_capacity(num_modes);
        let mut q_p = Vec::with_capacity(num_modes);
        let mut v_t = Vec::with_capacity(num_modes);
        self.for_each_initial_modal_displacement(
            length,
            f0,
            pluck_pos_ratio,
            velocity,
            num_modes,
            |_, mode_q_t, mode_q_p, mode_v_t| {
                q_t.push(mode_q_t);
                q_p.push(mode_q_p);
                v_t.push(mode_v_t);
            },
        );
        (q_t, q_p, v_t)
    }
}
