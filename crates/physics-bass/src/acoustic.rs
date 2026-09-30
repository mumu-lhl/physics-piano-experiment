//! Bass-specific transducers: a finite magnetic pickup aperture and an acoustic body.

use crate::string::FdtdString;
use physics_dsp::{Biquad, ModalTransition, OverdampedPolicy};
use std::f64::consts::PI;

#[derive(Debug, Clone, Copy)]
struct BodyMode {
    transition: ModalTransition,
    q: f64,
    v: f64,
    frequency: f64,
    gain: f64,
}

impl BodyMode {
    fn new(sample_rate: f64, frequency: f64, t60: f64, gain: f64) -> Self {
        let sigma = (1000.0_f64.ln() / t60.max(0.05)).min(frequency * PI * 0.95);
        Self {
            transition: ModalTransition::new(
                2.0 * PI * frequency,
                sigma,
                1.0 / sample_rate.max(1.0),
                OverdampedPolicy::ExponentialFallback,
            ),
            q: 0.0,
            v: 0.0,
            frequency,
            gain,
        }
    }

    #[inline]
    fn step(&mut self, force: f64) -> f64 {
        let (p11, p12, p21, p22) = self.transition.phi;
        let (g1, g2) = self.transition.gamma;
        let next_q = p11 * self.q + p12 * self.v + g1 * force;
        let next_v = p21 * self.q + p22 * self.v + g2 * force;
        self.q = next_q.clamp(-100.0, 100.0);
        self.v = next_v.clamp(-100.0, 100.0);
        self.q * self.gain
    }
}

/// Four low-frequency/signature modes of a wooden bass body.
#[derive(Debug, Clone)]
pub struct AcousticBassBody {
    modes: [BodyMode; 4],
    pub air_resonance_hz: f64,
    pub bridge_hill_hz: f64,
    last_output: f64,
}

impl AcousticBassBody {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            modes: [
                BodyMode::new(sample_rate, 59.0, 0.75, 0.95),  // A0
                BodyMode::new(sample_rate, 91.0, 0.55, 0.52),  // B1-
                BodyMode::new(sample_rate, 124.0, 0.48, 0.44), // B1+
                BodyMode::new(sample_rate, 560.0, 0.16, 0.20), // bridge hill
            ],
            air_resonance_hz: 59.0,
            bridge_hill_hz: 560.0,
            last_output: 0.0,
        }
    }

    /// Drives the body with the bridge force. The small force normalization is
    /// the acoustic radiation impedance of the idealized body.
    #[inline]
    pub fn process(&mut self, bridge_force: f64) -> f64 {
        let normalized = (bridge_force * 0.00008).clamp(-50.0, 50.0);
        let mut output = 0.0;
        for mode in &mut self.modes {
            output += mode.step(normalized);
        }
        self.last_output = (output * 0.22).clamp(-1.0, 1.0);
        self.last_output
    }

    pub fn bridge_velocity(&self) -> f64 {
        self.modes
            .iter()
            .map(|mode| mode.v * mode.gain)
            .sum::<f64>()
            * 0.0001
    }

    pub fn reset(&mut self, sample_rate: f64) {
        *self = Self::new(sample_rate);
    }

    pub fn mode_frequencies(&self) -> [f64; 4] {
        self.modes.map(|mode| mode.frequency)
    }
}

/// Electric bass pickup model. It integrates velocity over a finite aperture,
/// applies the magnetic-gap nonlinearity, then uses a passive tone low-pass.
#[derive(Debug, Clone)]
pub struct BassPickup {
    /// Pickup center as a fraction of the vibrating string length (0 = nut).
    pub position: f64,
    /// Gaussian aperture width as a fraction of string length.
    pub aperture: f64,
    /// Tone control, 0 = dark and 1 = open.
    pub tone: f64,
    /// Pickup output gain.
    pub gain: f64,
    pub magnetic_gap: f64,
    filter: Biquad,
    sample_rate: f64,
}

impl BassPickup {
    pub fn new(sample_rate: f64) -> Self {
        let mut pickup = Self {
            position: 0.18,
            aperture: 0.055,
            tone: 0.72,
            gain: 0.85,
            magnetic_gap: 0.012,
            filter: Biquad::zero(),
            sample_rate: sample_rate.max(1.0),
        };
        pickup.update_filter();
        pickup
    }

    pub fn set_tone(&mut self, tone: f64) {
        let tone = tone.clamp(0.0, 1.0);
        if (self.tone - tone).abs() > 1e-5 {
            self.tone = tone;
            self.update_filter();
        }
    }

    pub fn set_position(&mut self, position: f64) {
        self.position = position.clamp(0.06, 0.42);
    }

    #[inline]
    pub fn process_string(&mut self, string: &FdtdString) -> f64 {
        let velocity = string.weighted_velocity(self.position, self.aperture);
        // A finite gap makes large plucks asymmetric and produces the mild even
        // harmonic content of a magnetic pickup without a hard clip.
        let displacement = string.weighted_displacement(self.position, self.aperture);
        let gap = (self.magnetic_gap - displacement).max(self.magnetic_gap * 0.35);
        let nonlinear_gain = (self.magnetic_gap / gap).powi(2).clamp(0.25, 4.0);
        let signal = velocity * nonlinear_gain * self.gain * 0.018;
        self.filter.process(signal).clamp(-1.0, 1.0)
    }

    fn update_filter(&mut self) {
        let cutoff = 680.0_f64 * (4_200.0_f64 / 680.0_f64).powf(self.tone);
        let q = 0.85 + 0.8 * self.tone;
        self.filter = Biquad::lowpass(self.sample_rate, cutoff, q);
    }
}

#[cfg(test)]
mod tests {
    use super::{AcousticBassBody, BassPickup};
    use crate::params::BassStringParams;
    use crate::string::{FdtdString, PluckStyle};

    #[test]
    fn body_exposes_documented_signature_modes() {
        let body = AcousticBassBody::new(48_000.0);
        let modes = body.mode_frequencies();
        assert!((modes[0] - 59.0).abs() < 1.0);
        assert!(modes[3] > 400.0);
    }

    #[test]
    fn pickup_aperture_produces_finite_signal() {
        let mut string = FdtdString::new(BassStringParams::electric_four()[0], 48_000.0);
        string.trigger(0.8, PluckStyle::Finger, 0.2);
        let mut pickup = BassPickup::new(48_000.0);
        let mut peak: f64 = 0.0;
        for _ in 0..2048 {
            string.step();
            peak = peak.max(pickup.process_string(&string).abs());
        }
        assert!(peak.is_finite());
        assert!(peak > 0.0);
    }
}
