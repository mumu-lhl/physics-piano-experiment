//! Acoustic guitar Christensen body and soundhole air cavity resonator.

use std::f64::consts::PI;

/// Second-order biquad bandpass resonator for modal body peaks.
#[derive(Debug, Clone)]
pub struct BodyResonator {
    b0: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    x1: f64,
    x2: f64,
    y1: f64,
    y2: f64,
}

impl BodyResonator {
    pub fn new_bandpass(freq: f64, q: f64, sample_rate: f64) -> Self {
        let w0 = 2.0 * PI * freq / sample_rate;
        let alpha = w0.sin() / (2.0 * q);
        let cos_w0 = w0.cos();

        let a0 = 1.0 + alpha;
        let b0 = alpha / a0;
        let b2 = -alpha / a0;
        let a1 = (-2.0 * cos_w0) / a0;
        let a2 = (1.0 - alpha) / a0;

        Self {
            b0,
            b2,
            a1,
            a2,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    #[inline(always)]
    pub fn process(&mut self, input: f64) -> f64 {
        let out = self.b0 * input + self.b2 * self.x2 - self.a1 * self.y1 - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = input;
        self.y2 = self.y1;
        self.y1 = out;
        out
    }
}

/// Christensen 3-DOF coupled acoustic body model:
/// - Helmholtz soundhole air resonance (~100 Hz)
/// - Top plate main wood resonance (~205 Hz)
/// - Back plate resonance (~300 Hz)
/// - Upper body presence resonance (~450 Hz)
#[derive(Debug, Clone)]
pub struct AcousticGuitarBody {
    pub air_resonator: BodyResonator,
    pub top_plate_resonator: BodyResonator,
    pub back_plate_resonator: BodyResonator,
    pub presence_resonator: BodyResonator,
    pub body_mix: f64,
}

impl AcousticGuitarBody {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            // Helmholtz soundhole air cavity: 102 Hz, Q=12
            air_resonator: BodyResonator::new_bandpass(102.0, 12.0, sample_rate),
            // Top spruce plate piston mode: 208 Hz, Q=18
            top_plate_resonator: BodyResonator::new_bandpass(208.0, 18.0, sample_rate),
            // Rosewood back plate mode: 295 Hz, Q=22
            back_plate_resonator: BodyResonator::new_bandpass(295.0, 22.0, sample_rate),
            // Upper bout wood resonance: 460 Hz, Q=16
            presence_resonator: BodyResonator::new_bandpass(460.0, 16.0, sample_rate),
            body_mix: 0.75,
        }
    }

    /// Processes total bridge vertical force through the body acoustic resonator.
    #[inline(always)]
    pub fn process(&mut self, bridge_force: f64) -> f64 {
        let air = self.air_resonator.process(bridge_force) * 1.4;
        let top = self.top_plate_resonator.process(bridge_force) * 1.8;
        let back = self.back_plate_resonator.process(bridge_force) * 0.9;
        let presence = self.presence_resonator.process(bridge_force) * 0.6;

        let resonant_body = air + top + back + presence;
        // Blend direct bridge acoustic excitation with resonant cavity sound
        (1.0 - self.body_mix) * bridge_force + self.body_mix * resonant_body
    }
}
