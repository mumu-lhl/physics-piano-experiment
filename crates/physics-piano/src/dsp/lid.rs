//! Continuous Grand Piano Lid Opening Baffle & Acoustic Dispersion Model (Tier 7).
//!
//! Models:
//! - Continuous lid angle attenuation and acoustic shadowing (Closed -> Half -> Full -> Removed).
//! - Parameterized high-shelf diffraction filter.
//! - Early lid reflection comb/delay network.

use physics_dsp::Biquad;

/// High-shelf adapter around the shared biquad implementation.
#[derive(Debug, Clone, Copy)]
pub struct HighShelf(Biquad);

impl HighShelf {
    pub fn new(sample_rate: f64, frequency: f64, gain_db: f64) -> Self {
        Self(Biquad::high_shelf(sample_rate, frequency, gain_db))
    }

    pub fn update(&mut self, sample_rate: f64, frequency: f64, gain_db: f64) {
        self.0.set_high_shelf(sample_rate, frequency, gain_db);
    }

    #[inline(always)]
    pub fn process(&mut self, input: f64) -> f64 {
        self.0.process(input)
    }

    pub fn reset(&mut self) {
        self.0.reset();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LidPosition {
    Closed = 0,
    HalfStick = 1,
    FullStick = 2,
    Removed = 3,
}

impl LidPosition {
    pub fn from_index(idx: usize) -> Self {
        match idx {
            0 => LidPosition::Closed,
            1 => LidPosition::HalfStick,
            3 => LidPosition::Removed,
            _ => LidPosition::FullStick,
        }
    }

    pub fn to_angle_deg(self) -> f64 {
        match self {
            LidPosition::Closed => 0.0,
            LidPosition::HalfStick => 15.0,
            LidPosition::FullStick => 45.0,
            LidPosition::Removed => 60.0,
        }
    }
}

/// Delay-buffer length for a sample rate: 2.5 ms of headroom over the
/// longest (2.0 ms) lid reflection, so the tap always fits.
fn delay_buffer_len(sample_rate: f64) -> usize {
    ((0.0025 * sample_rate) as usize).max(2)
}

/// Number of samples a tap change crossfades over, so dragging the lid
/// does not jump the reflection read position (a click/zipper).
const TAP_CROSSFADE_SAMPLES: f64 = 64.0;

/// Grand Piano Lid Baffle & Acoustic Reflection Processor.
pub struct LidBaffle {
    sample_rate: f64,
    current_angle: f64,
    shelf_l: HighShelf,
    shelf_r: HighShelf,

    // Delay ring buffer for early lid reflection, sized from the sample
    // rate (2.5 ms) instead of a fixed 256 samples.
    delay_buf_l: Vec<f64>,
    delay_buf_r: Vec<f64>,
    delay_idx: usize,
    delay_samples: usize,
    /// Tap the crossfade started from while `tap_blend < 1.0`.
    prev_delay_samples: usize,
    /// 0.0 = fully the previous tap, 1.0 = fully the current tap.
    tap_blend: f64,
    reflection_gain: f64,
}

impl LidBaffle {
    pub fn new(sample_rate: f64) -> Self {
        let delay_len = delay_buffer_len(sample_rate);
        let mut baffle = Self {
            sample_rate,
            current_angle: f64::NAN, // Force the initial filter setup below.
            shelf_l: HighShelf::new(sample_rate, 3500.0, 0.0),
            shelf_r: HighShelf::new(sample_rate, 3500.0, 0.0),
            delay_buf_l: vec![0.0; delay_len],
            delay_buf_r: vec![0.0; delay_len],
            delay_idx: 0,
            delay_samples: ((0.0018 * sample_rate) as usize).clamp(1, delay_len - 1),
            prev_delay_samples: ((0.0018 * sample_rate) as usize).clamp(1, delay_len - 1),
            tap_blend: 1.0,
            reflection_gain: 0.25,
        };
        baffle.set_angle_deg(45.0);
        // The buffers start zeroed, so the initial tap needs no crossfade.
        baffle.tap_blend = 1.0;
        baffle.prev_delay_samples = baffle.delay_samples;
        baffle
    }

    /// Set continuous lid angle in degrees:
    /// - 0.0 deg: Closed (muffled, -12dB shelf at 2.2 kHz)
    /// - 15.0 deg: Half-stick (-5dB shelf at 3.2 kHz)
    /// - 45.0 deg: Full-stick (flat 0dB)
    /// - 60.0 deg: Removed (+1.5dB high presence)
    pub fn set_angle_deg(&mut self, angle_deg: f64) {
        let angle = angle_deg.clamp(0.0, 60.0);
        if (self.current_angle - angle).abs() <= 1e-4 {
            return;
        }
        self.current_angle = angle;

        let (gain_db, cutoff_freq, refl_gain) = if self.current_angle >= 55.0 {
            // Lid removed: open presence, no lid reflection
            (1.5, 6000.0, 0.0)
        } else {
            let norm = (self.current_angle / 45.0).clamp(0.0, 1.0);
            let gain = -12.0 + 12.0 * norm;
            let cutoff = 2200.0 + 2000.0 * norm;
            let refl = 0.28 * norm.sin();
            (gain, cutoff, refl)
        };

        self.shelf_l.update(self.sample_rate, cutoff_freq, gain_db);
        self.shelf_r.update(self.sample_rate, cutoff_freq, gain_db);
        self.reflection_gain = refl_gain;

        let delay_s = 0.0012 + 0.0008 * (1.0 - (self.current_angle / 45.0).clamp(0.0, 1.0));
        let new_delay =
            ((delay_s * self.sample_rate) as usize).clamp(1, self.delay_buf_l.len() - 1);
        if new_delay != self.delay_samples {
            // Crossfade the reflection tap instead of jumping it, so a
            // lid-angle change cannot click.
            self.prev_delay_samples = self.delay_samples;
            self.delay_samples = new_delay;
            self.tap_blend = 0.0;
        }
    }

    pub fn set_position(&mut self, pos: LidPosition) {
        self.set_angle_deg(pos.to_angle_deg());
    }

    #[inline]
    pub fn process(&mut self, in_l: f64, in_r: f64) -> (f64, f64) {
        // 1. High-shelf baffle filtering
        let filt_l = self.shelf_l.process(in_l);
        let filt_r = self.shelf_r.process(in_r);

        // 2. Early lid reflection delay path, crossfading between the
        // previous and current taps while the tap is moving.
        let buf_len = self.delay_buf_l.len();
        let read_idx = (self.delay_idx + buf_len - self.delay_samples) % buf_len;
        let prev_idx = (self.delay_idx + buf_len - self.prev_delay_samples) % buf_len;
        let blend = self.tap_blend;
        let refl_l =
            self.delay_buf_l[prev_idx] * (1.0 - blend) + self.delay_buf_l[read_idx] * blend;
        let refl_r =
            self.delay_buf_r[prev_idx] * (1.0 - blend) + self.delay_buf_r[read_idx] * blend;

        self.delay_buf_l[self.delay_idx] = in_l;
        self.delay_buf_r[self.delay_idx] = in_r;
        self.delay_idx = (self.delay_idx + 1) % buf_len;
        if self.tap_blend < 1.0 {
            self.tap_blend = (self.tap_blend + 1.0 / TAP_CROSSFADE_SAMPLES).min(1.0);
            if self.tap_blend >= 1.0 {
                self.prev_delay_samples = self.delay_samples;
            }
        }

        let out_l = filt_l + refl_l * self.reflection_gain;
        let out_r = filt_r + refl_r * self.reflection_gain;

        (out_l, out_r)
    }

    pub fn reset(&mut self) {
        self.shelf_l.reset();
        self.shelf_r.reset();
        self.delay_buf_l.fill(0.0);
        self.delay_buf_r.fill(0.0);
        self.delay_idx = 0;
        self.tap_blend = 1.0;
        self.prev_delay_samples = self.delay_samples;
    }
}

#[cfg(test)]
mod tests {
    use super::LidBaffle;

    #[test]
    fn delay_buffer_scales_with_sample_rate_and_tap_is_clamped() {
        let mut baffle = LidBaffle::new(192_000.0);
        assert_eq!(baffle.delay_buf_l.len(), 480);
        assert_eq!(baffle.delay_buf_r.len(), 480);
        // Widest reflection (lid closed, 2.0 ms) must fit with headroom.
        baffle.set_angle_deg(0.0);
        assert_eq!(baffle.delay_samples, 384);
        baffle.set_angle_deg(60.0);
        assert!(baffle.delay_samples >= 1 && baffle.delay_samples < 480);
        // Low rates keep a usable buffer too.
        let low = LidBaffle::new(8000.0);
        assert_eq!(low.delay_buf_l.len(), 20);
    }

    #[test]
    fn tap_change_crossfades_over_64_samples() {
        let mut baffle = LidBaffle::new(48_000.0);
        let before = baffle.delay_samples;
        assert_eq!(baffle.tap_blend, 1.0);
        baffle.set_angle_deg(0.0);
        assert_ne!(baffle.delay_samples, before);
        assert_eq!(baffle.prev_delay_samples, before);
        assert_eq!(baffle.tap_blend, 0.0);
        for _ in 0..63 {
            baffle.process(0.0, 0.0);
            assert!(baffle.tap_blend < 1.0);
        }
        baffle.process(0.0, 0.0);
        assert_eq!(baffle.tap_blend, 1.0);
        assert_eq!(baffle.prev_delay_samples, baffle.delay_samples);
        // Re-setting the same angle must not restart the crossfade.
        baffle.set_angle_deg(0.0);
        assert_eq!(baffle.tap_blend, 1.0);
    }

    #[test]
    fn reflection_arrives_at_the_configured_delay() {
        let mut baffle = LidBaffle::new(48_000.0);
        baffle.set_angle_deg(45.0);
        let delay = baffle.delay_samples;
        let mut outputs = Vec::new();
        for i in 0..delay + 4 {
            let input = if i == 0 { 1.0 } else { 0.0 };
            let (l, _) = baffle.process(input, input);
            outputs.push(l);
        }
        // The shelf is flat at 45 degrees, so the reflection peaks
        // exactly at the configured delay and nothing leaks before it.
        assert!(outputs[delay].abs() > 0.1);
        assert!(outputs[1..delay].iter().all(|x| x.abs() < 0.05));
    }
}
