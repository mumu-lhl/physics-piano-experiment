//! Shared insert FX: 3-band EQ + stereo reverb.
//!
//! Designed as a neutral-by-default insert: EQ gains of 0 dB and a reverb mix
//! of 0 pass the signal through unchanged, so existing presets sound exactly
//! as before until the user turns the FX knobs.
//!
//! The reverb is a Freeverb-style network: four lowpass-feedback comb filters
//! per channel in parallel, followed by two series allpass filters per
//! channel. Delay lengths are the classic Freeverb tunings scaled from
//! 44.1 kHz to the current sample rate.

use crate::Biquad;

/// Fixed EQ band frequencies (Hz) and mid-band Q.
pub const EQ_LOW_FREQ: f64 = 120.0;
pub const EQ_MID_FREQ: f64 = 1000.0;
pub const EQ_MID_Q: f64 = 1.0;
pub const EQ_HIGH_FREQ: f64 = 8000.0;

/// Comb tunings in samples at 44.1 kHz (Freeverb), left then right channel.
const COMB_TUNINGS_L: [usize; 4] = [1116, 1188, 1277, 1356];
const COMB_TUNINGS_R: [usize; 4] = [1429, 1491, 1557, 1617];
/// Allpass tunings in samples at 44.1 kHz (Freeverb), left then right channel.
const ALLPASS_TUNINGS_L: [usize; 2] = [556, 441];
const ALLPASS_TUNINGS_R: [usize; 2] = [341, 225];
/// Allpass feedback (fixed, as in Freeverb).
const ALLPASS_FEEDBACK: f64 = 0.5;
/// Comb feedback range mapped from the decay knob (0..1).
const COMB_FEEDBACK_MIN: f64 = 0.70;
const COMB_FEEDBACK_MAX: f64 = 0.98;
/// Lowpass damping in the comb feedback path (fixed).
const COMB_DAMP: f64 = 0.5;
/// Wet gain after the comb sum, keeping the reverb level comparable to dry.
const WET_GAIN: f64 = 0.5;

/// One-pole-damped feedback comb filter.
struct CombFilter {
    buffer: Vec<f64>,
    index: usize,
    feedback: f64,
    filter_state: f64,
}

impl CombFilter {
    fn new(length_samples: usize, feedback: f64) -> Self {
        Self {
            buffer: vec![0.0; length_samples.max(1)],
            index: 0,
            feedback,
            filter_state: 0.0,
        }
    }

    fn set_feedback(&mut self, feedback: f64) {
        self.feedback = feedback;
    }

    fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.index = 0;
        self.filter_state = 0.0;
    }

    #[inline]
    fn process(&mut self, input: f64) -> f64 {
        let out = self.buffer[self.index];
        self.filter_state = out * (1.0 - COMB_DAMP) + self.filter_state * COMB_DAMP;
        self.buffer[self.index] = input + self.feedback * self.filter_state;
        self.index += 1;
        if self.index >= self.buffer.len() {
            self.index = 0;
        }
        out
    }
}

/// Series allpass filter.
struct AllpassFilter {
    buffer: Vec<f64>,
    index: usize,
}

impl AllpassFilter {
    fn new(length_samples: usize) -> Self {
        Self {
            buffer: vec![0.0; length_samples.max(1)],
            index: 0,
        }
    }

    fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.index = 0;
    }

    #[inline]
    fn process(&mut self, input: f64) -> f64 {
        let delayed = self.buffer[self.index];
        let out = -ALLPASS_FEEDBACK * input + delayed;
        self.buffer[self.index] = input + ALLPASS_FEEDBACK * out;
        self.index += 1;
        if self.index >= self.buffer.len() {
            self.index = 0;
        }
        out
    }
}

/// Stereo Freeverb-style reverb producing only the wet signal.
pub struct Reverb {
    combs_l: Vec<CombFilter>,
    combs_r: Vec<CombFilter>,
    allpasses_l: Vec<AllpassFilter>,
    allpasses_r: Vec<AllpassFilter>,
    sample_rate: f64,
    feedback: f64,
}

fn scale_tuning(tuning_44100: usize, sample_rate: f64) -> usize {
    ((tuning_44100 as f64 * sample_rate / 44100.0).round() as usize).max(1)
}

impl Reverb {
    pub fn new(sample_rate: f64, feedback: f64) -> Self {
        let mut reverb = Self {
            combs_l: Vec::new(),
            combs_r: Vec::new(),
            allpasses_l: Vec::new(),
            allpasses_r: Vec::new(),
            sample_rate,
            feedback,
        };
        reverb.rebuild();
        reverb
    }

    fn rebuild(&mut self) {
        self.combs_l = COMB_TUNINGS_L
            .iter()
            .map(|t| CombFilter::new(scale_tuning(*t, self.sample_rate), self.feedback))
            .collect();
        self.combs_r = COMB_TUNINGS_R
            .iter()
            .map(|t| CombFilter::new(scale_tuning(*t, self.sample_rate), self.feedback))
            .collect();
        self.allpasses_l = ALLPASS_TUNINGS_L
            .iter()
            .map(|t| AllpassFilter::new(scale_tuning(*t, self.sample_rate)))
            .collect();
        self.allpasses_r = ALLPASS_TUNINGS_R
            .iter()
            .map(|t| AllpassFilter::new(scale_tuning(*t, self.sample_rate)))
            .collect();
    }

    pub fn set_sample_rate(&mut self, sample_rate: f64) {
        if (sample_rate - self.sample_rate).abs() > 1.0 {
            self.sample_rate = sample_rate;
            self.rebuild();
        }
    }

    pub fn set_feedback(&mut self, feedback: f64) {
        self.feedback = feedback;
        for comb in self.combs_l.iter_mut().chain(self.combs_r.iter_mut()) {
            comb.set_feedback(feedback);
        }
    }

    pub fn reset(&mut self) {
        for comb in self.combs_l.iter_mut().chain(self.combs_r.iter_mut()) {
            comb.reset();
        }
        for ap in self.allpasses_l.iter_mut().chain(self.allpasses_r.iter_mut()) {
            ap.reset();
        }
    }

    /// Process one stereo frame, returning the wet-only signal.
    pub fn process(&mut self, left: f64, right: f64) -> (f64, f64) {
        let mut wet_l = 0.0;
        for comb in self.combs_l.iter_mut() {
            wet_l += comb.process(left);
        }
        let mut wet_r = 0.0;
        for comb in self.combs_r.iter_mut() {
            wet_r += comb.process(right);
        }
        // Average the four combs, then run the allpass chain.
        wet_l *= 0.25;
        wet_r *= 0.25;
        for ap in self.allpasses_l.iter_mut() {
            wet_l = ap.process(wet_l);
        }
        for ap in self.allpasses_r.iter_mut() {
            wet_r = ap.process(wet_r);
        }
        (wet_l * WET_GAIN, wet_r * WET_GAIN)
    }
}

/// Full insert chain: 3-band EQ into the reverb, with a dry/wet mix.
/// A mix of 0 skips the reverb entirely, so the output is the EQ'd input
/// (and with 0 dB EQ gains, transparent up to floating-point rounding).
pub struct FxChain {
    sample_rate: f64,
    eq_l: [Biquad; 3],
    eq_r: [Biquad; 3],
    reverb: Reverb,
    mix: f64,
}

impl FxChain {
    pub fn new(sample_rate: f64) -> Self {
        let flat = || {
            [
                Biquad::low_shelf(sample_rate, EQ_LOW_FREQ, 0.0),
                Biquad::peaking(sample_rate, EQ_MID_FREQ, EQ_MID_Q, 0.0),
                Biquad::high_shelf(sample_rate, EQ_HIGH_FREQ, 0.0),
            ]
        };
        Self {
            sample_rate,
            eq_l: flat(),
            eq_r: flat(),
            reverb: Reverb::new(sample_rate, COMB_FEEDBACK_MIN),
            mix: 0.0,
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f64) {
        if (sample_rate - self.sample_rate).abs() > 1.0 {
            self.sample_rate = sample_rate;
            self.reverb.set_sample_rate(sample_rate);
        }
    }

    /// Update EQ gains (dB) and reverb mix (0..1) / decay (0..1) from params.
    /// Coefficient updates happen once per block; the host calls this once
    /// per block before `process_block`.
    pub fn set_params(
        &mut self,
        eq_low_db: f64,
        eq_mid_db: f64,
        eq_high_db: f64,
        reverb_mix: f64,
        reverb_decay: f64,
    ) {
        let sr = self.sample_rate;
        for channel in [&mut self.eq_l, &mut self.eq_r] {
            channel[0].set_low_shelf(sr, EQ_LOW_FREQ, eq_low_db);
            channel[1].set_peaking(sr, EQ_MID_FREQ, EQ_MID_Q, eq_mid_db);
            channel[2].set_high_shelf(sr, EQ_HIGH_FREQ, eq_high_db);
        }
        self.mix = reverb_mix.clamp(0.0, 1.0);
        let feedback =
            COMB_FEEDBACK_MIN + (COMB_FEEDBACK_MAX - COMB_FEEDBACK_MIN) * reverb_decay.clamp(0.0, 1.0);
        self.reverb.set_feedback(feedback);
    }

    pub fn reset(&mut self) {
        for biquad in self.eq_l.iter_mut().chain(self.eq_r.iter_mut()) {
            biquad.reset();
        }
        self.reverb.reset();
    }

    /// Process one stereo frame: EQ, then reverb crossfaded by mix.
    /// Per-sample entry point for engines that render sample by sample;
    /// block rendering below is defined in terms of it.
    pub fn process_sample(&mut self, left: f64, right: f64) -> (f64, f64) {
        let mut l = left;
        let mut r = right;
        for band in 0..3 {
            l = self.eq_l[band].process(l);
            r = self.eq_r[band].process(r);
        }
        if self.mix > 0.0 {
            let (wet_l, wet_r) = self.reverb.process(l, r);
            l = l * (1.0 - self.mix) + wet_l * self.mix;
            r = r * (1.0 - self.mix) + wet_r * self.mix;
        }
        (l, r)
    }

    /// Process a stereo block in place: EQ, then reverb crossfaded by mix.
    pub fn process_block(&mut self, left: &mut [f64], right: &mut [f64]) {
        debug_assert_eq!(left.len(), right.len());
        for i in 0..left.len() {
            let (l, r) = self.process_sample(left[i], right[i]);
            left[i] = l;
            right[i] = r;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f64, sample_rate: f64, len: usize) -> Vec<f64> {
        (0..len)
            .map(|i| (2.0 * std::f64::consts::PI * freq * i as f64 / sample_rate).sin())
            .collect()
    }

    fn rms(signal: &[f64]) -> f64 {
        (signal.iter().map(|x| x * x).sum::<f64>() / signal.len().max(1) as f64).sqrt()
    }

    #[test]
    fn fx_chain_neutral_is_transparent() {
        let sr = 48000.0;
        let mut fx = FxChain::new(sr);
        fx.set_params(0.0, 0.0, 0.0, 0.0, 0.5);
        let input: Vec<f64> = (0..4096)
            .map(|i| ((i as f64 * 0.37).sin() * 0.8 + (i as f64 * 0.11).cos() * 0.4))
            .collect();
        let mut left = input.clone();
        let mut right = input.clone();
        fx.process_block(&mut left, &mut right);
        // 0 dB bands are identity filters up to intermediate rounding.
        let max_diff = left
            .iter()
            .zip(input.iter())
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f64, f64::max);
        assert!(max_diff < 1e-12, "neutral FX should be transparent, got {max_diff}");
        assert_eq!(right, left);
    }

    #[test]
    fn eq_low_shelf_boosts_lows() {
        let sr = 48000.0;
        let mut fx = FxChain::new(sr);
        fx.set_params(12.0, 0.0, 0.0, 0.0, 0.5);
        // 50 Hz sits deep in the shelf plateau (corner 120 Hz), so the full
        // +12 dB should apply; 100 Hz is still on the transition slope.
        let input = sine(50.0, sr, sr as usize);
        let mut left = input.clone();
        let mut right = input.clone();
        fx.process_block(&mut left, &mut right);
        // Skip the filter settling transient, then compare steady-state level.
        let ratio = rms(&left[sr as usize / 2..]) / rms(&input[sr as usize / 2..]);
        assert!(ratio > 3.0, "low shelf +12 dB should ~quadruple lows, got {ratio}");
    }

    #[test]
    fn eq_high_shelf_cuts_highs() {
        let sr = 48000.0;
        let mut fx = FxChain::new(sr);
        fx.set_params(0.0, 0.0, -12.0, 0.0, 0.5);
        let input = sine(12000.0, sr, sr as usize);
        let mut left = input.clone();
        let mut right = input.clone();
        fx.process_block(&mut left, &mut right);
        let ratio = rms(&left[sr as usize / 2..]) / rms(&input[sr as usize / 2..]);
        assert!(ratio < 0.3, "high shelf -12 dB should cut highs, got {ratio}");
    }

    #[test]
    fn reverb_tail_decays_and_exists() {
        let sr = 48000.0;
        let mut fx = FxChain::new(sr);
        fx.set_params(0.0, 0.0, 0.0, 1.0, 0.5);
        let len = (sr * 0.5) as usize;
        let mut left = vec![0.0; len];
        let mut right = vec![0.0; len];
        left[0] = 1.0;
        right[0] = 1.0;
        fx.process_block(&mut left, &mut right);
        let early = rms(&left[0..2000]);
        let late = rms(&left[8000..16000]);
        assert!(early > 1e-4, "reverb should produce a wet tail");
        assert!(late < early, "reverb tail should decay, early={early} late={late}");
    }

    #[test]
    fn reverb_higher_decay_rings_longer() {
        let sr = 48000.0;
        let tail_ratio = |decay: f64| -> f64 {
            let mut fx = FxChain::new(sr);
            fx.set_params(0.0, 0.0, 0.0, 1.0, decay);
            let len = (sr * 0.5) as usize;
            let mut left = vec![0.0; len];
            let mut right = vec![0.0; len];
            left[0] = 1.0;
            right[0] = 1.0;
            fx.process_block(&mut left, &mut right);
            rms(&left[12000..24000]) / rms(&left[0..4000]).max(1e-12)
        };
        let short = tail_ratio(0.2);
        let long = tail_ratio(0.9);
        assert!(long > short, "higher decay should ring longer: {short} vs {long}");
    }

    #[test]
    fn reverb_reset_clears_tail() {
        let sr = 48000.0;
        let mut fx = FxChain::new(sr);
        fx.set_params(0.0, 0.0, 0.0, 1.0, 0.8);
        let mut left = vec![0.0; 4096];
        let mut right = vec![0.0; 4096];
        left[0] = 1.0;
        right[0] = 1.0;
        fx.process_block(&mut left, &mut right);
        fx.reset();
        let mut zeros_l = vec![0.0; 4096];
        let mut zeros_r = vec![0.0; 4096];
        fx.process_block(&mut zeros_l, &mut zeros_r);
        assert!(zeros_l.iter().all(|x| *x == 0.0));
        assert!(zeros_r.iter().all(|x| *x == 0.0));
    }
}
