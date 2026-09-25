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
            PluckStyle::Plectrum => (0.0015, 0.0006, PI / 6.0),    // 1.5 mm celluloid/tortex pick contact, 0.6ms release, 30 degrees
        };
        Self {
            style,
            half_width,
            release_time,
            angle_rad,
        }
    }

    /// Computes the initial modal displacements (q_T, q_P) for each mode m = 1..=num_modes
    /// given:
    /// - `length`: effective string length L
    /// - `pluck_pos_ratio`: pluck position x0 / L (typically 0.08 to 0.30)
    /// - `velocity`: MIDI velocity [0.0, 1.0]
    /// - `num_modes`: number of modes
    pub fn compute_initial_modal_displacements(
        &self,
        length: f64,
        pluck_pos_ratio: f64,
        velocity: f64,
        num_modes: usize,
    ) -> (Vec<f64>, Vec<f64>) {
        let x0 = (pluck_pos_ratio.clamp(0.04, 0.45)) * length;
        // Peak displacement y0 scaled by velocity (typically 0.5mm ~ 4.0mm)
        let y0 = 0.0005 + velocity * 0.0035;
        let bw = self.half_width;

        let mut q_t = Vec::with_capacity(num_modes);
        let mut q_p = Vec::with_capacity(num_modes);

        let cos_theta = self.angle_rad.cos();
        let sin_theta = self.angle_rad.sin();

        for m in 1..=num_modes {
            let m_f = m as f64;
            // 1. Analytical ideal triangular pluck displacement
            // q_m(0) = sqrt(2/L) * (y0 * L^2) / (pi^2 * m^2 * x0 * (L - x0)) * sin(m * pi * x0 / L)
            let sin_term = (m_f * PI * x0 / length).sin();
            let denom = PI.powi(2) * m_f.powi(2) * x0 * (length - x0);
            let q_ideal = (2.0 / length).sqrt() * (y0 * length.powi(2) / denom) * sin_term;

            // 2. Spatial contact width low-pass filtering:
            let alpha = m_f * PI * bw / length;
            let sinc_term = if alpha.abs() < 1e-6 {
                1.0
            } else {
                alpha.sin() / alpha
            };

            // 3. Release impedance attenuation |S(omega)| ~ 1 / sqrt(1 + (omega * tau_rel)^2)
            let omega_approx = m_f * 2.0 * PI * 220.0;
            let release_filter = 1.0 / (1.0 + (omega_approx * self.release_time).powi(2)).sqrt();

            let q_filtered = q_ideal * sinc_term * release_filter;

            // 4. Project into Vertical (T) and Horizontal (P) planes
            q_t.push(q_filtered * cos_theta);
            q_p.push(q_filtered * sin_theta);
        }

        (q_t, q_p)
    }
}
