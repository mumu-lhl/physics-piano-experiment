//! Piano Bridge Anisotropic Admittance and Soundboard Radiation in Rust.

use std::f64::consts::PI;

pub struct BridgeSoundboard {
    pub sample_rate: f64,
    pub dt: f64,

    // 3x3 Admittance tensor coefficients
    pub y_tt: f64,
    pub y_pp: f64,
    pub y_tp: f64,
    pub y_pt: f64,
    pub y_tl: f64,
    pub y_lt: f64,
    pub y_ll: f64,
    pub y_pl: f64,
    pub y_lp: f64,

    // Soundboard modal filter bank
    pub body_freqs: [f64; 9],
    pub body_q: [f64; 9],
    pub body_gains: [f64; 9],
    pub num_body_modes: usize,

    // Filter coefficients: (b0, b2, a1, a2)
    pub biquad_coeffs: [(f64, f64, f64, f64); 9],

    // Filter states: (x1, x2, y1, y2)
    pub filter_states: [(f64, f64, f64, f64); 9],
}

impl BridgeSoundboard {
    pub fn new(sample_rate: f64) -> Self {
        let dt = 1.0 / sample_rate;
        let body_freqs = [85.0, 140.0, 220.0, 310.0, 480.0, 720.0, 1150.0, 1900.0, 3200.0];
        let body_q = [12.0, 14.0, 16.0, 18.0, 20.0, 22.0, 25.0, 28.0, 30.0];
        let body_gains = [1.2, 1.5, 1.4, 1.1, 0.9, 0.7, 0.5, 0.35, 0.2];

        let mut biquad_coeffs = [(0.0, 0.0, 0.0, 0.0); 9];
        for i in 0..9 {
            let w0 = 2.0 * PI * body_freqs[i] / sample_rate;
            let alpha = w0.sin() / (2.0 * body_q[i]);
            let cos_w0 = w0.cos();
            let a0 = 1.0 + alpha;

            let b0 = (alpha * body_gains[i]) / a0;
            let b2 = (-alpha * body_gains[i]) / a0;
            let a1 = (-2.0 * cos_w0) / a0;
            let a2 = (1.0 - alpha) / a0;

            biquad_coeffs[i] = (b0, b2, a1, a2);
        }

        Self {
            sample_rate,
            dt,
            y_tt: 1.8e-4,
            y_pp: 2.2e-5,
            y_tp: 3.5e-5,
            y_pt: 3.5e-5,
            y_tl: 1.5e-5,
            y_lt: 1.5e-5,
            y_ll: 4.0e-5,
            y_pl: 0.5e-5,
            y_lp: 0.5e-5,
            body_freqs,
            body_q,
            body_gains,
            num_body_modes: 9,
            biquad_coeffs,
            filter_states: [(0.0, 0.0, 0.0, 0.0); 9],
        }
    }

    #[inline]
    pub fn calculate_coupling_forces(&self, fb_t: f64, fb_p: f64, fb_l: f64) -> (f64, f64, f64) {
        let v_bridge_t = self.y_tt * fb_t + self.y_tp * fb_p + self.y_tl * fb_l;
        let v_bridge_p = self.y_pt * fb_t + self.y_pp * fb_p + self.y_pl * fb_l;

        let f_react_t = -v_bridge_t * 8.0;
        let f_react_p = -v_bridge_p * 12.0;

        let f_soundboard = 0.82 * fb_t + 0.15 * fb_p + 0.28 * fb_l;

        (f_react_t, f_react_p, f_soundboard)
    }

    #[inline]
    pub fn step_soundboard(&mut self, f_in: f64, pan: f64) -> (f64, f64) {
        let mut modal_sound = 0.0;

        for i in 0..self.num_body_modes {
            let (b0, b2, a1, a2) = self.biquad_coeffs[i];
            let (x1, x2, y1, y2) = self.filter_states[i];

            let y_mode = b0 * f_in + b2 * x2 - a1 * y1 - a2 * y2;

            self.filter_states[i] = (f_in, x1, y_mode, y1);
            modal_sound += y_mode;
        }

        let soundboard_out = 0.92 * modal_sound + 0.08 * f_in * 1e-4;

        let pan_clamped = pan.clamp(0.0, 1.0);
        let left_gain = (pan_clamped * PI * 0.5).cos();
        let right_gain = (pan_clamped * PI * 0.5).sin();

        (soundboard_out * left_gain, soundboard_out * right_gain)
    }
}
