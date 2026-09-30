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

/// Grand Piano Lid Baffle & Acoustic Reflection Processor.
pub struct LidBaffle {
    sample_rate: f64,
    current_angle: f64,
    shelf_l: HighShelf,
    shelf_r: HighShelf,

    // Delay ring buffer for early lid reflection (~2.5ms max = 256 samples)
    delay_buf_l: [f64; 256],
    delay_buf_r: [f64; 256],
    delay_idx: usize,
    delay_samples: usize,
    reflection_gain: f64,
}

impl LidBaffle {
    pub fn new(sample_rate: f64) -> Self {
        let mut baffle = Self {
            sample_rate,
            current_angle: f64::NAN, // Force the initial filter setup below.
            shelf_l: HighShelf::new(sample_rate, 3500.0, 0.0),
            shelf_r: HighShelf::new(sample_rate, 3500.0, 0.0),
            delay_buf_l: [0.0; 256],
            delay_buf_r: [0.0; 256],
            delay_idx: 0,
            delay_samples: (0.0018 * sample_rate) as usize,
            reflection_gain: 0.25,
        };
        baffle.set_angle_deg(45.0);
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
        self.delay_samples = ((delay_s * self.sample_rate) as usize).clamp(1, 255);
    }

    pub fn set_position(&mut self, pos: LidPosition) {
        self.set_angle_deg(pos.to_angle_deg());
    }

    #[inline]
    pub fn process(&mut self, in_l: f64, in_r: f64) -> (f64, f64) {
        // 1. High-shelf baffle filtering
        let filt_l = self.shelf_l.process(in_l);
        let filt_r = self.shelf_r.process(in_r);

        // 2. Early lid reflection delay path
        let read_idx = (self.delay_idx + 256 - self.delay_samples) % 256;
        let refl_l = self.delay_buf_l[read_idx];
        let refl_r = self.delay_buf_r[read_idx];

        self.delay_buf_l[self.delay_idx] = in_l;
        self.delay_buf_r[self.delay_idx] = in_r;
        self.delay_idx = (self.delay_idx + 1) % 256;

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
    }
}
