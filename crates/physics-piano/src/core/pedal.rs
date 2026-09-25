//! Pedal Mechanism Physics & Damper Articulations (Tier 6).
//!
//! Models:
//! - 88-damper bulk lift "whoosh" noise upon sustain pedal depression
//! - Cast-iron plate / frame shock impulse upon rapid pedal stomp
//! - Restrike damper felt friction buzzing on vibrating strings

use crate::core::action::Biquad;

/// Lightweight, deterministic XorShift PRNG for zero-allocation real-time noise.
#[derive(Debug, Clone)]
struct FastNoise {
    state: u32,
}

impl FastNoise {
    fn new(seed: u32) -> Self {
        Self { state: seed.max(1) }
    }

    #[inline(always)]
    fn next_f64(&mut self) -> f64 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 17;
        self.state ^= self.state << 5;
        // Map to [-1.0, 1.0]
        (self.state as f64) / 2147483648.0 - 1.0
    }
}

/// Damper Bulk Lift Whoosh Synthesizer.
///
/// When the sustain pedal lifts 88 felt dampers off the strings simultaneously,
/// a gentle broadband acoustic whoosh is radiated through the rim and soundboard.
pub struct DamperWhoosh {
    _sample_rate: f64,
    noise: FastNoise,
    filter_l: Biquad,
    filter_r: Biquad,
    envelope: f64,
    decay_rate: f64,
    pub gain: f64,
}

impl DamperWhoosh {
    pub fn new(sample_rate: f64) -> Self {
        let f_l = Biquad::bandpass(sample_rate, 1100.0, 1.2);
        let f_r = Biquad::bandpass(sample_rate, 1350.0, 1.2);
        // ~70ms decay
        let decay_rate = (-1.0 / (0.070 * sample_rate)).exp();

        Self {
            _sample_rate: sample_rate,
            noise: FastNoise::new(0x1337BEEF),
            filter_l: f_l,
            filter_r: f_r,
            envelope: 0.0,
            decay_rate,
            gain: 1.0,
        }
    }

    /// Trigger whoosh when sustain pedal lifts dampers, scaled by pedal speed.
    pub fn trigger(&mut self, pedal_speed: f64) {
        let speed = pedal_speed.clamp(0.0, 5.0);
        let amp = (speed / 1.5).clamp(0.05, 1.0).powf(1.2) * 0.025;
        self.envelope = self.envelope.max(amp);
    }

    #[inline]
    pub fn step(&mut self) -> (f64, f64) {
        if self.envelope <= 1e-6 || self.gain <= 1e-5 {
            self.envelope = 0.0;
            return (0.0, 0.0);
        }

        let n = self.noise.next_f64() * self.envelope;
        self.envelope *= self.decay_rate;

        let out_l = self.filter_l.process(n) * self.gain;
        let out_r = self.filter_r.process(n) * self.gain;

        (out_l, out_r)
    }

    pub fn reset(&mut self) {
        self.envelope = 0.0;
        self.filter_l.reset();
        self.filter_r.reset();
    }
}

/// Cast-Iron Plate Structural Shock Impulse.
///
/// Rapid pedal stomping transmits reaction force via the lyre trapwork levers
/// directly into the massive cast-iron frame plate, exciting low-frequency modes.
pub struct PlateShock {
    _sample_rate: f64,
    modes_l: [Biquad; 3],
    modes_r: [Biquad; 3],
    impulse_l: f64,
    impulse_r: f64,
    pub gain: f64,
}

impl PlateShock {
    pub fn new(sample_rate: f64) -> Self {
        // Cast-iron frame modes: 58 Hz, 165 Hz, 340 Hz
        let modes_l = [
            Biquad::resonator(sample_rate, 58.0, 10.0),
            Biquad::resonator(sample_rate, 165.0, 18.0),
            Biquad::resonator(sample_rate, 340.0, 25.0),
        ];
        let modes_r = [
            Biquad::resonator(sample_rate, 58.0, 10.0),
            Biquad::resonator(sample_rate, 165.0, 18.0),
            Biquad::resonator(sample_rate, 340.0, 25.0),
        ];

        Self {
            _sample_rate: sample_rate,
            modes_l,
            modes_r,
            impulse_l: 0.0,
            impulse_r: 0.0,
            gain: 1.0,
        }
    }

    /// Trigger pedal trapwork shock upon fast pedal strike or release.
    pub fn trigger(&mut self, pedal_speed: f64) {
        if pedal_speed.abs() > 0.45 {
            let amp = (pedal_speed.abs() - 0.45).clamp(0.0, 3.0).powf(1.3) * 0.035;
            // Frame shock is slightly center-weighted with subtle stereo spread
            self.impulse_l += amp * 0.72;
            self.impulse_r += amp * 0.68;
        }
    }

    #[inline]
    pub fn step(&mut self) -> (f64, f64) {
        if self.gain <= 1e-5 {
            self.impulse_l = 0.0;
            self.impulse_r = 0.0;
            return (0.0, 0.0);
        }

        let inp_l = self.impulse_l;
        let inp_r = self.impulse_r;
        self.impulse_l = 0.0;
        self.impulse_r = 0.0;

        let out_l = (self.modes_l[0].process(inp_l) * 0.6
            + self.modes_l[1].process(inp_l) * 0.3
            + self.modes_l[2].process(inp_l) * 0.1)
            * self.gain;

        let out_r = (self.modes_r[0].process(inp_r) * 0.6
            + self.modes_r[1].process(inp_r) * 0.3
            + self.modes_r[2].process(inp_r) * 0.1)
            * self.gain;

        (out_l, out_r)
    }

    pub fn reset(&mut self) {
        self.impulse_l = 0.0;
        self.impulse_r = 0.0;
        for m in &mut self.modes_l {
            m.reset();
        }
        for m in &mut self.modes_r {
            m.reset();
        }
    }
}

/// Damper Restrike Friction Buzzing.
///
/// When a key is released while a string has significant vibration energy,
/// the descending felt lightly touches the string before halting it,
/// creating a brief friction buzzing/scraping transient (~25ms).
pub struct RestrikeBuzz {
    _sample_rate: f64,
    noise: FastNoise,
    buzz_filters: [Biquad; 2],
    envelope: f64,
    decay_rate: f64,
    pan_l: f64,
    pan_r: f64,
    pub gain: f64,
}

impl RestrikeBuzz {
    pub fn new(sample_rate: f64) -> Self {
        let f1 = Biquad::bandpass(sample_rate, 2200.0, 3.5);
        let f2 = Biquad::bandpass(sample_rate, 4500.0, 4.0);
        let decay_rate = (-1.0 / (0.030 * sample_rate)).exp(); // 30ms decay

        Self {
            _sample_rate: sample_rate,
            noise: FastNoise::new(0xCAFEBABE),
            buzz_filters: [f1, f2],
            envelope: 0.0,
            decay_rate,
            pan_l: 0.707,
            pan_r: 0.707,
            gain: 1.0,
        }
    }

    /// Trigger restrike buzz if string energy at release exceeds threshold.
    pub fn trigger(&mut self, key: u8, string_energy: f64) {
        if string_energy > 0.005 {
            let amp = (string_energy * 20.0).clamp(0.01, 1.0).sqrt() * 0.02;
            self.envelope = self.envelope.max(amp);

            let pan = ((key.clamp(21, 108) - 21) as f64 / 87.0).clamp(0.0, 1.0);
            self.pan_l = (1.0 - pan).sqrt();
            self.pan_r = pan.sqrt();
        }
    }

    #[inline]
    pub fn step(&mut self) -> (f64, f64) {
        if self.envelope <= 1e-6 || self.gain <= 1e-5 {
            self.envelope = 0.0;
            return (0.0, 0.0);
        }

        let n = self.noise.next_f64() * self.envelope;
        self.envelope *= self.decay_rate;

        let b = self.buzz_filters[0].process(n) * 0.6 + self.buzz_filters[1].process(n) * 0.4;
        let out_l = b * self.pan_l * self.gain;
        let out_r = b * self.pan_r * self.gain;

        (out_l, out_r)
    }

    pub fn reset(&mut self) {
        self.envelope = 0.0;
        self.buzz_filters[0].reset();
        self.buzz_filters[1].reset();
    }
}
