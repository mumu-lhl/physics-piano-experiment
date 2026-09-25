//! Built-in Electric Guitar Tube Amp & 12-inch Cabinet Simulator.
//!
//! Provides out-of-the-box warm studio tone:
//! - 12AX7 asymmetric triode tube saturation curve
//! - 12-inch Celestion-style cabinet acoustic filtering (low-end thump, presence peak, high-end fizz cut)

use std::f64::consts::PI;

/// Asymmetric 12AX7 tube preamplifier saturation.
#[inline(always)]
fn tube_saturate(x: f64, drive: f64) -> f64 {
    let driven = x * (1.0 + drive * 4.0);
    // Asymmetric triode transfer function generating rich 2nd and 3rd harmonics
    if driven >= 0.0 {
        driven.tanh()
    } else {
        // Slight asymmetry on negative half-cycle
        (driven * 1.25).tanh() / 1.25
    }
}

/// 12-inch Celestion Vintage 30 Guitar Speaker Cabinet Filter.
/// Guitar speakers are band-limited acoustic transducers:
/// - Sub-bass cutoff below 75 Hz (removes muddiness)
/// - Cabinet mechanical resonance bump at ~100 Hz
/// - Midrange presence peak at ~2.8 kHz (classic guitar bite)
/// - Steep cone roll-off above 4.8 kHz (eliminates harsh digital fizz)
#[derive(Debug, Clone)]
pub struct GuitarCabinet {
    // 2-pole highpass (75 Hz)
    hp_x1: f64, hp_x2: f64, hp_y1: f64, hp_y2: f64,
    hp_b0: f64, hp_b1: f64, hp_b2: f64, hp_a1: f64, hp_a2: f64,

    // 2-pole presence peak (2800 Hz, Q=2.2, +4dB)
    pk_x1: f64, pk_x2: f64, pk_y1: f64, pk_y2: f64,
    pk_b0: f64, pk_b1: f64, pk_b2: f64, pk_a1: f64, pk_a2: f64,

    // 2-pole lowpass fizz cut (4800 Hz, Q=0.85)
    lp_x1: f64, lp_x2: f64, lp_y1: f64, lp_y2: f64,
    lp_b0: f64, lp_b1: f64, lp_b2: f64, lp_a1: f64, lp_a2: f64,
}

impl GuitarCabinet {
    pub fn new(sample_rate: f64) -> Self {
        // 1. Highpass at 75 Hz
        let w_hp = 2.0 * PI * 75.0 / sample_rate;
        let q_hp = 0.707;
        let alpha_hp = w_hp.sin() / (2.0 * q_hp);
        let cos_hp = w_hp.cos();
        let a0_hp = 1.0 + alpha_hp;
        let hp_b0 = ((1.0 + cos_hp) * 0.5) / a0_hp;
        let hp_b1 = (-(1.0 + cos_hp)) / a0_hp;
        let hp_b2 = hp_b0;
        let hp_a1 = (-2.0 * cos_hp) / a0_hp;
        let hp_a2 = (1.0 - alpha_hp) / a0_hp;

        // 2. Presence peak at 2800 Hz (+4dB)
        let w_pk = 2.0 * PI * 2800.0 / sample_rate;
        let q_pk = 2.0;
        let a_gain = 10.0f64.powf(4.0 / 40.0); // +4 dB
        let alpha_pk = w_pk.sin() / (2.0 * q_pk);
        let cos_pk = w_pk.cos();
        let a0_pk = 1.0 + alpha_pk / a_gain;
        let pk_b0 = (1.0 + alpha_pk * a_gain) / a0_pk;
        let pk_b1 = (-2.0 * cos_pk) / a0_pk;
        let pk_b2 = (1.0 - alpha_pk * a_gain) / a0_pk;
        let pk_a1 = (-2.0 * cos_pk) / a0_pk;
        let pk_a2 = (1.0 - alpha_pk / a_gain) / a0_pk;

        // 3. Lowpass fizz cut at 4800 Hz
        let w_lp = 2.0 * PI * 4800.0 / sample_rate;
        let q_lp = 0.85;
        let alpha_lp = w_lp.sin() / (2.0 * q_lp);
        let cos_lp = w_lp.cos();
        let a0_lp = 1.0 + alpha_lp;
        let lp_b0 = ((1.0 - cos_lp) * 0.5) / a0_lp;
        let lp_b1 = (1.0 - cos_lp) / a0_lp;
        let lp_b2 = lp_b0;
        let lp_a1 = (-2.0 * cos_lp) / a0_lp;
        let lp_a2 = (1.0 - alpha_lp) / a0_lp;

        Self {
            hp_x1: 0.0, hp_x2: 0.0, hp_y1: 0.0, hp_y2: 0.0,
            hp_b0, hp_b1, hp_b2, hp_a1, hp_a2,

            pk_x1: 0.0, pk_x2: 0.0, pk_y1: 0.0, pk_y2: 0.0,
            pk_b0, pk_b1, pk_b2, pk_a1, pk_a2,

            lp_x1: 0.0, lp_x2: 0.0, lp_y1: 0.0, lp_y2: 0.0,
            lp_b0, lp_b1, lp_b2, lp_a1, lp_a2,
        }
    }

    #[inline(always)]
    pub fn process(&mut self, input: f64) -> f64 {
        // Highpass
        let hp_out = self.hp_b0 * input + self.hp_b1 * self.hp_x1 + self.hp_b2 * self.hp_x2
            - self.hp_a1 * self.hp_y1 - self.hp_a2 * self.hp_y2;
        self.hp_x2 = self.hp_x1;
        self.hp_x1 = input;
        self.hp_y2 = self.hp_y1;
        self.hp_y1 = hp_out;

        // Presence peaking
        let pk_out = self.pk_b0 * hp_out + self.pk_b1 * self.pk_x1 + self.pk_b2 * self.pk_x2
            - self.pk_a1 * self.pk_y1 - self.pk_a2 * self.pk_y2;
        self.pk_x2 = self.pk_x1;
        self.pk_x1 = hp_out;
        self.pk_y2 = self.pk_y1;
        self.pk_y1 = pk_out;

        // Lowpass
        let lp_out = self.lp_b0 * pk_out + self.lp_b1 * self.lp_x1 + self.lp_b2 * self.lp_x2
            - self.lp_a1 * self.lp_y1 - self.lp_a2 * self.lp_y2;
        self.lp_x2 = self.lp_x1;
        self.lp_x1 = pk_out;
        self.lp_y2 = self.lp_y1;
        self.lp_y1 = lp_out;

        lp_out
    }
}

/// Integrated Electric Guitar Amp & Cabinet System.
#[derive(Debug, Clone)]
pub struct GuitarAmpCab {
    pub is_enabled: bool,
    pub cab_enabled: bool,
    pub drive: f64, // 0.0 = clean, 1.0 = crunch/lead
    pub cabinet: GuitarCabinet,
}

impl GuitarAmpCab {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            is_enabled: true,
            cab_enabled: true,
            drive: 0.25, // warm, dynamic clean/edge-of-breakup by default
            cabinet: GuitarCabinet::new(sample_rate),
        }
    }

    #[inline(always)]
    pub fn process(&mut self, di_input: f64) -> f64 {
        if !self.is_enabled {
            return di_input;
        }

        // 1. Tube preamp saturation
        let saturated = tube_saturate(di_input, self.drive);

        // 2. 12-inch guitar cabinet filtering (can be bypassed for external IRs)
        if self.cab_enabled {
            self.cabinet.process(saturated)
        } else {
            saturated
        }
    }
}
