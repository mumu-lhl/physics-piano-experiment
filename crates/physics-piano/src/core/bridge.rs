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

    // Filter states: Left & Right (x1, x2, y1, y2)
    pub filter_states: [(f64, f64, f64, f64); 9],
    pub filter_states_r: [(f64, f64, f64, f64); 9],

    // Multi-microphone perspective gains
    pub close_gain: f64,
    pub player_gain: f64,
    pub ambient_gain: f64,
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
            filter_states_r: [(0.0, 0.0, 0.0, 0.0); 9],
            close_gain: 1.0,
            player_gain: 0.707,
            ambient_gain: 0.501,
        }
    }

    pub fn set_mic_gains(&mut self, close: f64, player: f64, ambient: f64) {
        self.close_gain = close;
        self.player_gain = player;
        self.ambient_gain = ambient;
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

    /// Step soundboard with independent Left and Right bridge driving forces (True Stereo Soundstage).
    #[inline]
    pub fn step_soundboard_stereo(&mut self, f_in_l: f64, f_in_r: f64) -> (f64, f64) {
        let mut modes_l = 0.0;
        let mut modes_r = 0.0;

        for i in 0..self.num_body_modes {
            let (b0, b2, a1, a2) = self.biquad_coeffs[i];

            // Left modal filter
            let (x1_l, x2_l, y1_l, y2_l) = self.filter_states[i];
            let y_mode_l = b0 * f_in_l + b2 * x2_l - a1 * y1_l - a2 * y2_l;
            self.filter_states[i] = (f_in_l, x1_l, y_mode_l, y1_l);
            modes_l += y_mode_l;

            // Right modal filter
            let (x1_r, x2_r, y1_r, y2_r) = self.filter_states_r[i];
            let y_mode_r = b0 * f_in_r + b2 * x2_r - a1 * y1_r - a2 * y2_r;
            self.filter_states_r[i] = (f_in_r, x1_r, y_mode_r, y1_r);
            modes_r += y_mode_r;
        }

        let direct_l = f_in_l * 1e-4;
        let direct_r = f_in_r * 1e-4;

        // Perspective 1: Close Mic (crisp near-field strike attack with wide stereo spread)
        let close_l = 0.88 * modes_l + 0.12 * modes_r + 0.15 * direct_l;
        let close_r = 0.12 * modes_l + 0.88 * modes_r + 0.15 * direct_r;

        // Perspective 2: Player Mic (seated pianist binaural perspective with natural cross-bleed)
        let player_l = 0.68 * modes_l + 0.32 * modes_r + 0.05 * direct_l;
        let player_r = 0.32 * modes_l + 0.68 * modes_r + 0.05 * direct_r;

        // Perspective 3: Ambient Mic (diffuse hall reverberation)
        let amb_l = 0.50 * modes_l + 0.50 * modes_r;
        let amb_r = 0.50 * modes_l + 0.50 * modes_r;

        let out_l = (self.close_gain * close_l + self.player_gain * player_l + self.ambient_gain * amb_l) * 0.6;
        let out_r = (self.close_gain * close_r + self.player_gain * player_r + self.ambient_gain * amb_r) * 0.6;

        (out_l, out_r)
    }

    /// Single point bridge excitation with pan angle (backwards compatible helper).
    #[inline]
    pub fn step_soundboard(&mut self, f_in: f64, pan: f64) -> (f64, f64) {
        let pan_clamped = pan.clamp(0.0, 1.0);
        let pan_l = ((1.0 - pan_clamped) * PI * 0.5).sin();
        let pan_r = (pan_clamped * PI * 0.5).sin();
        self.step_soundboard_stereo(f_in * pan_l, f_in * pan_r)
    }
}
