use std::f64::consts::PI;

/// Second-order IIR filter using Direct Form I.
///
/// The filter is allocation-free in the audio path. Coefficients can be updated
/// while preserving input/output history, avoiding a reset of recent signal state.
#[derive(Debug, Clone, Copy)]
pub struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    x1: f64,
    x2: f64,
    y1: f64,
    y2: f64,
}

impl Biquad {
    /// Creates a silent filter. Useful as an array initializer before assigning
    /// per-mode filters.
    pub const fn zero() -> Self {
        Self {
            b0: 0.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    /// Creates a filter from normalized biquad coefficients.
    ///
    /// The denominator is `1 + a1 z^-1 + a2 z^-2`.
    pub fn from_coefficients(b0: f64, b1: f64, b2: f64, a1: f64, a2: f64) -> Self {
        Self {
            b0,
            b1,
            b2,
            a1,
            a2,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    /// Replaces coefficients while preserving the input/output history.
    pub fn set_coefficients(&mut self, b0: f64, b1: f64, b2: f64, a1: f64, a2: f64) {
        self.b0 = b0;
        self.b1 = b1;
        self.b2 = b2;
        self.a1 = a1;
        self.a2 = a2;
    }

    #[inline]
    fn angular_frequency(sample_rate: f64, frequency: f64) -> f64 {
        2.0 * PI * (frequency / sample_rate).clamp(0.0001, 0.499)
    }

    #[inline]
    fn replace_coefficients(&mut self, replacement: Self) {
        self.set_coefficients(
            replacement.b0,
            replacement.b1,
            replacement.b2,
            replacement.a1,
            replacement.a2,
        );
    }

    /// Second-order low-pass filter.
    pub fn lowpass(sample_rate: f64, frequency: f64, q: f64) -> Self {
        let w0 = Self::angular_frequency(sample_rate, frequency);
        let alpha = w0.sin() / (2.0 * q.max(0.1));
        let cos_w0 = w0.cos();
        let a0 = 1.0 + alpha;

        Self::from_coefficients(
            ((1.0 - cos_w0) * 0.5) / a0,
            (1.0 - cos_w0) / a0,
            ((1.0 - cos_w0) * 0.5) / a0,
            (-2.0 * cos_w0) / a0,
            (1.0 - alpha) / a0,
        )
    }

    /// Compatibility constructor with `(frequency, q, sample_rate)` ordering.
    pub fn new_lowpass(frequency: f64, q: f64, sample_rate: f64) -> Self {
        Self::lowpass(sample_rate, frequency, q)
    }

    /// Second-order high-pass filter.
    pub fn highpass(sample_rate: f64, frequency: f64, q: f64) -> Self {
        let w0 = Self::angular_frequency(sample_rate, frequency);
        let alpha = w0.sin() / (2.0 * q.max(0.1));
        let cos_w0 = w0.cos();
        let a0 = 1.0 + alpha;

        Self::from_coefficients(
            ((1.0 + cos_w0) * 0.5) / a0,
            (-(1.0 + cos_w0)) / a0,
            ((1.0 + cos_w0) * 0.5) / a0,
            (-2.0 * cos_w0) / a0,
            (1.0 - alpha) / a0,
        )
    }

    /// Compatibility constructor with `(frequency, q, sample_rate)` ordering.
    pub fn new_highpass(frequency: f64, q: f64, sample_rate: f64) -> Self {
        Self::highpass(sample_rate, frequency, q)
    }

    /// Constant-peak-gain second-order band-pass filter.
    pub fn bandpass(sample_rate: f64, frequency: f64, q: f64) -> Self {
        let w0 = Self::angular_frequency(sample_rate, frequency);
        let alpha = w0.sin() / (2.0 * q.max(0.1));
        let cos_w0 = w0.cos();
        let a0 = 1.0 + alpha;

        Self::from_coefficients(
            alpha / a0,
            0.0,
            -alpha / a0,
            (-2.0 * cos_w0) / a0,
            (1.0 - alpha) / a0,
        )
    }

    /// Compatibility constructor with `(frequency, q, sample_rate)` ordering.
    pub fn new_bandpass(frequency: f64, q: f64, sample_rate: f64) -> Self {
        Self::bandpass(sample_rate, frequency, q)
    }

    /// Second-order peaking EQ filter.
    pub fn peaking(sample_rate: f64, frequency: f64, q: f64, gain_db: f64) -> Self {
        let w0 = Self::angular_frequency(sample_rate, frequency);
        let a = 10.0f64.powf(gain_db / 40.0);
        let alpha = w0.sin() / (2.0 * q.max(0.1));
        let cos_w0 = w0.cos();
        let a0 = 1.0 + alpha / a;

        Self::from_coefficients(
            (1.0 + alpha * a) / a0,
            (-2.0 * cos_w0) / a0,
            (1.0 - alpha * a) / a0,
            (-2.0 * cos_w0) / a0,
            (1.0 - alpha / a) / a0,
        )
    }

    /// Second-order high-shelf filter with slope 1 (Q ~= 0.707).
    pub fn high_shelf(sample_rate: f64, frequency: f64, gain_db: f64) -> Self {
        let a = 10.0f64.powf(gain_db / 40.0);
        let w0 = Self::angular_frequency(sample_rate, frequency);
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / 2.0 * (2.0f64).sqrt();
        let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;

        let a0 = (a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha;
        let b0 = a * ((a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha);
        let b1 = -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w0);
        let b2 = a * ((a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha);
        let a1 = 2.0 * ((a - 1.0) - (a + 1.0) * cos_w0);
        let a2 = (a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha;

        Self::from_coefficients(b0 / a0, b1 / a0, b2 / a0, a1 / a0, a2 / a0)
    }

    /// A damped modal resonator excited by an impulse-like input.
    pub fn resonator(sample_rate: f64, frequency: f64, q: f64) -> Self {
        let w0 = Self::angular_frequency(sample_rate, frequency);
        let r = (-w0 / (2.0 * q.max(0.1))).exp();
        let cos_w0 = w0.cos();

        Self::from_coefficients(w0.sin(), 0.0, 0.0, -2.0 * r * cos_w0, r * r)
    }

    pub fn set_lowpass(&mut self, sample_rate: f64, frequency: f64, q: f64) {
        self.replace_coefficients(Self::lowpass(sample_rate, frequency, q));
    }

    pub fn set_highpass(&mut self, sample_rate: f64, frequency: f64, q: f64) {
        self.replace_coefficients(Self::highpass(sample_rate, frequency, q));
    }

    pub fn set_bandpass(&mut self, sample_rate: f64, frequency: f64, q: f64) {
        self.replace_coefficients(Self::bandpass(sample_rate, frequency, q));
    }

    pub fn set_peaking(&mut self, sample_rate: f64, frequency: f64, q: f64, gain_db: f64) {
        self.replace_coefficients(Self::peaking(sample_rate, frequency, q, gain_db));
    }

    pub fn set_high_shelf(&mut self, sample_rate: f64, frequency: f64, gain_db: f64) {
        self.replace_coefficients(Self::high_shelf(sample_rate, frequency, gain_db));
    }

    #[inline(always)]
    pub fn process(&mut self, input: f64) -> f64 {
        let output = self.b0 * input + self.b1 * self.x1 + self.b2 * self.x2
            - self.a1 * self.y1
            - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = input;
        self.y2 = self.y1;
        self.y1 = output;
        output
    }

    pub fn reset(&mut self) {
        self.x1 = 0.0;
        self.x2 = 0.0;
        self.y1 = 0.0;
        self.y2 = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::Biquad;

    #[test]
    fn constructors_remain_finite_and_stable() {
        let constructors = [
            Biquad::lowpass(48_000.0, 1_000.0, 0.707),
            Biquad::highpass(48_000.0, 1_000.0, 0.707),
            Biquad::bandpass(48_000.0, 1_000.0, 2.0),
            Biquad::peaking(48_000.0, 2_800.0, 2.0, 4.0),
            Biquad::high_shelf(48_000.0, 3_500.0, -12.0),
            Biquad::resonator(48_000.0, 180.0, 3.0),
        ];

        for mut filter in constructors {
            let mut peak = 0.0_f64;
            for sample in std::iter::once(1.0).chain(std::iter::repeat_n(0.0, 4_799)) {
                let output = filter.process(sample);
                assert!(output.is_finite());
                peak = peak.max(output.abs());
            }
            assert!(peak > 0.0);
        }
    }

    #[test]
    fn coefficient_updates_preserve_state() {
        let mut filter = Biquad::bandpass(48_000.0, 1_000.0, 2.0);
        filter.process(1.0);
        filter.set_bandpass(48_000.0, 2_000.0, 2.0);
        assert!(filter.process(0.0).is_finite());
    }
}
