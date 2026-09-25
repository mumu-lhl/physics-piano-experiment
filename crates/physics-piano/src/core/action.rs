//! Micro-Mechanical Action Noise & Physical Articulations (Tier 6).
//!
//! Models the tactile, acoustic micro-mechanics of the grand piano action:
//! - Key-bottom thump (wooden key lever colliding with felt punching & keybed)
//! - Escapement jack let-off click (subtle mechanical snap before let-off)
//! - Key-up back-rail clack (key lever returning to rest felt on NoteOff)

use std::f64::consts::PI;

/// Direct Form II Transposed Biquad Filter.
#[derive(Debug, Clone, Copy)]
pub struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    z1: f64,
    z2: f64,
}

impl Biquad {
    pub fn zero() -> Self {
        Self {
            b0: 0.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            z1: 0.0,
            z2: 0.0,
        }
    }

    /// 2nd-order Bandpass Filter (constant 0 dB peak gain).
    pub fn bandpass(sample_rate: f64, freq: f64, q: f64) -> Self {
        let w0 = 2.0 * PI * (freq / sample_rate).clamp(0.0001, 0.499);
        let alpha = w0.sin() / (2.0 * q.max(0.1));
        let cos_w0 = w0.cos();

        let a0 = 1.0 + alpha;
        let b0 = alpha / a0;
        let b1 = 0.0;
        let b2 = -alpha / a0;
        let a1 = (-2.0 * cos_w0) / a0;
        let a2 = (1.0 - alpha) / a0;

        Self {
            b0,
            b1,
            b2,
            a1,
            a2,
            z1: 0.0,
            z2: 0.0,
        }
    }

    /// 2nd-order Modal Resonator with peak impulse response ~ 1.0.
    pub fn resonator(sample_rate: f64, freq: f64, q: f64) -> Self {
        let w0 = 2.0 * PI * (freq / sample_rate).clamp(0.0001, 0.499);
        let r = (-w0 / (2.0 * q.max(0.1))).exp();
        let cos_w0 = w0.cos();

        let a1 = -2.0 * r * cos_w0;
        let a2 = r * r;
        let b0 = w0.sin();
        let b1 = 0.0;
        let b2 = 0.0;

        Self {
            b0,
            b1,
            b2,
            a1,
            a2,
            z1: 0.0,
            z2: 0.0,
        }
    }

    /// 2nd-order Highpass Filter.
    pub fn highpass(sample_rate: f64, freq: f64, q: f64) -> Self {
        let w0 = 2.0 * PI * (freq / sample_rate).clamp(0.0001, 0.499);
        let alpha = w0.sin() / (2.0 * q.max(0.1));
        let cos_w0 = w0.cos();

        let a0 = 1.0 + alpha;
        let b0 = ((1.0 + cos_w0) / 2.0) / a0;
        let b1 = (-(1.0 + cos_w0)) / a0;
        let b2 = ((1.0 + cos_w0) / 2.0) / a0;
        let a1 = (-2.0 * cos_w0) / a0;
        let a2 = (1.0 - alpha) / a0;

        Self {
            b0,
            b1,
            b2,
            a1,
            a2,
            z1: 0.0,
            z2: 0.0,
        }
    }

    #[inline(always)]
    pub fn process(&mut self, input: f64) -> f64 {
        let out = self.b0 * input + self.z1;
        self.z1 = self.b1 * input - self.a1 * out + self.z2;
        self.z2 = self.b2 * input - self.a2 * out;
        out
    }

    pub fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }
}

/// Action & Keybed Mechanical Noise Synthesizer.
///
/// Models:
/// 1. Keybed Bottom Thump: Shared spruce keybed body modes excited by key-bottom impacts.
/// 2. Escapement Jack Let-Off Click: Micro-transient click when jack slips off roller.
/// 3. Key-Up Rail Clack: Soft back-rail cloth impact when keys are released.
pub struct KeyActionNoise {
    _sample_rate: f64,
    // Left & Right keybed modal resonators (72 Hz, 135 Hz, 240 Hz)
    thump_modes_l: [Biquad; 3],
    thump_modes_r: [Biquad; 3],

    // Escapement click bandpass resonators (3.8 kHz)
    escapement_mode_l: Biquad,
    escapement_mode_r: Biquad,

    // Key-up rail resonator (180 Hz)
    key_up_mode_l: Biquad,
    key_up_mode_r: Biquad,

    // Injection accumulators for the current sample
    impulse_thump_l: f64,
    impulse_thump_r: f64,
    impulse_escapement_l: f64,
    impulse_escapement_r: f64,
    impulse_keyup_l: f64,
    impulse_keyup_r: f64,

    // Master noise gain
    pub gain: f64,
}

impl KeyActionNoise {
    pub fn new(sample_rate: f64) -> Self {
        let thump_l = [
            Biquad::resonator(sample_rate, 72.0, 4.0),
            Biquad::resonator(sample_rate, 135.0, 6.5),
            Biquad::resonator(sample_rate, 240.0, 8.0),
        ];
        let thump_r = [
            Biquad::resonator(sample_rate, 72.0, 4.0),
            Biquad::resonator(sample_rate, 135.0, 6.5),
            Biquad::resonator(sample_rate, 240.0, 8.0),
        ];

        let esc_l = Biquad::resonator(sample_rate, 3800.0, 3.0);
        let esc_r = Biquad::resonator(sample_rate, 3800.0, 3.0);

        let up_l = Biquad::resonator(sample_rate, 180.0, 3.0);
        let up_r = Biquad::resonator(sample_rate, 180.0, 3.0);

        Self {
            _sample_rate: sample_rate,
            thump_modes_l: thump_l,
            thump_modes_r: thump_r,
            escapement_mode_l: esc_l,
            escapement_mode_r: esc_r,
            key_up_mode_l: up_l,
            key_up_mode_r: up_r,
            impulse_thump_l: 0.0,
            impulse_thump_r: 0.0,
            impulse_escapement_l: 0.0,
            impulse_escapement_r: 0.0,
            impulse_keyup_l: 0.0,
            impulse_keyup_r: 0.0,
            gain: 1.0,
        }
    }

    /// Trigger key strike mechanics (NoteOn): key-bottom thump and escapement click.
    pub fn trigger_note_on(&mut self, key: u8, velocity: f64) {
        // Normalized keyboard pan: 21 (A0) -> 0.0 (left), 108 (C8) -> 1.0 (right)
        let pan = ((key.clamp(21, 108) - 21) as f64 / 87.0).clamp(0.0, 1.0);
        let pan_l = (1.0 - pan).sqrt();
        let pan_r = pan.sqrt();

        // 1. Keybed Bottom Thump: Non-linear strike impulse ~ v^1.25
        let thump_amp = velocity.powf(1.25) * 0.035;
        self.impulse_thump_l += thump_amp * pan_l;
        self.impulse_thump_r += thump_amp * pan_r;

        // 2. Escapement Jack Let-Off Click:
        // More prominent in soft touch (v < 0.6), masked at high velocity.
        let esc_amp = (1.0 - 0.45 * velocity) * velocity.powf(0.5) * 0.012;
        self.impulse_escapement_l += esc_amp * pan_l;
        self.impulse_escapement_r += esc_amp * pan_r;
    }

    /// Trigger key release mechanics (NoteOff): back-rail felt clack.
    pub fn trigger_note_off(&mut self, key: u8, release_velocity: f64) {
        let pan = ((key.clamp(21, 108) - 21) as f64 / 87.0).clamp(0.0, 1.0);
        let pan_l = (1.0 - pan).sqrt();
        let pan_r = pan.sqrt();

        let clack_amp = release_velocity.clamp(0.1, 1.0).powf(1.1) * 0.018;
        self.impulse_keyup_l += clack_amp * pan_l;
        self.impulse_keyup_r += clack_amp * pan_r;
    }

    /// Process one audio sample, returning `(left, right)` mechanical noise.
    #[inline]
    pub fn step(&mut self) -> (f64, f64) {
        if self.gain <= 1e-5 {
            self.impulse_thump_l = 0.0;
            self.impulse_thump_r = 0.0;
            self.impulse_escapement_l = 0.0;
            self.impulse_escapement_r = 0.0;
            self.impulse_keyup_l = 0.0;
            self.impulse_keyup_r = 0.0;
            return (0.0, 0.0);
        }

        // Consume impulse injections
        let inp_t_l = self.impulse_thump_l;
        let inp_t_r = self.impulse_thump_r;
        let inp_e_l = self.impulse_escapement_l;
        let inp_e_r = self.impulse_escapement_r;
        let inp_u_l = self.impulse_keyup_l;
        let inp_u_r = self.impulse_keyup_r;

        self.impulse_thump_l = 0.0;
        self.impulse_thump_r = 0.0;
        self.impulse_escapement_l = 0.0;
        self.impulse_escapement_r = 0.0;
        self.impulse_keyup_l = 0.0;
        self.impulse_keyup_r = 0.0;

        // Process keybed modes
        let thump_out_l = self.thump_modes_l[0].process(inp_t_l) * 0.5
            + self.thump_modes_l[1].process(inp_t_l) * 0.35
            + self.thump_modes_l[2].process(inp_t_l) * 0.2;

        let thump_out_r = self.thump_modes_r[0].process(inp_t_r) * 0.5
            + self.thump_modes_r[1].process(inp_t_r) * 0.35
            + self.thump_modes_r[2].process(inp_t_r) * 0.2;

        // Process escapement click
        let esc_out_l = self.escapement_mode_l.process(inp_e_l);
        let esc_out_r = self.escapement_mode_r.process(inp_e_r);

        // Process key-up clack
        let up_out_l = self.key_up_mode_l.process(inp_u_l);
        let up_out_r = self.key_up_mode_r.process(inp_u_r);

        let out_l = (thump_out_l + esc_out_l + up_out_l) * self.gain;
        let out_r = (thump_out_r + esc_out_r + up_out_r) * self.gain;

        (out_l, out_r)
    }

    pub fn reset(&mut self) {
        for m in &mut self.thump_modes_l {
            m.reset();
        }
        for m in &mut self.thump_modes_r {
            m.reset();
        }
        self.escapement_mode_l.reset();
        self.escapement_mode_r.reset();
        self.key_up_mode_l.reset();
        self.key_up_mode_r.reset();
        self.impulse_thump_l = 0.0;
        self.impulse_thump_r = 0.0;
        self.impulse_escapement_l = 0.0;
        self.impulse_escapement_r = 0.0;
        self.impulse_keyup_l = 0.0;
        self.impulse_keyup_r = 0.0;
    }
}
