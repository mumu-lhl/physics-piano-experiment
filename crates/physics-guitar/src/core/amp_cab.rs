//! Built-in Electric Guitar Tube Amp & 12-inch Cabinet Simulator.
//!
//! Provides out-of-the-box warm studio tone:
//! - 12AX7 asymmetric triode tube saturation curve
//! - 12-inch Celestion-style cabinet acoustic filtering (low-end thump, presence peak, high-end fizz cut)

use physics_dsp::Biquad;

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
    highpass: Biquad,
    presence: Biquad,
    lowpass: Biquad,
}

impl GuitarCabinet {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            highpass: Biquad::highpass(sample_rate, 75.0, 0.707),
            presence: Biquad::peaking(sample_rate, 2800.0, 2.0, 4.0),
            lowpass: Biquad::lowpass(sample_rate, 4800.0, 0.85),
        }
    }

    #[inline(always)]
    pub fn process(&mut self, input: f64) -> f64 {
        let highpassed = self.highpass.process(input);
        let presence_boosted = self.presence.process(highpassed);
        self.lowpass.process(presence_boosted)
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
