//! Shared insert FX: 3-band EQ + compressor + stereo delay + stereo reverb.
//!
//! Chain order inside [`FxChain`]: EQ -> compressor -> delay -> reverb.
//! `process_sample` / `process_block` reflect that order.
//!
//! Designed as a neutral-by-default insert: EQ gains of 0 dB, a compressor
//! ratio of 1, a delay mix of 0 and a reverb mix of 0 pass the signal
//! through unchanged, so existing presets sound exactly as before until the
//! user turns the FX knobs.
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
        // Flush subnormal feedback state so a decaying tail goes exactly
        // silent instead of lingering in the denormal range.
        self.filter_state =
            crate::flush_denormal(out * (1.0 - COMB_DAMP) + self.filter_state * COMB_DAMP);
        self.buffer[self.index] = crate::flush_denormal(input + self.feedback * self.filter_state);
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
        self.buffer[self.index] = crate::flush_denormal(input + ALLPASS_FEEDBACK * out);
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

/// Copy a circular delay line's logical contents (oldest sample first,
/// starting at the old write position) into a fresh buffer, returning
/// the write index that follows the copied sequence. Used when a
/// sample-rate change rebuilds the reverb so the sounding tail survives
/// the rebuild instead of being zeroed.
fn copy_delay_contents(old_buffer: &[f64], old_index: usize, new_buffer: &mut [f64]) -> usize {
    if old_buffer.is_empty() || new_buffer.is_empty() {
        return 0;
    }
    let n = old_buffer.len().min(new_buffer.len());
    for i in 0..n {
        new_buffer[i] = old_buffer[(old_index + i) % old_buffer.len()];
    }
    n % new_buffer.len()
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
        // Preserve the overlapping delay-line contents (and each comb's
        // feedback state) across the rebuild so a sounding tail survives
        // a sample-rate change; only the first build has nothing to keep.
        let old_combs_l = std::mem::take(&mut self.combs_l);
        let old_combs_r = std::mem::take(&mut self.combs_r);
        let old_allpasses_l = std::mem::take(&mut self.allpasses_l);
        let old_allpasses_r = std::mem::take(&mut self.allpasses_r);
        self.combs_l = COMB_TUNINGS_L
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let mut comb = CombFilter::new(scale_tuning(*t, self.sample_rate), self.feedback);
                if let Some(old) = old_combs_l.get(i) {
                    comb.index = copy_delay_contents(&old.buffer, old.index, &mut comb.buffer);
                    comb.filter_state = old.filter_state;
                }
                comb
            })
            .collect();
        self.combs_r = COMB_TUNINGS_R
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let mut comb = CombFilter::new(scale_tuning(*t, self.sample_rate), self.feedback);
                if let Some(old) = old_combs_r.get(i) {
                    comb.index = copy_delay_contents(&old.buffer, old.index, &mut comb.buffer);
                    comb.filter_state = old.filter_state;
                }
                comb
            })
            .collect();
        self.allpasses_l = ALLPASS_TUNINGS_L
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let mut ap = AllpassFilter::new(scale_tuning(*t, self.sample_rate));
                if let Some(old) = old_allpasses_l.get(i) {
                    ap.index = copy_delay_contents(&old.buffer, old.index, &mut ap.buffer);
                }
                ap
            })
            .collect();
        self.allpasses_r = ALLPASS_TUNINGS_R
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let mut ap = AllpassFilter::new(scale_tuning(*t, self.sample_rate));
                if let Some(old) = old_allpasses_r.get(i) {
                    ap.index = copy_delay_contents(&old.buffer, old.index, &mut ap.buffer);
                }
                ap
            })
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

/// Maximum delay time (ms); the buffers hold this much audio at the
/// current sample rate and are allocated once at construction.
const DELAY_MAX_MS: f64 = 2000.0;
/// One-pole lowpass coefficient in the delay feedback path (fixed, same
/// value as [`COMB_DAMP`]).
const DELAY_DAMP: f64 = 0.5;

/// Stereo delay with one shared time and a per-channel circular buffer,
/// producing only the wet (delayed) signal. The [`FxChain`] crossfades
/// dry/wet by the delay mix, mirroring the reverb.
///
/// Changing the delay time moves the read tap only; the buffer contents
/// are kept. The feedback path writes `input + feedback * lowpassed
/// delayed sample`, so repeats darken as they decay, like the comb
/// filters in the reverb.
pub struct Delay {
    buffer_l: Vec<f64>,
    buffer_r: Vec<f64>,
    index: usize,
    delay_samples: usize,
    time_ms: f64,
    feedback: f64,
    sample_rate: f64,
    filter_state_l: f64,
    filter_state_r: f64,
}

impl Delay {
    pub fn new(sample_rate: f64) -> Self {
        let mut delay = Self {
            buffer_l: Vec::new(),
            buffer_r: Vec::new(),
            index: 0,
            delay_samples: 1,
            time_ms: DELAY_TIME_DEFAULT_MS,
            feedback: DELAY_FEEDBACK_DEFAULT,
            sample_rate,
            filter_state_l: 0.0,
            filter_state_r: 0.0,
        };
        delay.rebuild();
        delay
    }

    /// Buffer length in samples for the maximum delay time at `sample_rate`.
    fn max_len(sample_rate: f64) -> usize {
        ((DELAY_MAX_MS * sample_rate / 1000.0).ceil() as usize).max(1)
    }

    /// Read-tap position in samples for the stored time, clamped to the
    /// buffer length (a 2000 ms tap at exactly the buffer length reads the
    /// slot about to be overwritten, i.e. the oldest sample).
    fn tap_samples(&self) -> usize {
        ((self.time_ms * self.sample_rate / 1000.0).round() as usize)
            .clamp(1, self.buffer_l.len().max(1))
    }

    fn rebuild(&mut self) {
        let new_len = Self::max_len(self.sample_rate);
        let mut new_l = vec![0.0; new_len];
        let mut new_r = vec![0.0; new_len];
        // Preserve the sounding tail across the rebuild, as `Reverb`
        // does: both channels share one write index, so one copied
        // index serves both.
        let new_index = copy_delay_contents(&self.buffer_l, self.index, &mut new_l);
        copy_delay_contents(&self.buffer_r, self.index, &mut new_r);
        self.buffer_l = new_l;
        self.buffer_r = new_r;
        self.index = new_index;
        self.delay_samples = self.tap_samples();
    }

    pub fn set_sample_rate(&mut self, sample_rate: f64) {
        if (sample_rate - self.sample_rate).abs() > 1.0 {
            self.sample_rate = sample_rate;
            self.rebuild();
        }
    }

    /// Update time (ms) and feedback. Only the read tap moves; the
    /// buffer is never cleared by a time change.
    pub fn set_params(&mut self, time_ms: f64, feedback: f64) {
        self.time_ms = clamp_delay_time(time_ms);
        self.feedback = clamp_delay_feedback(feedback);
        self.delay_samples = self.tap_samples();
    }

    pub fn reset(&mut self) {
        self.buffer_l.fill(0.0);
        self.buffer_r.fill(0.0);
        self.index = 0;
        self.filter_state_l = 0.0;
        self.filter_state_r = 0.0;
    }

    /// Process one stereo frame, returning the wet-only signal.
    pub fn process(&mut self, left: f64, right: f64) -> (f64, f64) {
        let len = self.buffer_l.len();
        let read = (self.index + len - self.delay_samples) % len;
        let delayed_l = self.buffer_l[read];
        let delayed_r = self.buffer_r[read];
        // One-pole-lowpass the delayed sample in the feedback path, as
        // the comb filters do, and flush subnormals on the write.
        self.filter_state_l = crate::flush_denormal(
            delayed_l * (1.0 - DELAY_DAMP) + self.filter_state_l * DELAY_DAMP,
        );
        self.filter_state_r = crate::flush_denormal(
            delayed_r * (1.0 - DELAY_DAMP) + self.filter_state_r * DELAY_DAMP,
        );
        self.buffer_l[self.index] =
            crate::flush_denormal(left + self.feedback * self.filter_state_l);
        self.buffer_r[self.index] =
            crate::flush_denormal(right + self.feedback * self.filter_state_r);
        self.index += 1;
        if self.index >= len {
            self.index = 0;
        }
        (delayed_l, delayed_r)
    }
}

/// Fixed 6 dB soft knee (half-width 3 dB) for the compressor.
const COMP_KNEE_WIDTH_DB: f64 = 6.0;
const COMP_KNEE_HALF_DB: f64 = COMP_KNEE_WIDTH_DB / 2.0;
/// An envelope at or below this linear level counts as silence: the gain
/// computer would take log10 of ~0, so the gain is 1 instead.
const COMP_SILENCE: f64 = 1e-12;

/// Linked stereo compressor: one peak envelope follower on
/// max(|L|, |R|) drives a single gain applied to both channels, so the
/// stereo image never shifts.
///
/// Gain computer with threshold `T`, ratio `r` and knee half-width 3 dB,
/// input level `x` dB of the envelope:
/// * `x <= T - 3` → no reduction (`y = x`);
/// * `x >= T + 3` → `y = T + (x - T) / r`, written as
///   `x + (x - T) * (1/r - 1)` so a ratio of 1 yields `y = x` exactly
///   and the gain is exactly 1.0;
/// * inside the knee → `y = x + (1/r - 1) * (x - T + 3)^2 / (2 * 6)`.
/// The linear gain `10^((y - x) / 20)` is applied to both channels, then
/// the makeup gain `10^(makeup_db / 20)`.
pub struct Compressor {
    sample_rate: f64,
    threshold_db: f64,
    ratio: f64,
    attack_ms: f64,
    release_ms: f64,
    makeup_db: f64,
    envelope: f64,
    attack_coeff: f64,
    release_coeff: f64,
    last_gain: f64,
}

impl Compressor {
    pub fn new(sample_rate: f64) -> Self {
        let mut comp = Self {
            sample_rate,
            threshold_db: COMP_THRESHOLD_DEFAULT_DB,
            ratio: COMP_RATIO_DEFAULT,
            attack_ms: COMP_ATTACK_DEFAULT_MS,
            release_ms: COMP_RELEASE_DEFAULT_MS,
            makeup_db: COMP_MAKEUP_DEFAULT_DB,
            envelope: 0.0,
            attack_coeff: 0.0,
            release_coeff: 0.0,
            last_gain: 1.0,
        };
        comp.retune();
        comp
    }

    /// One-pole envelope coefficient for a time constant in ms:
    /// `alpha = 1 - exp(-1 / (sr * tau_seconds))`.
    fn coeff(sample_rate: f64, tau_ms: f64) -> f64 {
        1.0 - (-1.0 / (sample_rate * tau_ms / 1000.0)).exp()
    }

    /// (Re)compute the attack/release coefficients from the stored time
    /// constants and sample rate. Called from `set_params` and on a
    /// sample-rate change, never per sample.
    fn retune(&mut self) {
        self.attack_coeff = Self::coeff(self.sample_rate, self.attack_ms);
        self.release_coeff = Self::coeff(self.sample_rate, self.release_ms);
    }

    pub fn set_sample_rate(&mut self, sample_rate: f64) {
        if (sample_rate - self.sample_rate).abs() > 1.0 {
            self.sample_rate = sample_rate;
            self.retune();
        }
    }

    pub fn set_params(
        &mut self,
        threshold_db: f64,
        ratio: f64,
        attack_ms: f64,
        release_ms: f64,
        makeup_db: f64,
    ) {
        self.threshold_db = clamp_comp_threshold(threshold_db);
        self.ratio = clamp_comp_ratio(ratio);
        self.attack_ms = clamp_comp_attack(attack_ms);
        self.release_ms = clamp_comp_release(release_ms);
        self.makeup_db = clamp_comp_makeup(makeup_db);
        self.retune();
    }

    pub fn reset(&mut self) {
        self.envelope = 0.0;
        self.last_gain = 1.0;
    }

    /// Linear gain applied to the most recent frame (gain reduction
    /// times makeup). Exactly 1.0 whenever the gain computer yields no
    /// reduction and the makeup is 0 dB. Test-only observability: the
    /// gain is otherwise only visible through the processed audio.
    #[cfg(test)]
    pub fn last_gain(&self) -> f64 {
        self.last_gain
    }

    /// Process one stereo frame, applying the linked gain to both channels.
    pub fn process(&mut self, left: f64, right: f64) -> (f64, f64) {
        let peak = left.abs().max(right.abs());
        let coeff = if peak > self.envelope {
            self.attack_coeff
        } else {
            self.release_coeff
        };
        self.envelope = crate::flush_denormal(self.envelope + coeff * (peak - self.envelope));
        let gain = if self.envelope <= COMP_SILENCE {
            1.0
        } else {
            let x = 20.0 * self.envelope.log10();
            let t = self.threshold_db;
            let inv = 1.0 / self.ratio - 1.0;
            let y = if x <= t - COMP_KNEE_HALF_DB {
                x
            } else if x >= t + COMP_KNEE_HALF_DB {
                x + (x - t) * inv
            } else {
                x + inv * (x - t + COMP_KNEE_HALF_DB).powi(2) / (2.0 * COMP_KNEE_WIDTH_DB)
            };
            10.0f64.powf((y - x) / 20.0)
        };
        let total = gain * 10.0f64.powf(self.makeup_db / 20.0);
        self.last_gain = total;
        (left * total, right * total)
    }
}

/// EQ gains are clamped to this range inside `FxChain::set_params` so a
/// host/automation value outside the param range (or a non-finite value
/// from a corrupt preset) can never drive NaN into the biquads.
const EQ_GAIN_LIMIT_DB: f64 = 24.0;

/// Clamp an EQ gain to ±24 dB, mapping non-finite values to 0 dB.
fn clamp_eq_gain(gain_db: f64) -> f64 {
    if gain_db.is_finite() {
        gain_db.clamp(-EQ_GAIN_LIMIT_DB, EQ_GAIN_LIMIT_DB)
    } else {
        0.0
    }
}

/// Clamp a param to `[lo, hi]`, mapping non-finite values to `default`.
/// Shared by the delay and compressor params below so a corrupt preset
/// can never drive NaN into the FX chain.
fn clamp_or_default(value: f64, lo: f64, hi: f64, default: f64) -> f64 {
    if value.is_finite() {
        value.clamp(lo, hi)
    } else {
        default
    }
}

/// Delay param ranges and neutral defaults (mix defaults to 0, so the
/// delay is off until the user turns it up).
const DELAY_TIME_MIN_MS: f64 = 1.0;
const DELAY_TIME_MAX_MS: f64 = 2000.0;
const DELAY_TIME_DEFAULT_MS: f64 = 250.0;
const DELAY_FEEDBACK_MIN: f64 = 0.0;
const DELAY_FEEDBACK_MAX: f64 = 0.95;
const DELAY_FEEDBACK_DEFAULT: f64 = 0.35;
const DELAY_MIX_MIN: f64 = 0.0;
const DELAY_MIX_MAX: f64 = 1.0;
const DELAY_MIX_DEFAULT: f64 = 0.0;

/// Compressor param ranges and neutral defaults (ratio 1, makeup 0 dB,
/// so the compressor is transparent until the user turns it up).
const COMP_THRESHOLD_MIN_DB: f64 = -60.0;
const COMP_THRESHOLD_MAX_DB: f64 = 0.0;
const COMP_THRESHOLD_DEFAULT_DB: f64 = -18.0;
const COMP_RATIO_MIN: f64 = 1.0;
const COMP_RATIO_MAX: f64 = 20.0;
const COMP_RATIO_DEFAULT: f64 = 1.0;
const COMP_ATTACK_MIN_MS: f64 = 0.1;
const COMP_ATTACK_MAX_MS: f64 = 100.0;
const COMP_ATTACK_DEFAULT_MS: f64 = 10.0;
const COMP_RELEASE_MIN_MS: f64 = 10.0;
const COMP_RELEASE_MAX_MS: f64 = 1000.0;
const COMP_RELEASE_DEFAULT_MS: f64 = 100.0;
const COMP_MAKEUP_MIN_DB: f64 = 0.0;
const COMP_MAKEUP_MAX_DB: f64 = 24.0;
const COMP_MAKEUP_DEFAULT_DB: f64 = 0.0;

fn clamp_delay_time(time_ms: f64) -> f64 {
    clamp_or_default(
        time_ms,
        DELAY_TIME_MIN_MS,
        DELAY_TIME_MAX_MS,
        DELAY_TIME_DEFAULT_MS,
    )
}

fn clamp_delay_feedback(feedback: f64) -> f64 {
    clamp_or_default(
        feedback,
        DELAY_FEEDBACK_MIN,
        DELAY_FEEDBACK_MAX,
        DELAY_FEEDBACK_DEFAULT,
    )
}

fn clamp_delay_mix(mix: f64) -> f64 {
    clamp_or_default(mix, DELAY_MIX_MIN, DELAY_MIX_MAX, DELAY_MIX_DEFAULT)
}

fn clamp_comp_threshold(threshold_db: f64) -> f64 {
    clamp_or_default(
        threshold_db,
        COMP_THRESHOLD_MIN_DB,
        COMP_THRESHOLD_MAX_DB,
        COMP_THRESHOLD_DEFAULT_DB,
    )
}

fn clamp_comp_ratio(ratio: f64) -> f64 {
    clamp_or_default(ratio, COMP_RATIO_MIN, COMP_RATIO_MAX, COMP_RATIO_DEFAULT)
}

fn clamp_comp_attack(attack_ms: f64) -> f64 {
    clamp_or_default(
        attack_ms,
        COMP_ATTACK_MIN_MS,
        COMP_ATTACK_MAX_MS,
        COMP_ATTACK_DEFAULT_MS,
    )
}

fn clamp_comp_release(release_ms: f64) -> f64 {
    clamp_or_default(
        release_ms,
        COMP_RELEASE_MIN_MS,
        COMP_RELEASE_MAX_MS,
        COMP_RELEASE_DEFAULT_MS,
    )
}

fn clamp_comp_makeup(makeup_db: f64) -> f64 {
    clamp_or_default(
        makeup_db,
        COMP_MAKEUP_MIN_DB,
        COMP_MAKEUP_MAX_DB,
        COMP_MAKEUP_DEFAULT_DB,
    )
}

/// Full insert chain: EQ -> compressor -> delay -> reverb.
/// A mix of 0 skips the delay / reverb entirely, so the output is the
/// EQ'd, compressed input (and with 0 dB EQ gains and a compressor ratio
/// of 1, transparent up to floating-point rounding).
pub struct FxChain {
    sample_rate: f64,
    eq_l: [Biquad; 3],
    eq_r: [Biquad; 3],
    eq_low_db: f64,
    eq_mid_db: f64,
    eq_high_db: f64,
    compressor: Compressor,
    delay: Delay,
    delay_mix: f64,
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
            eq_low_db: 0.0,
            eq_mid_db: 0.0,
            eq_high_db: 0.0,
            compressor: Compressor::new(sample_rate),
            delay: Delay::new(sample_rate),
            delay_mix: 0.0,
            reverb: Reverb::new(sample_rate, COMB_FEEDBACK_MIN),
            mix: 0.0,
        }
    }

    /// (Re)build the EQ coefficients for the current sample rate and the
    /// stored gains. The state-preserving `set_*` calls keep the filter
    /// history, so a rate change does not reset the EQ.
    fn apply_eq(&mut self) {
        let sr = self.sample_rate;
        for channel in [&mut self.eq_l, &mut self.eq_r] {
            channel[0].set_low_shelf(sr, EQ_LOW_FREQ, self.eq_low_db);
            channel[1].set_peaking(sr, EQ_MID_FREQ, EQ_MID_Q, self.eq_mid_db);
            channel[2].set_high_shelf(sr, EQ_HIGH_FREQ, self.eq_high_db);
        }
    }

    pub fn set_sample_rate(&mut self, sample_rate: f64) {
        if (sample_rate - self.sample_rate).abs() > 1.0 {
            self.sample_rate = sample_rate;
            self.compressor.set_sample_rate(sample_rate);
            self.delay.set_sample_rate(sample_rate);
            self.reverb.set_sample_rate(sample_rate);
            self.apply_eq();
        }
    }

    /// Update EQ gains (dB), reverb mix (0..1) / decay (0..1), delay
    /// time (1..2000 ms) / feedback (0..0.95) / mix (0..1), and
    /// compressor threshold (-60..0 dB) / ratio (1..20) / attack
    /// (0.1..100 ms) / release (10..1000 ms) / makeup (0..24 dB) from
    /// params. Non-finite values map to the neutral defaults and
    /// out-of-range values clamp to the rails. Coefficient updates
    /// happen once per block; the host calls this once per block before
    /// `process_block`.
    pub fn set_params(
        &mut self,
        eq_low_db: f64,
        eq_mid_db: f64,
        eq_high_db: f64,
        reverb_mix: f64,
        reverb_decay: f64,
        delay_time_ms: f64,
        delay_feedback: f64,
        delay_mix: f64,
        comp_threshold_db: f64,
        comp_ratio: f64,
        comp_attack_ms: f64,
        comp_release_ms: f64,
        comp_makeup_db: f64,
    ) {
        self.eq_low_db = clamp_eq_gain(eq_low_db);
        self.eq_mid_db = clamp_eq_gain(eq_mid_db);
        self.eq_high_db = clamp_eq_gain(eq_high_db);
        self.apply_eq();
        self.mix = reverb_mix.clamp(0.0, 1.0);
        let feedback =
            COMB_FEEDBACK_MIN + (COMB_FEEDBACK_MAX - COMB_FEEDBACK_MIN) * reverb_decay.clamp(0.0, 1.0);
        self.reverb.set_feedback(feedback);
        self.delay.set_params(delay_time_ms, delay_feedback);
        self.delay_mix = clamp_delay_mix(delay_mix);
        self.compressor.set_params(
            comp_threshold_db,
            comp_ratio,
            comp_attack_ms,
            comp_release_ms,
            comp_makeup_db,
        );
    }

    pub fn reset(&mut self) {
        for biquad in self.eq_l.iter_mut().chain(self.eq_r.iter_mut()) {
            biquad.reset();
        }
        self.compressor.reset();
        self.delay.reset();
        self.reverb.reset();
    }

    /// Process one stereo frame: EQ -> compressor -> delay -> reverb,
    /// with the delay and reverb crossfaded by their mixes.
    /// Per-sample entry point for engines that render sample by sample;
    /// block rendering below is defined in terms of it.
    pub fn process_sample(&mut self, left: f64, right: f64) -> (f64, f64) {
        let mut l = left;
        let mut r = right;
        for band in 0..3 {
            l = self.eq_l[band].process(l);
            r = self.eq_r[band].process(r);
        }
        (l, r) = self.compressor.process(l, r);
        if self.delay_mix > 0.0 {
            let (wet_l, wet_r) = self.delay.process(l, r);
            l = l * (1.0 - self.delay_mix) + wet_l * self.delay_mix;
            r = r * (1.0 - self.delay_mix) + wet_r * self.delay_mix;
        }
        if self.mix > 0.0 {
            let (wet_l, wet_r) = self.reverb.process(l, r);
            l = l * (1.0 - self.mix) + wet_l * self.mix;
            r = r * (1.0 - self.mix) + wet_r * self.mix;
        }
        (l, r)
    }

    /// Process a stereo block in place: EQ -> compressor -> delay ->
    /// reverb, with the delay and reverb crossfaded by their mixes.
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
        fx.set_params(
            0.0, 0.0, 0.0, 0.0, 0.5, 250.0, 0.35, 0.0, -18.0, 1.0, 10.0, 100.0, 0.0,
        );
        let input: Vec<f64> = (0..4096)
            .map(|i| (i as f64 * 0.37).sin() * 0.8 + (i as f64 * 0.11).cos() * 0.4)
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
        fx.set_params(
            12.0, 0.0, 0.0, 0.0, 0.5, 250.0, 0.35, 0.0, -18.0, 1.0, 10.0, 100.0, 0.0,
        );
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
        fx.set_params(
            0.0, 0.0, -12.0, 0.0, 0.5, 250.0, 0.35, 0.0, -18.0, 1.0, 10.0, 100.0, 0.0,
        );
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
        fx.set_params(
            0.0, 0.0, 0.0, 1.0, 0.5, 250.0, 0.35, 0.0, -18.0, 1.0, 10.0, 100.0, 0.0,
        );
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
            fx.set_params(
                0.0, 0.0, 0.0, 1.0, decay, 250.0, 0.35, 0.0, -18.0, 1.0, 10.0, 100.0, 0.0,
            );
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
        fx.set_params(
            0.0, 0.0, 0.0, 1.0, 0.8, 250.0, 0.35, 0.0, -18.0, 1.0, 10.0, 100.0, 0.0,
        );
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

    #[test]
    fn eq_gains_clamped_to_24_db_and_nonfinite_maps_to_zero() {
        let sr = 48000.0;
        let input: Vec<f64> = (0..4096)
            .map(|i| (i as f64 * 0.37).sin() * 0.8 + (i as f64 * 0.11).cos() * 0.4)
            .collect();
        let run = |low: f64, mid: f64, high: f64| -> Vec<f64> {
            let mut fx = FxChain::new(sr);
            fx.set_params(
                low, mid, high, 0.0, 0.5, 250.0, 0.35, 0.0, -18.0, 1.0, 10.0, 100.0, 0.0,
            );
            let mut left = input.clone();
            let mut right = input.clone();
            fx.process_block(&mut left, &mut right);
            assert!(left.iter().all(|x| x.is_finite()), "EQ output must stay finite");
            left
        };
        // Out-of-range gains behave exactly like the ±24 dB rails.
        assert_eq!(run(100.0, 0.0, 0.0), run(24.0, 0.0, 0.0));
        assert_eq!(run(-100.0, 0.0, 0.0), run(-24.0, 0.0, 0.0));
        assert_eq!(run(0.0, 1000.0, 0.0), run(0.0, 24.0, 0.0));
        assert_eq!(run(0.0, 0.0, -100.0), run(0.0, 0.0, -24.0));
        // Non-finite gains behave exactly like 0 dB (no NaN in the biquads).
        assert_eq!(run(f64::NAN, 0.0, 0.0), run(0.0, 0.0, 0.0));
        assert_eq!(run(0.0, f64::INFINITY, 0.0), run(0.0, 0.0, 0.0));
        assert_eq!(run(0.0, 0.0, f64::NEG_INFINITY), run(0.0, 0.0, 0.0));
    }

    #[test]
    fn set_sample_rate_retunes_eq_and_preserves_filter_state() {
        // State preservation: a settled DC signal through a +12 dB low
        // shelf must not jump when the rate changes — a reset filter
        // would restart from zero history.
        let mut fx = FxChain::new(48_000.0);
        fx.set_params(
            12.0, 0.0, 0.0, 0.0, 0.5, 250.0, 0.35, 0.0, -18.0, 1.0, 10.0, 100.0, 0.0,
        );
        let mut last = 0.0;
        for _ in 0..4800 {
            (last, _) = fx.process_sample(1.0, 1.0);
        }
        assert!(last > 3.0, "settled +12 dB shelf DC gain, got {last}");
        fx.set_sample_rate(96_000.0);
        let (first, _) = fx.process_sample(1.0, 1.0);
        assert!(
            (first - last).abs() < 0.5,
            "rate change must not reset EQ state: {last} -> {first}"
        );

        // Retuning: a 12 kHz tone through a +12 dB high shelf (corner
        // 8 kHz) must still get the full shelf boost at 96 kHz. With
        // 44.1 kHz coefficients the effective corner would sit near
        // 17.4 kHz and the tone would see far less boost.
        let mut fx = FxChain::new(44_100.0);
        fx.set_params(
            0.0, 0.0, 12.0, 0.0, 0.5, 250.0, 0.35, 0.0, -18.0, 1.0, 10.0, 100.0, 0.0,
        );
        let input = sine(12_000.0, 44_100.0, 4410);
        let mut left = input.clone();
        let mut right = input.clone();
        fx.process_block(&mut left, &mut right);
        fx.set_sample_rate(96_000.0);
        let input = sine(12_000.0, 96_000.0, 9600);
        let mut left = input.clone();
        let mut right = input.clone();
        fx.process_block(&mut left, &mut right);
        let ratio = rms(&left[4800..]) / rms(&input[4800..]);
        assert!(ratio > 3.0, "retuned high shelf should ~quadruple 12 kHz, got {ratio}");
    }

    #[test]
    fn reverb_tail_survives_sample_rate_change() {
        let mut fx = FxChain::new(48_000.0);
        fx.set_params(
            0.0, 0.0, 0.0, 1.0, 0.8, 250.0, 0.35, 0.0, -18.0, 1.0, 10.0, 100.0, 0.0,
        );
        let mut left = vec![0.0; 4096];
        let mut right = vec![0.0; 4096];
        left[0] = 1.0;
        right[0] = 1.0;
        fx.process_block(&mut left, &mut right);
        // The rebuild must keep the sounding tail: silence in must
        // still come out wet immediately, not only after the new
        // delay lines refill (which would take >1000 samples).
        fx.set_sample_rate(44_100.0);
        let mut silence_l = vec![0.0; 512];
        let mut silence_r = vec![0.0; 512];
        fx.process_block(&mut silence_l, &mut silence_r);
        assert!(
            rms(&silence_l) > 1e-7,
            "preserved tail should still ring after rebuild, rms={}",
            rms(&silence_l)
        );
        assert!(silence_l.iter().any(|x| *x != 0.0));
    }

    #[test]
    fn delay_mix_zero_is_transparent() {
        let sr = 48000.0;
        let mut fx = FxChain::new(sr);
        // A long, heavily fed-back delay must still pass through
        // untouched while its mix is 0: the delay is skipped entirely.
        fx.set_params(
            0.0, 0.0, 0.0, 0.0, 0.5, 100.0, 0.9, 0.0, -18.0, 1.0, 10.0, 100.0, 0.0,
        );
        let input: Vec<f64> = (0..4096)
            .map(|i| (i as f64 * 0.37).sin() * 0.8 + (i as f64 * 0.11).cos() * 0.4)
            .collect();
        let mut left = input.clone();
        let mut right = input.clone();
        fx.process_block(&mut left, &mut right);
        let max_diff = left
            .iter()
            .zip(input.iter())
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f64, f64::max);
        assert!(
            max_diff < 1e-12,
            "delay mix 0 should be transparent, got {max_diff}"
        );
        assert_eq!(right, left);
    }

    #[test]
    fn delay_impulse_echo_at_configured_time() {
        let sr = 48000.0;
        let mut fx = FxChain::new(sr);
        // 100 ms at 48 kHz is exactly 4800 samples; no feedback, so a
        // single echo at full level.
        fx.set_params(
            0.0, 0.0, 0.0, 0.0, 0.5, 100.0, 0.0, 1.0, -18.0, 1.0, 10.0, 100.0, 0.0,
        );
        let len = (sr * 0.5) as usize;
        let mut left = vec![0.0; len];
        let mut right = vec![0.0; len];
        left[0] = 1.0;
        right[0] = 0.5;
        fx.process_block(&mut left, &mut right);
        // Nothing before the tap: the wet signal is silence until the
        // impulse comes back around.
        assert!(left[..4800].iter().all(|x| *x == 0.0));
        assert!(right[..4800].iter().all(|x| *x == 0.0));
        // The echo lands exactly on the tap, at the input level.
        assert!(
            (left[4800] - 1.0).abs() < 1e-9,
            "left echo, got {}",
            left[4800]
        );
        assert!(
            (right[4800] - 0.5).abs() < 1e-9,
            "right echo, got {}",
            right[4800]
        );
        // And it is the peak in the neighbourhood, not a few samples off.
        let peak_at = left[4700..4900]
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
            .map(|(i, _)| i + 4700)
            .unwrap();
        assert_eq!(peak_at, 4800, "echo should land exactly on the tap");
    }

    #[test]
    fn delay_feedback_produces_decaying_repeats() {
        let sr = 48000.0;
        let mut fx = FxChain::new(sr);
        // 50 ms at 48 kHz is 2400 samples; feedback 0.5 with the fixed
        // 0.5 feedback lowpass. The lowpass state is updated every
        // sample (as in the comb filters), so over the 2399 silent
        // samples between repeats it decays to ~0 and each repeat is
        // feedback * 0.5 = 0.25 times the previous one.
        fx.set_params(
            0.0, 0.0, 0.0, 0.0, 0.5, 50.0, 0.5, 1.0, -18.0, 1.0, 10.0, 100.0, 0.0,
        );
        let len = (sr * 0.5) as usize;
        let mut left = vec![0.0; len];
        let mut right = vec![0.0; len];
        left[0] = 1.0;
        right[0] = 1.0;
        fx.process_block(&mut left, &mut right);
        let expected = [1.0, 0.25, 0.0625, 0.015625];
        for (n, want) in expected.iter().enumerate() {
            let got = left[(n + 1) * 2400];
            assert!(
                (got - want).abs() < 1e-9,
                "repeat {} should be {want}, got {got}",
                n + 1
            );
        }
        // Later repeats keep decaying, never growing or sticking.
        let fifth = left[5 * 2400];
        assert!(
            fifth > 0.0 && fifth < expected[3],
            "5th repeat should decay, got {fifth}"
        );
        assert!(left[1200] == 0.0, "silence between repeats");
    }

    #[test]
    fn delay_mix_crossfades_dry_wet() {
        let sr = 48000.0;
        let mut fx = FxChain::new(sr);
        // Mix 0.5, no feedback: the dry impulse is halved and the echo
        // is halved.
        fx.set_params(
            0.0, 0.0, 0.0, 0.0, 0.5, 100.0, 0.0, 0.5, -18.0, 1.0, 10.0, 100.0, 0.0,
        );
        let len = (sr * 0.5) as usize;
        let mut left = vec![0.0; len];
        let mut right = vec![0.0; len];
        left[0] = 1.0;
        right[0] = 1.0;
        fx.process_block(&mut left, &mut right);
        assert!((left[0] - 0.5).abs() < 1e-9, "dry half, got {}", left[0]);
        assert!(
            (left[4800] - 0.5).abs() < 1e-9,
            "wet half, got {}",
            left[4800]
        );
    }

    #[test]
    fn delay_time_change_moves_tap_without_clearing() {
        let sr = 48000.0;
        let mut fx = FxChain::new(sr);
        fx.set_params(
            0.0, 0.0, 0.0, 0.0, 0.5, 250.0, 0.0, 1.0, -18.0, 1.0, 10.0, 100.0, 0.0,
        );
        let mut left = vec![0.0; 4800];
        let mut right = vec![0.0; 4800];
        left[0] = 1.0;
        right[0] = 1.0;
        fx.process_block(&mut left, &mut right);
        // No echo yet: the 250 ms tap is 12000 samples out.
        assert!(left.iter().all(|x| *x == 0.0));
        // Shorten to 100 ms (4800 samples): the write head is already
        // at 4800, so the stored impulse is exactly on the new tap and
        // sounds on the very next sample — the buffer was not cleared.
        fx.set_params(
            0.0, 0.0, 0.0, 0.0, 0.5, 100.0, 0.0, 1.0, -18.0, 1.0, 10.0, 100.0, 0.0,
        );
        let mut left = vec![0.0; 4800];
        let mut right = vec![0.0; 4800];
        fx.process_block(&mut left, &mut right);
        assert!(
            (left[0] - 1.0).abs() < 1e-9,
            "retimed echo, got {}",
            left[0]
        );
        assert!(left[1..].iter().all(|x| *x == 0.0));
    }

    #[test]
    fn delay_reset_clears_buffer() {
        let sr = 48000.0;
        let mut fx = FxChain::new(sr);
        fx.set_params(
            0.0, 0.0, 0.0, 0.0, 0.5, 100.0, 0.5, 1.0, -18.0, 1.0, 10.0, 100.0, 0.0,
        );
        let mut left = vec![0.0; 4096];
        let mut right = vec![0.0; 4096];
        left[0] = 1.0;
        right[0] = 1.0;
        fx.process_block(&mut left, &mut right);
        fx.reset();
        // The echo would have arrived 704 samples into this block.
        let mut zeros_l = vec![0.0; 4096];
        let mut zeros_r = vec![0.0; 4096];
        fx.process_block(&mut zeros_l, &mut zeros_r);
        assert!(zeros_l.iter().all(|x| *x == 0.0));
        assert!(zeros_r.iter().all(|x| *x == 0.0));
    }

    #[test]
    fn delay_tail_survives_sample_rate_change() {
        let mut fx = FxChain::new(48_000.0);
        // No feedback, so the line holds only the impulse itself.
        fx.set_params(
            0.0, 0.0, 0.0, 0.0, 0.5, 250.0, 0.0, 1.0, -18.0, 1.0, 10.0, 100.0, 0.0,
        );
        // 10000 samples of pre-roll: the impulse is old enough to
        // survive the shrink to 44.1 kHz (the rebuild keeps the oldest
        // 88200 of 96000 samples) but young enough that the new
        // 11025-sample tap still reaches it before the write head
        // overwrites it. It lands at index 86000 of the new line and
        // is read 8825 samples after the change.
        let mut left = vec![0.0; 10000];
        let mut right = vec![0.0; 10000];
        left[0] = 1.0;
        right[0] = 1.0;
        fx.process_block(&mut left, &mut right);
        // No echo yet: the 250 ms tap at 48 kHz is 12000 samples out.
        assert!(left.iter().all(|x| *x == 0.0));
        fx.set_sample_rate(44_100.0);
        let mut silence_l = vec![0.0; 20000];
        let mut silence_r = vec![0.0; 20000];
        fx.process_block(&mut silence_l, &mut silence_r);
        assert!(
            silence_l[..8825].iter().all(|x| *x == 0.0),
            "nothing should sound before the preserved echo"
        );
        assert!(
            (silence_l[8825] - 1.0).abs() < 1e-9,
            "preserved echo should land at 8825, got {}",
            silence_l[8825]
        );
        assert!((silence_r[8825] - 1.0).abs() < 1e-9);
    }

    #[test]
    fn compressor_ratio_one_applies_exactly_unity_gain() {
        let sr = 48000.0;
        let mut comp = Compressor::new(sr);
        comp.set_params(-18.0, 1.0, 10.0, 100.0, 0.0);
        // Loud 1 kHz sine, well above the -18 dB threshold: the gain
        // computer must yield exactly 1.0 on every sample, so the
        // output is bit-identical to the input.
        let input = sine(1000.0, sr, (sr / 10.0) as usize);
        for &x in &input {
            let sample = 0.9 * x;
            let (l, r) = comp.process(sample, sample);
            assert_eq!(comp.last_gain(), 1.0, "ratio 1 gain must be exactly 1.0");
            assert_eq!(l, sample);
            assert_eq!(r, sample);
        }
    }

    #[test]
    fn compressor_reduces_loud_sine_at_ratio_four() {
        let sr = 48000.0;
        let mut comp = Compressor::new(sr);
        comp.set_params(-18.0, 4.0, 10.0, 100.0, 0.0);
        // 0.9 peak is about -0.9 dB, so the steady-state reduction is
        // about 12.8 dB (linear gain ~0.23).
        let input = sine(1000.0, sr, sr as usize);
        let mut out = vec![0.0; input.len()];
        for (i, &x) in input.iter().enumerate() {
            let (l, _) = comp.process(0.9 * x, 0.9 * x);
            out[i] = l;
        }
        let ratio = rms(&out[sr as usize / 2..]) / (0.9 * rms(&input[sr as usize / 2..]));
        assert!(
            (0.15..0.35).contains(&ratio),
            "ratio 4 should cut a loud sine to ~0.23, got {ratio}"
        );
    }

    #[test]
    fn compressor_fast_attack_clamps_transient_sooner() {
        let sr = 48000.0;
        // Peak of the first 5 ms of a sudden full-scale DC burst.
        let run = |attack_ms: f64| -> f64 {
            let mut comp = Compressor::new(sr);
            comp.set_params(-18.0, 20.0, attack_ms, 100.0, 0.0);
            let mut peak: f64 = 0.0;
            for _ in 0..240 {
                let (l, _) = comp.process(1.0, 1.0);
                peak = peak.max(l.abs());
            }
            peak
        };
        let fast = run(0.1);
        let slow = run(100.0);
        assert!(
            fast < 0.8,
            "0.1 ms attack should clamp the transient, peak={fast}"
        );
        assert!(
            slow > 0.95,
            "100 ms attack should let it through, peak={slow}"
        );
        assert!(
            fast < slow,
            "fast attack must clamp sooner: {fast} vs {slow}"
        );
    }

    #[test]
    fn compressor_makeup_gain_raises_level() {
        let sr = 48000.0;
        let run = |makeup_db: f64| -> f64 {
            let mut comp = Compressor::new(sr);
            comp.set_params(-18.0, 4.0, 10.0, 100.0, makeup_db);
            let input = sine(1000.0, sr, sr as usize);
            let mut out = vec![0.0; input.len()];
            for (i, &x) in input.iter().enumerate() {
                let (l, _) = comp.process(0.9 * x, 0.9 * x);
                out[i] = l;
            }
            rms(&out[sr as usize / 2..])
        };
        let ratio = run(12.0) / run(0.0);
        // +12 dB makeup is a factor of 10^(12/20) = 3.981.
        assert!(
            (ratio - 3.981).abs() < 0.05,
            "makeup +12 dB should x3.98, got {ratio}"
        );
    }

    #[test]
    fn fx_new_params_extreme_values_never_nan() {
        let sr = 48000.0;
        let input: Vec<f64> = (0..4096)
            .map(|i| (i as f64 * 0.37).sin() * 0.8 + (i as f64 * 0.11).cos() * 0.4)
            .collect();
        // Params in delay/compressor order: time, feedback, mix,
        // threshold, ratio, attack, release, makeup.
        let run = |params: [f64; 8]| -> Vec<f64> {
            let mut fx = FxChain::new(sr);
            fx.set_params(
                0.0, 0.0, 0.0, 0.0, 0.5, params[0], params[1], params[2], params[3], params[4],
                params[5], params[6], params[7],
            );
            let mut left = input.clone();
            let mut right = input.clone();
            fx.process_block(&mut left, &mut right);
            assert!(
                left.iter().all(|x| x.is_finite()),
                "FX output must stay finite"
            );
            assert!(
                right.iter().all(|x| x.is_finite()),
                "FX output must stay finite"
            );
            left
        };
        // Non-finite values map to the neutral defaults.
        run([
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NAN,
            f64::INFINITY,
            f64::NAN,
            f64::NEG_INFINITY,
            f64::NAN,
        ]);
        // Out-of-range values behave exactly like the rails.
        assert_eq!(
            run([5000.0, 2.0, 2.0, 100.0, 100.0, 500.0, 5000.0, 100.0]),
            run([2000.0, 0.95, 1.0, 0.0, 20.0, 100.0, 1000.0, 24.0])
        );
        assert_eq!(
            run([-5000.0, -2.0, -2.0, -100.0, 0.0, -5.0, -5.0, -100.0]),
            run([1.0, 0.0, 0.0, -60.0, 1.0, 0.1, 10.0, 0.0])
        );
    }

    #[test]
    fn compressor_reset_clears_envelope() {
        let sr = 48000.0;
        let mut comp = Compressor::new(sr);
        comp.set_params(-18.0, 4.0, 10.0, 100.0, 0.0);
        // Build a hot envelope with a loud burst.
        for _ in 0..4800 {
            comp.process(0.9, 0.9);
        }
        assert!(comp.last_gain() < 0.5, "loud input should be reduced");
        // Without a reset the envelope is still hot, so a moderate
        // sample stays reduced.
        comp.process(0.5, 0.5);
        assert!(comp.last_gain() < 1.0, "hot envelope should still reduce");
        comp.reset();
        let (l, _) = comp.process(0.5, 0.5);
        assert_eq!(comp.last_gain(), 1.0, "reset envelope must give unity gain");
        assert_eq!(l, 0.5);
    }

    #[test]
    fn comb_filter_flushes_subnormal_feedback_state() {
        let mut comb = CombFilter::new(4, 0.98);
        // A subnormal input must be flushed on write, not stored: the
        // first read returns the zero-filled slot, and no subnormal
        // value may linger in the buffer or the feedback state.
        assert_eq!(comb.process(f64::MIN_POSITIVE / 2.0), 0.0);
        assert!(comb.buffer.iter().all(|x| !x.is_subnormal()));
        assert!(!comb.filter_state.is_subnormal());
        // When the written slot comes back around, it must still read
        // exactly zero (without the flush it would return the stored
        // subnormal).
        for _ in 0..8 {
            assert_eq!(comb.process(0.0), 0.0);
        }
        assert_eq!(comb.filter_state, 0.0);
    }
}
