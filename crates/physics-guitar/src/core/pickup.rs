//! Electric guitar magnetic pickup spatial sampling, RLC passive tone network, and Stratitis non-linearity.
//!
//! Features (docx Chapter 5):
//! - Single-coil Gaussian aperture & Humbucker differential dual-pole comb filtering
//! - Passive guitar RLC tone circuit (250k/500k pot + 0.022uF/0.047uF capacitor)
//! - Stratitis magnetic pull: negative stiffness and 2nd harmonic distortion beta2 * u^2
//! - Multi-pickup parallel blending (Bridge, Middle, Neck, Bridge+Neck, Bridge+Middle)

use std::f64::consts::PI;
use crate::core::guitar_string::GuitarString;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PickupType {
    /// Single-coil pickup (bright, crisp, wide bandwidth, Fender Stratocaster/Telecaster style)
    SingleCoil,
    /// Dual-coil Humbucker (fat, warm, high output, Gibson Les Paul style)
    Humbucker,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PickupSelector {
    /// Near bridge (~0.085 L): biting, tight bass, maximum treble
    Bridge,
    /// Middle position (~0.165 L): balanced scooped mids
    Middle,
    /// Near neck (~0.245 L): warm, singing sustain, full fundamental
    Neck,
    /// Bridge + Neck in parallel: classic dual-pickup open chime (Telecaster middle / Les Paul middle)
    BridgeAndNeck,
    /// Bridge + Middle in parallel: classic Stratocaster position 2 out-of-phase quack
    BridgeAndMiddle,
}

pub use PickupSelector as PickupPosition;

/// Passive RLC electric guitar tone circuit model.
/// Simulates the guitar's internal pickup inductance (L_p), coil resistance (R_p),
/// cable capacitance (C_cable), and the variable tone potentiometer (R_T) + capacitor (C_T).
#[derive(Debug, Clone)]
pub struct PassiveToneCircuit {
    pub tone_knob: f64, // 0.0 = dark/muffled, 1.0 = wide open/bright
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    x1: f64,
    x2: f64,
    y1: f64,
    y2: f64,
    sample_rate: f64,
    pickup_type: PickupType,
}

impl PassiveToneCircuit {
    pub fn new(sample_rate: f64, pickup_type: PickupType) -> Self {
        let mut s = Self {
            tone_knob: 1.0,
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
            sample_rate,
            pickup_type,
        };
        s.update_coefficients();
        s
    }

    pub fn set_tone(&mut self, tone: f64) {
        self.tone_knob = tone.clamp(0.0, 1.0);
        self.update_coefficients();
    }

    pub fn set_pickup_type(&mut self, pt: PickupType) {
        self.pickup_type = pt;
        self.update_coefficients();
    }

    /// Recomputes RLC second-order lowpass biquad coefficients based on Tone knob position.
    /// - When Tone = 1.0: Resonant peak at ~3.5 kHz (Single-Coil) or ~2.6 kHz (Humbucker), Q ~ 2.0 (bright bite)
    /// - When Tone = 0.0: Resonant cutoff drops to ~650 Hz, Q ~ 1.2 (creamy warm jazz tone)
    fn update_coefficients(&mut self) {
        let (base_fc, min_fc, base_q): (f64, f64, f64) = match self.pickup_type {
            PickupType::SingleCoil => (3600.0, 680.0, 2.2),
            PickupType::Humbucker => (2800.0, 520.0, 1.8),
        };

        // Logarithmic frequency sweep characteristic of audio-taper tone potentiometers
        let fc = min_fc * (base_fc / min_fc).powf(self.tone_knob);
        let q = 0.8 + (base_q - 0.8) * self.tone_knob;

        let w0 = 2.0 * PI * (fc / self.sample_rate).clamp(0.001, 0.49);
        let alpha = w0.sin() / (2.0 * q);
        let cos_w0 = w0.cos();

        let a0 = 1.0 + alpha;
        self.b0 = ((1.0 - cos_w0) * 0.5) / a0;
        self.b1 = (1.0 - cos_w0) / a0;
        self.b2 = self.b0;
        self.a1 = (-2.0 * cos_w0) / a0;
        self.a2 = (1.0 - alpha) / a0;
    }

    #[inline(always)]
    pub fn process(&mut self, input: f64) -> f64 {
        let out = self.b0 * input + self.b1 * self.x1 + self.b2 * self.x2
            - self.a1 * self.y1 - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = input;
        self.y2 = self.y1;
        self.y1 = out;
        out
    }
}

/// Advanced Electric Guitar Magnetic Pickup Model:
/// - Finite pole aperture spatial Gaussian integration
/// - Dual-coil humbucker differential comb filtering
/// - Stratitis permanent magnet non-linear pull (second harmonic distortion)
/// - Passive RLC tone network
#[derive(Debug, Clone)]
pub struct MagneticPickup {
    pub pickup_type: PickupType,
    pub selector: PickupSelector,
    /// Pole spread sigma_w (meters)
    pub pole_width: f64,
    /// Dual pole spacing for humbucker d (meters, ~18mm)
    pub humbucker_spacing: f64,
    /// Stratitis magnet pull strength [0.0 = neutral, 1.0 = heavy vintage magnet pull]
    pub stratitis_strength: f64,
    /// Internal RLC tone circuit
    pub tone_circuit: PassiveToneCircuit,
}

impl MagneticPickup {
    pub fn new(pickup_type: PickupType, selector: PickupSelector, sample_rate: f64) -> Self {
        Self {
            pickup_type,
            selector,
            pole_width: 0.0035, // 3.5 mm Gaussian aperture
            humbucker_spacing: 0.019, // 19 mm humbucker pole gap
            stratitis_strength: 0.35, // subtle authentic magnet pull
            tone_circuit: PassiveToneCircuit::new(sample_rate, pickup_type),
        }
    }

    pub fn set_tone(&mut self, tone: f64) {
        self.tone_circuit.set_tone(tone);
    }

    pub fn set_pickup_type(&mut self, pt: PickupType) {
        self.pickup_type = pt;
        self.tone_circuit.set_pickup_type(pt);
    }

    /// Evaluates spatial EMF at a specific position along the string.
    fn sample_at_pos(&self, string: &GuitarString, pos_ratio: f64) -> f64 {
        let length = string.effective_length;
        let x_pu = pos_ratio * string.params.scale_length;

        if x_pu > length {
            return 0.0;
        }

        let mut signal = 0.0;
        let sigma_w = self.pole_width;
        let d = self.humbucker_spacing;

        for m in 0..string.num_modes {
            let m_f = (m + 1) as f64;
            let k_m = m_f * PI / length;

            let gauss_decay = (-0.5 * (k_m * sigma_w).powi(2)).exp();
            let spatial_factor = (k_m * x_pu).sin() * gauss_decay;

            let sensitivity = match self.pickup_type {
                PickupType::SingleCoil => spatial_factor,
                PickupType::Humbucker => {
                    let comb = (k_m * d * 0.5).cos();
                    spatial_factor * comb
                }
            };

            // Capture primary vertical modal velocity and secondary horizontal (~12%)
            let v_eff = string.state_t[m].v + 0.12 * string.state_p[m].v;
            signal += v_eff * sensitivity;

            // Stratitis non-linear magnetic pull: second harmonic generation beta2 * u^2 (docx Chapter 5)
            if self.stratitis_strength > 0.0 {
                let u_vert = string.state_t[m].q;
                let distortion_2nd = self.stratitis_strength * 120.0 * u_vert.powi(2) * sensitivity;
                signal += distortion_2nd;
            }
        }

        signal
    }

    /// Evaluates the total induced EMF from a string according to current selector setting.
    pub fn sample_string(&mut self, string: &GuitarString) -> f64 {
        let raw_emf = match self.selector {
            PickupSelector::Bridge => self.sample_at_pos(string, 0.085),
            PickupSelector::Middle => self.sample_at_pos(string, 0.165),
            PickupSelector::Neck => self.sample_at_pos(string, 0.245),
            PickupSelector::BridgeAndNeck => {
                let b = self.sample_at_pos(string, 0.085);
                let n = self.sample_at_pos(string, 0.245);
                (b + n) * 0.65
            }
            PickupSelector::BridgeAndMiddle => {
                let b = self.sample_at_pos(string, 0.085);
                let m = self.sample_at_pos(string, 0.165);
                (b + m) * 0.65
            }
        };

        // Filter through passive RLC tone network
        self.tone_circuit.process(raw_emf)
    }
}
