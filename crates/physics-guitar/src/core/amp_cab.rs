//! Built-in Electric Guitar Tube Amp & 12-inch Cabinet Simulator.
//!
//! Provides out-of-the-box warm studio tone:
//! - Multi-stage tube amplification:
//!   - 12AX7 asymmetric triode preamplifier with dynamic cathode bias drift & "tube bloom"
//!   - 3-band interactive tone stack (Bass, Middle, Treble)
//!   - Push-pull power amplifier (6L6/EL34) with dynamic power-supply SAG compression
//! - 12-inch Celestion Vintage 30 cabinet acoustic filtering with thump bump and cone roll-off

use physics_dsp::Biquad;
use crate::core::convolution::{CabinetIrProfile, UpolsConvolutionEngine};

/// Multi-rate oversampling factor for the non-linear tube preamp stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OversamplingFactor {
    X1,
    X2,
    X4,
}

/// 1st-order allpass section for polyphase half-band filters.
#[derive(Debug, Clone, Copy, Default)]
struct PolyphaseAllpass {
    a: f64,
    z1: f64,
    z2: f64,
}

impl PolyphaseAllpass {
    pub fn new(a: f64) -> Self {
        Self { a, z1: 0.0, z2: 0.0 }
    }

    #[inline(always)]
    pub fn process(&mut self, x: f64) -> f64 {
        let y = self.a * x + self.z2;
        self.z2 = self.z1;
        self.z1 = x - self.a * y;
        y
    }

    pub fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }
}

/// High-efficiency polyphase IIR half-band 2x oversampler for zero-allocation anti-aliased saturation.
#[derive(Debug, Clone)]
pub struct HalfbandOversampler2x {
    // 2-branch allpass coefficients yielding > 50 dB stopband attenuation
    up_ap0: PolyphaseAllpass,
    up_ap1: PolyphaseAllpass,
    down_ap0: PolyphaseAllpass,
    down_ap1: PolyphaseAllpass,
}

impl HalfbandOversampler2x {
    pub fn new() -> Self {
        Self {
            up_ap0: PolyphaseAllpass::new(0.1413476743),
            up_ap1: PolyphaseAllpass::new(0.5899948011),
            down_ap0: PolyphaseAllpass::new(0.1413476743),
            down_ap1: PolyphaseAllpass::new(0.5899948011),
        }
    }

    /// Upsamples 1 input sample into 2 oversampled samples.
    #[inline(always)]
    pub fn upsample(&mut self, x: f64) -> (f64, f64) {
        let s0 = self.up_ap0.process(x);
        let s1 = self.up_ap1.process(x);
        (s0, s1)
    }

    /// Decimates 2 oversampled samples into 1 anti-aliased output sample.
    #[inline(always)]
    pub fn downsample(&mut self, y0: f64, y1: f64) -> f64 {
        let s0 = self.down_ap0.process(y0);
        let s1 = self.down_ap1.process(y1);
        0.5 * (s0 + s1)
    }

    pub fn reset(&mut self) {
        self.up_ap0.reset();
        self.up_ap1.reset();
        self.down_ap0.reset();
        self.down_ap1.reset();
    }
}

/// Asymmetric 12AX7 tube preamplifier saturation with dynamic cathode bias drift.
#[inline(always)]
fn preamp_12ax7(x: f64, drive: f64, bias_drift: &mut f64, dt: f64) -> f64 {
    // Dynamic cathode bias drift:
    // Large input signals cause grid conduction, charging the cathode bypass capacitor
    // and dynamically shifting the operating bias point.
    let target_drift = (x.abs() * drive * 0.45).min(0.60);
    let leak_rate = if target_drift > *bias_drift { 45.0 } else { 12.0 };
    *bias_drift += (target_drift - *bias_drift) * (1.0 - (-leak_rate * dt).exp());

    let driven = (x - *bias_drift * 0.35) * (1.0 + drive * 5.0);
    // Asymmetric triode transfer function generating rich 2nd and 3rd harmonics
    if driven >= 0.0 {
        driven.tanh()
    } else {
        // Slight asymmetry on negative half-cycle
        (driven * 1.25).tanh() / 1.25
    }
}

/// Symmetrical push-pull power amp saturation (6L6 / EL34 pair) with dynamic power-supply sag.
#[inline(always)]
fn power_amp_push_pull(x: f64, power_sag: &mut f64, sag_control: f64, dt: f64) -> f64 {
    // Power supply sag: heavy current draw under sustained high output causes rail drop,
    // adding organic touch-sensitive dynamic compression.
    let target_sag = (x * x * sag_control).min(0.65);
    let sag_rate = if target_sag > *power_sag { 35.0 } else { 8.0 };
    *power_sag += (target_sag - *power_sag) * (1.0 - (-sag_rate * dt).exp());

    let headroom = 1.0 / (1.0 + *power_sag * 0.45);
    let scaled = x / headroom;
    scaled.tanh() * headroom
}

/// 12-inch Celestion Vintage 30 Guitar Speaker Cabinet Filter.
/// Guitar speakers are band-limited acoustic transducers:
/// - Sub-bass cutoff below 75 Hz (removes muddiness)
/// - Cabinet mechanical resonance thump bump at ~105 Hz
/// - Midrange presence peak at ~2.8 kHz (classic guitar bite)
/// - Steep cone roll-off above 4.8 kHz (eliminates harsh digital fizz)
/// Available electric guitar speaker cabinet profiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CabinetModel {
    /// Celestion Vintage 30 12" 4x12: punchy mechanical thump (105 Hz), tight 2.8 kHz bite, 4.8 kHz cutoff
    Vintage30,
    /// Fender 65 Twin Reverb 2x12 (JBL D120F): deep 80 Hz bass, scooped warm midrange, open glassy 6.2 kHz air
    TwinReverb,
    /// Marshall 1960A Greenback 4x12: prominent 2.2 kHz woody bark, smooth rolled highs
    Greenback,
}

/// Multi-Profile 12-inch Guitar Speaker Cabinet Filter with microphone placement simulation.
#[derive(Debug, Clone)]
pub struct GuitarCabinet {
    pub model: CabinetModel,
    pub mic_distance_cm: f64,
    highpass: Biquad,
    thump: Biquad,
    presence: Biquad,
    lowpass: Biquad,
    proximity: Biquad,
    sample_rate: f64,
}

impl GuitarCabinet {
    pub fn new(sample_rate: f64) -> Self {
        let mut s = Self {
            model: CabinetModel::Vintage30,
            mic_distance_cm: 2.0, // close-mic placement
            highpass: Biquad::highpass(sample_rate, 75.0, 0.707),
            thump: Biquad::peaking(sample_rate, 105.0, 1.6, 2.0),
            presence: Biquad::peaking(sample_rate, 2800.0, 2.0, 3.8),
            lowpass: Biquad::lowpass(sample_rate, 4800.0, 0.85),
            proximity: Biquad::peaking(sample_rate, 130.0, 0.75, 2.5),
            sample_rate,
        };
        s.update_filters();
        s
    }

    pub fn set_model(&mut self, model: CabinetModel) {
        if self.model != model {
            self.model = model;
            self.update_filters();
        }
    }

    pub fn set_mic_distance(&mut self, distance_cm: f64) {
        let d = distance_cm.clamp(0.0, 30.0);
        if (self.mic_distance_cm - d).abs() > 0.05 {
            self.mic_distance_cm = d;
            self.update_filters();
        }
    }

    fn update_filters(&mut self) {
        let (hp_fc, thump_fc, thump_q, thump_db, pres_fc, pres_q, pres_db, lp_fc) = match self.model {
            CabinetModel::Vintage30 => (75.0, 105.0, 1.6, 2.0, 2800.0, 2.0, 3.8, 4800.0),
            CabinetModel::TwinReverb => (65.0, 88.0, 1.3, 3.2, 3500.0, 1.8, 2.2, 6200.0),
            CabinetModel::Greenback => (82.0, 115.0, 1.8, 1.5, 2200.0, 2.2, 4.5, 4400.0),
        };

        // Proximity effect: close mic (0~5 cm) provides up to +4.5 dB bass boost, decays with distance
        let prox_gain_db = (4.5 * (1.0 - (self.mic_distance_cm / 15.0).min(1.0))).max(0.0);

        self.highpass.set_highpass(self.sample_rate, hp_fc, 0.707);
        self.thump.set_peaking(self.sample_rate, thump_fc, thump_q, thump_db);
        self.presence.set_peaking(self.sample_rate, pres_fc, pres_q, pres_db);
        self.lowpass.set_lowpass(self.sample_rate, lp_fc, 0.85);
        self.proximity.set_peaking(self.sample_rate, 130.0, 0.75, prox_gain_db);
    }

    #[inline(always)]
    pub fn process(&mut self, input: f64) -> f64 {
        let highpassed = self.highpass.process(input);
        let thumped = self.thump.process(highpassed);
        let presence_boosted = self.presence.process(thumped);
        let filtered = self.lowpass.process(presence_boosted);
        self.proximity.process(filtered)
    }
}

/// Integrated Multi-Stage Electric Guitar Tube Amp & Cabinet System.
#[derive(Debug, Clone)]
pub struct GuitarAmpCab {
    pub is_enabled: bool,
    pub cab_enabled: bool,
    pub drive: f64, // 0.0 = clean, 1.0 = crunch/lead
    pub bass: f64,
    pub middle: f64,
    pub treble: f64,
    pub presence: f64,
    pub sag: f64,
    pub cabinet: GuitarCabinet,
    pub oversampling: OversamplingFactor,
    pub ir_engine: UpolsConvolutionEngine,
    pub use_ir_cabinet: bool,
    oversampler1: HalfbandOversampler2x,
    oversampler2: HalfbandOversampler2x,
    bias_drift: f64,
    power_sag: f64,
    bass_filter: Biquad,
    mid_filter: Biquad,
    treble_filter: Biquad,
    sample_rate: f64,
}

impl GuitarAmpCab {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            is_enabled: true,
            cab_enabled: true,
            drive: 0.25, // warm, dynamic clean/edge-of-breakup by default
            bass: 0.5,
            middle: 0.5,
            treble: 0.5,
            presence: 0.5,
            sag: 0.35,
            oversampling: OversamplingFactor::X2,
            ir_engine: UpolsConvolutionEngine::new(),
            use_ir_cabinet: false,
            oversampler1: HalfbandOversampler2x::new(),
            oversampler2: HalfbandOversampler2x::new(),
            bias_drift: 0.0,
            power_sag: 0.0,
            bass_filter: Biquad::peaking(sample_rate, 110.0, 0.75, 0.0),
            mid_filter: Biquad::peaking(sample_rate, 650.0, 0.9, 0.0),
            treble_filter: Biquad::high_shelf(sample_rate, 3200.0, 0.0),
            sample_rate,
            cabinet: GuitarCabinet::new(sample_rate),
        }
    }

    pub fn set_tone_stack(&mut self, bass: f64, middle: f64, treble: f64, presence: f64) {
        self.bass = bass.clamp(0.0, 1.0);
        self.middle = middle.clamp(0.0, 1.0);
        self.treble = treble.clamp(0.0, 1.0);
        self.presence = presence.clamp(0.0, 1.0);
        self.update_tone_filters();
    }

    pub fn set_drive(&mut self, drive: f64) {
        self.drive = drive.clamp(0.0, 1.0);
    }

    pub fn set_oversampling(&mut self, factor: OversamplingFactor) {
        self.oversampling = factor;
        self.oversampler1.reset();
        self.oversampler2.reset();
    }

    pub fn set_use_ir_cabinet(&mut self, use_ir: bool) {
        self.use_ir_cabinet = use_ir;
    }

    pub fn load_ir_profile(&mut self, profile: CabinetIrProfile) {
        self.ir_engine.load_preset(profile);
    }

    pub fn load_custom_ir(&mut self, ir: &[f64]) {
        self.ir_engine.load_ir(ir);
    }

    pub fn set_cabinet_model(&mut self, model: CabinetModel) {
        self.cabinet.set_model(model);
        match model {
            CabinetModel::Vintage30 => self.ir_engine.load_preset(CabinetIrProfile::CelestionVintage30),
            CabinetModel::TwinReverb => self.ir_engine.load_preset(CabinetIrProfile::FenderTwinReverb),
            CabinetModel::Greenback => self.ir_engine.load_preset(CabinetIrProfile::MarshallGreenback),
        }
    }

    pub fn set_mic_distance(&mut self, distance_cm: f64) {
        self.cabinet.set_mic_distance(distance_cm);
    }

    fn update_tone_filters(&mut self) {
        let bass_db = (self.bass - 0.5) * 12.0;
        let mid_db = (self.middle - 0.5) * 10.0;
        let treble_db = (self.treble - 0.5) * 12.0;
        self.bass_filter.set_peaking(self.sample_rate, 110.0, 0.75, bass_db);
        self.mid_filter.set_peaking(self.sample_rate, 650.0, 0.9, mid_db);
        self.treble_filter = Biquad::high_shelf(self.sample_rate, 3200.0, treble_db);
    }

    #[inline(always)]
    pub fn process(&mut self, di_input: f64) -> f64 {
        if !self.is_enabled {
            return di_input;
        }
        let dt = 1.0 / self.sample_rate;

        // 1. Stage 1: 12AX7 Preamp tube with polyphase oversampling & anti-aliasing
        let preamped = match self.oversampling {
            OversamplingFactor::X1 => {
                preamp_12ax7(di_input, self.drive, &mut self.bias_drift, dt)
            }
            OversamplingFactor::X2 => {
                let (x0, x1) = self.oversampler1.upsample(di_input);
                let dt_half = dt * 0.5;
                let y0 = preamp_12ax7(x0, self.drive, &mut self.bias_drift, dt_half);
                let y1 = preamp_12ax7(x1, self.drive, &mut self.bias_drift, dt_half);
                self.oversampler1.downsample(y0, y1)
            }
            OversamplingFactor::X4 => {
                let (x_a, x_b) = self.oversampler1.upsample(di_input);
                let (x00, x01) = self.oversampler2.upsample(x_a);
                let (x10, x11) = self.oversampler2.upsample(x_b);
                let dt_quarter = dt * 0.25;
                let y00 = preamp_12ax7(x00, self.drive, &mut self.bias_drift, dt_quarter);
                let y01 = preamp_12ax7(x01, self.drive, &mut self.bias_drift, dt_quarter);
                let y10 = preamp_12ax7(x10, self.drive, &mut self.bias_drift, dt_quarter);
                let y11 = preamp_12ax7(x11, self.drive, &mut self.bias_drift, dt_quarter);
                let y_a = self.oversampler2.downsample(y00, y01);
                let y_b = self.oversampler2.downsample(y10, y11);
                self.oversampler1.downsample(y_a, y_b)
            }
        };

        // 2. 3-Band Tone Stack (Bass, Middle, Treble)
        let tone_shaped = self
            .treble_filter
            .process(self.mid_filter.process(self.bass_filter.process(preamped)));

        // 3. Stage 2: Push-pull power amplifier (6L6/EL34) with dynamic power sag
        let saturated = power_amp_push_pull(tone_shaped, &mut self.power_sag, self.sag, dt);

        // 4. 12-inch guitar cabinet filtering (analytic physical biquad or UPOLS IR engine)
        if self.cab_enabled {
            if self.use_ir_cabinet {
                self.ir_engine.process_sample(saturated)
            } else {
                self.cabinet.process(saturated)
            }
        } else {
            saturated
        }
    }
}
