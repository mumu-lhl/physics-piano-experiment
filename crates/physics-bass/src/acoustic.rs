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
        self.v * self.gain
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
        let normalized = (bridge_force * 0.08).clamp(-50.0, 50.0);
        let mut output = 0.0;
        for mode in &mut self.modes {
            output += mode.step(normalized);
        }
        // Modal velocity is converted to a calibrated near-field audio level;
        // the downstream master gain and soft limiter provide user headroom.
        self.last_output = (output * 160.0).clamp(-1.0, 1.0);
        self.last_output
    }

    /// Modal bridge-point velocity used as the reciprocal mechanical response.
    pub fn bridge_velocity(&self) -> f64 {
        self.modes
            .iter()
            .map(|mode| mode.v * mode.gain)
            .sum::<f64>()
            * 0.10
    }

    /// Modal bridge-point displacement, bounded to a physically plausible
    /// sub-millimetre motion before it is fed back into the string boundary.
    pub fn bridge_displacement(&self) -> f64 {
        self.modes
            .iter()
            .map(|mode| mode.q * mode.gain)
            .sum::<f64>()
            .mul_add(0.10, 0.0)
            .clamp(-0.001, 0.001)
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
    /// Pickup blend: 0.0 = Neck pickup, 0.5 = 50/50 scooped J-bass, 1.0 = Bridge pickup.
    pub pickup_blend: f64,
    /// Tone control, 0 = dark and 1 = open.
    pub tone: f64,
    /// Pickup output gain.
    pub gain: f64,
    pub magnetic_gap: f64,
    /// Physical pickup/cable equivalent parameters for the passive RLC pole.
    pub inductance_h: f64,
    pub capacitance_f: f64,
    pub series_resistance: f64,
    pub load_resistance: f64,
    filter: Biquad,
    sample_rate: f64,
}

impl BassPickup {
    pub fn new(sample_rate: f64) -> Self {
        let mut pickup = Self {
            position: 0.18,
            aperture: 0.055,
            pickup_blend: 0.0,
            tone: 0.72,
            gain: 0.85,
            magnetic_gap: 0.012,
            inductance_h: 2.4,
            capacitance_f: 420.0e-12,
            series_resistance: 8_200.0,
            load_resistance: 470_000.0,
            filter: Biquad::zero(),
            sample_rate: sample_rate.max(1.0),
        };
        pickup.update_filter();
        pickup
    }

    pub fn set_tone(&mut self, tone: f64) {
        let tone = tone.clamp(0.0, 1.0);
        if (self.tone - tone).abs() > 0.01 {
            self.tone = tone;
            self.update_filter();
        }
    }

    pub fn set_position(&mut self, position: f64) {
        self.position = position.clamp(0.06, 0.42);
    }

    pub fn set_blend(&mut self, blend: f64) {
        self.pickup_blend = blend.clamp(0.0, 1.0);
    }

    #[inline]
    pub fn process_string(&mut self, string: &FdtdString) -> f64 {
        let (velocity, displacement) = if self.pickup_blend <= 0.001 {
            (
                string.weighted_velocity(self.position, self.aperture),
                string.weighted_displacement(self.position, self.aperture),
            )
        } else {
            // Dual J-Bass pickup geometry: Neck pickup (warmer, deeper) and Bridge pickup (bite)
            let neck_pos = (self.position + 0.06).clamp(0.12, 0.42);
            let bridge_pos = (self.position - 0.06).clamp(0.06, 0.25);
            let w_neck = 1.0 - self.pickup_blend;
            let w_bridge = self.pickup_blend;

            let v_neck = string.weighted_velocity(neck_pos, self.aperture);
            let v_bridge = string.weighted_velocity(bridge_pos, self.aperture);
            let d_neck = string.weighted_displacement(neck_pos, self.aperture);
            let d_bridge = string.weighted_displacement(bridge_pos, self.aperture);

            (
                w_neck * v_neck + w_bridge * v_bridge,
                w_neck * d_neck + w_bridge * d_bridge,
            )
        };

        // A finite gap makes large plucks asymmetric and produces the mild even
        // harmonic content of a magnetic pickup without a hard clip.
        let gap = (self.magnetic_gap - displacement).max(self.magnetic_gap * 0.35);
        let nonlinear_gain = (self.magnetic_gap / gap).powi(2).clamp(0.25, 4.0);
        let signal = velocity * nonlinear_gain * self.gain * 0.018;
        self.filter.process(signal).clamp(-1.0, 1.0)
    }

    /// Rebuilds the bilinear-transform equivalent of the pickup's passive RLC
    /// network. Tone changes the shunt capacitance; the filter remains a true
    /// second-order resonant circuit rather than a generic low-pass shortcut.
    fn update_filter(&mut self) {
        let tone_capacitance = 1.9e-9 * (1.0 - self.tone).powf(0.85);
        let capacitance = (self.capacitance_f + tone_capacitance).max(20.0e-12);
        let inductance = self.inductance_h.max(1e-6);
        let load = self.load_resistance.max(1.0);
        let numerator_gain = 1.0 / (inductance * capacitance);
        let a0 = numerator_gain * (1.0 + self.series_resistance / load);
        let a1 = self.series_resistance / inductance + 1.0 / (load * capacitance);
        let k = 2.0 * self.sample_rate;
        let d0 = k * k + a1 * k + a0;
        let d1 = -2.0 * k * k + 2.0 * a0;
        let d2 = k * k - a1 * k + a0;
        self.filter = Biquad::from_coefficients(
            numerator_gain / d0,
            2.0 * numerator_gain / d0,
            numerator_gain / d0,
            d1 / d0,
            d2 / d0,
        );
    }

    pub fn reset(&mut self) {
        self.filter.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::{AcousticBassBody, BassPickup};
    use crate::params::BassStringParams;
    use crate::string::{FdtdString, PluckStyle};
    use std::f64::consts::PI;

    #[test]
    fn body_exposes_documented_signature_modes() {
        let body = AcousticBassBody::new(48_000.0);
        let modes = body.mode_frequencies();
        assert!((modes[0] - 59.0).abs() < 1.0);
        assert!(modes[3] > 400.0);
    }

    #[test]
    fn passive_pickup_tone_moves_the_resonance_as_capacitance_changes() {
        fn measured_peak_frequency(tone: f64) -> f64 {
            let mut pickup = BassPickup::new(48_000.0);
            pickup.set_tone(tone);
            let mut best_frequency = 0.0;
            let mut best_level = 0.0;
            for frequency in (500..=7_000).step_by(250) {
                pickup.filter.reset();
                let mut energy = 0.0;
                for sample in 0..8_192 {
                    let input = (2.0 * PI * frequency as f64 * sample as f64 / 48_000.0).sin();
                    let output = pickup.filter.process(input);
                    if sample >= 4_096 {
                        energy += output * output;
                    }
                }
                let level = energy.sqrt();
                if level > best_level {
                    best_level = level;
                    best_frequency = frequency as f64;
                }
            }
            best_frequency
        }

        let dark_peak = measured_peak_frequency(0.0);
        let open_peak = measured_peak_frequency(1.0);
        assert!(
            open_peak > dark_peak + 1_000.0,
            "{dark_peak} -> {open_peak}"
        );
    }

    #[test]
    fn acoustic_body_radiates_and_returns_a_finite_bridge_response() {
        let mut body = AcousticBassBody::new(48_000.0);
        let mut peak = 0.0_f64;
        for sample in 0..48_000 {
            let force = if sample == 0 { 10.0 } else { 0.0 };
            peak = peak.max(body.process(force).abs());
        }
        assert!(peak.is_finite() && peak > 1e-9);
        assert!(body.bridge_displacement().is_finite());
        assert!(body.bridge_velocity().is_finite());
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
