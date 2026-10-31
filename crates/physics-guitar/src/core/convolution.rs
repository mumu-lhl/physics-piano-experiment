//! Uniformly Partitioned Overlap-Save (UPOLS) Convolution IR Engine.
//!
//! Provides ultra-low latency partitioned convolution (block size = 64 samples,
//! latency ~1.33ms at 48kHz) for electric guitar cabinet impulse responses (IRs)
//! and acoustic body soundboard residual responses without audio-thread allocation.

use std::f64::consts::PI;

pub const UPOLS_BLOCK_SIZE: usize = 64;
pub const UPOLS_FFT_SIZE: usize = UPOLS_BLOCK_SIZE * 2; // 128
pub const MAX_PARTITIONS: usize = 16; // 16 * 64 = 1024 taps (~21ms at 48kHz)

/// Complex number for lightweight, self-contained FFT/IFFT without heavy external crates.
#[derive(Debug, Clone, Copy, Default)]
pub struct Complex {
    pub re: f64,
    pub im: f64,
}

impl Complex {
    #[inline(always)]
    pub fn new(re: f64, im: f64) -> Self {
        Self { re, im }
    }

    #[inline(always)]
    pub fn mul(self, rhs: Self) -> Self {
        Self {
            re: self.re * rhs.re - self.im * rhs.im,
            im: self.re * rhs.im + self.im * rhs.re,
        }
    }

    #[inline(always)]
    pub fn add(self, rhs: Self) -> Self {
        Self {
            re: self.re + rhs.re,
            im: self.im + rhs.im,
        }
    }
}

/// Radix-2 in-place Cooley-Tukey FFT of length N (N must be power of 2, here 128).
pub fn fft_128(buffer: &mut [Complex; UPOLS_FFT_SIZE]) {
    let n = UPOLS_FFT_SIZE;
    // Bit-reversal permutation
    let mut j = 0;
    for i in 0..n {
        if i < j {
            buffer.swap(i, j);
        }
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
    }

    // Cooley-Tukey decimation in time
    let mut len = 2;
    while len <= n {
        let half = len / 2;
        let angle = -2.0 * PI / (len as f64);
        let w_step = Complex::new(angle.cos(), angle.sin());
        let mut i = 0;
        while i < n {
            let mut w = Complex::new(1.0, 0.0);
            for k in 0..half {
                let u = buffer[i + k];
                let v = buffer[i + k + half].mul(w);
                buffer[i + k] = u.add(v);
                buffer[i + k + half] = Complex::new(u.re - v.re, u.im - v.im);
                w = w.mul(w_step);
            }
            i += len;
        }
        len <<= 1;
    }
}

/// Radix-2 in-place IFFT of length N.
pub fn ifft_128(buffer: &mut [Complex; UPOLS_FFT_SIZE]) {
    // Conjugate input
    for c in buffer.iter_mut() {
        c.im = -c.im;
    }
    fft_128(buffer);
    // Conjugate output and scale by 1/N
    let inv_n = 1.0 / (UPOLS_FFT_SIZE as f64);
    for c in buffer.iter_mut() {
        c.re *= inv_n;
        c.im = -c.im * inv_n;
    }
}

/// Preset cabinet impulse response profiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CabinetIrProfile {
    CelestionVintage30,
    FenderTwinReverb,
    MarshallGreenback,
    Custom,
}

/// Zero-allocation Uniformly Partitioned Overlap-Save (UPOLS) convolution engine.
#[derive(Debug, Clone)]
pub struct UpolsConvolutionEngine {
    /// Frequency domain response partitions: [num_partitions][UPOLS_FFT_SIZE]
    partitions: Vec<[Complex; UPOLS_FFT_SIZE]>,
    pub num_partitions: usize,

    /// Frequency domain input history delay-line: [MAX_PARTITIONS][UPOLS_FFT_SIZE]
    input_history: Vec<[Complex; UPOLS_FFT_SIZE]>,
    input_history_index: usize,

    /// Time-domain input FIFO buffer of length UPOLS_FFT_SIZE (retaining previous block)
    input_buffer: [f64; UPOLS_FFT_SIZE],
    input_fill: usize,

    /// Time-domain output block buffer of length UPOLS_BLOCK_SIZE
    output_buffer: [f64; UPOLS_BLOCK_SIZE],
    output_index: usize,

    /// Frequency domain accumulator
    freq_acc: [Complex; UPOLS_FFT_SIZE],

    pub profile: CabinetIrProfile,
    pub is_enabled: bool,
    pub mix: f64,
    /// Sample rate the current IR was synthesized at. Factory IRs are
    /// synthesized on this time grid (`t = i / sample_rate`) with the
    /// tap count scaled to the rate, so the cabinet tone does not
    /// shift with the host rate.
    pub sample_rate: f64,
}

/// Synthesizes a factory cabinet impulse response on a
/// `sample_rate` time grid: `t = i / sample_rate`, so the 105 Hz
/// mechanical thump, 420 Hz wood resonance and 2.8 kHz presence
/// bite keep their frequencies at any host rate. The tap count
/// scales with the rate (512 taps at 44.1 kHz, ~11.6 ms) and is
/// capped by the caller's buffer, so the IR duration stays
/// constant instead of shrinking at high rates. Returns the
/// number of taps written.
fn synthesize_preset_ir(
    profile: CabinetIrProfile,
    sample_rate: f64,
    out: &mut [f64],
) -> usize {
    let rate = sample_rate.max(1.0);
    let taps = ((512.0 * rate / 44100.0).round() as usize).clamp(1, out.len());
    let decay_rate = match profile {
        CabinetIrProfile::CelestionVintage30 => 120.0,
        CabinetIrProfile::FenderTwinReverb => 85.0,
        CabinetIrProfile::MarshallGreenback => 105.0,
        CabinetIrProfile::Custom => 100.0,
    };
    // Synthesize authentic cabinet resonance envelope with speaker cone damping
    for (i, sample) in out.iter_mut().take(taps).enumerate() {
        let t = i as f64 / rate;
        let envelope = (-decay_rate * t).exp();
        let mechanical_thump = (2.0 * PI * 105.0 * t).sin() * 0.45;
        let presence_bite = (2.0 * PI * 2800.0 * t).sin() * 0.35 * (-450.0 * t).exp();
        let wood_resonance = (2.0 * PI * 420.0 * t).sin() * 0.25;
        *sample = envelope * (mechanical_thump + presence_bite + wood_resonance);
    }
    taps
}

impl UpolsConvolutionEngine {
    pub fn new(sample_rate: f64) -> Self {
        let mut engine = Self {
            partitions: vec![[Complex::default(); UPOLS_FFT_SIZE]; MAX_PARTITIONS],
            num_partitions: 1,
            input_history: vec![[Complex::default(); UPOLS_FFT_SIZE]; MAX_PARTITIONS],
            input_history_index: 0,
            input_buffer: [0.0; UPOLS_FFT_SIZE],
            input_fill: 0,
            output_buffer: [0.0; UPOLS_BLOCK_SIZE],
            output_index: 0,
            freq_acc: [Complex::default(); UPOLS_FFT_SIZE],
            profile: CabinetIrProfile::CelestionVintage30,
            is_enabled: true,
            mix: 1.0,
            sample_rate: sample_rate.max(1.0),
        };
        let rate = engine.sample_rate;
        engine.load_preset(CabinetIrProfile::CelestionVintage30, rate);
        engine
    }

    /// Loads a factory cabinet impulse response profile, synthesized
    /// at `sample_rate` (see [`synthesize_preset_ir`]).
    pub fn load_preset(&mut self, profile: CabinetIrProfile, sample_rate: f64) {
        self.sample_rate = sample_rate.max(1.0);
        self.profile = profile;
        let mut ir = [0.0_f64; MAX_PARTITIONS * UPOLS_BLOCK_SIZE];
        let taps = synthesize_preset_ir(profile, self.sample_rate, &mut ir);
        self.load_ir(&ir[..taps]);
    }

    /// Partitions and transforms a time-domain impulse response into UPOLS frequency blocks.
    pub fn load_ir(&mut self, ir: &[f64]) {
        let total_taps = ir.len().min(MAX_PARTITIONS * UPOLS_BLOCK_SIZE);
        self.num_partitions = (total_taps + UPOLS_BLOCK_SIZE - 1) / UPOLS_BLOCK_SIZE;
        self.num_partitions = self.num_partitions.clamp(1, MAX_PARTITIONS);

        for p in 0..self.num_partitions {
            let mut time_block = [Complex::default(); UPOLS_FFT_SIZE];
            let start = p * UPOLS_BLOCK_SIZE;
            for i in 0..UPOLS_BLOCK_SIZE {
                if start + i < ir.len() {
                    time_block[i].re = ir[start + i];
                }
            }
            fft_128(&mut time_block);
            self.partitions[p] = time_block;
        }

        self.reset();
    }

    /// Clears internal delay lines and state buffers.
    pub fn reset(&mut self) {
        for hist in self.input_history.iter_mut() {
            hist.fill(Complex::default());
        }
        self.input_history_index = 0;
        self.input_buffer.fill(0.0);
        self.input_fill = 0;
        self.output_buffer.fill(0.0);
        self.output_index = 0;
        self.freq_acc.fill(Complex::default());
    }

    /// Processes a single input sample through the UPOLS convolution pipeline with zero allocation.
    #[inline]
    pub fn process_sample(&mut self, input: f64) -> f64 {
        if !self.is_enabled {
            return input;
        }

        // Store input into the current block of input_buffer
        self.input_buffer[UPOLS_BLOCK_SIZE + self.input_fill] = input;
        self.input_fill += 1;

        let conv_out = self.output_buffer[self.output_index];
        self.output_index += 1;

        // When a full block of UPOLS_BLOCK_SIZE samples is accumulated:
        if self.input_fill >= UPOLS_BLOCK_SIZE {
            self.process_block();
            self.input_fill = 0;
            self.output_index = 0;
        }

        input * (1.0 - self.mix) + conv_out * self.mix
    }

    /// Executes the uniform frequency-domain block convolution.
    fn process_block(&mut self) {
        // Prepare 2B time-domain input vector
        let mut time_in = [Complex::default(); UPOLS_FFT_SIZE];
        for i in 0..UPOLS_FFT_SIZE {
            time_in[i].re = self.input_buffer[i];
        }

        // FFT of input block: X_k
        fft_128(&mut time_in);

        // Shift input history pointer (circular buffer)
        if self.input_history_index == 0 {
            self.input_history_index = self.num_partitions - 1;
        } else {
            self.input_history_index -= 1;
        }
        self.input_history[self.input_history_index] = time_in;

        // Clear frequency accumulator
        self.freq_acc.fill(Complex::default());

        // Multiply-accumulate across all partitions: Y = Sum_p (X_{k-p} * H_p)
        for p in 0..self.num_partitions {
            let hist_idx = (self.input_history_index + p) % self.num_partitions;
            let x_p = &self.input_history[hist_idx];
            let h_p = &self.partitions[p];
            for k in 0..UPOLS_FFT_SIZE {
                self.freq_acc[k] = self.freq_acc[k].add(x_p[k].mul(h_p[k]));
            }
        }

        // IFFT of accumulated frequency response
        ifft_128(&mut self.freq_acc);

        // In Overlap-Save, discard first B samples and keep second B samples as output
        for i in 0..UPOLS_BLOCK_SIZE {
            self.output_buffer[i] = self.freq_acc[UPOLS_BLOCK_SIZE + i].re;
        }

        // Shift input buffer: move current block to previous block position
        for i in 0..UPOLS_BLOCK_SIZE {
            self.input_buffer[i] = self.input_buffer[UPOLS_BLOCK_SIZE + i];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_ir_tap_count_scales_with_sample_rate() {
        let mut buf = [0.0f64; MAX_PARTITIONS * UPOLS_BLOCK_SIZE];
        assert_eq!(
            synthesize_preset_ir(CabinetIrProfile::CelestionVintage30, 44100.0, &mut buf),
            512
        );
        assert_eq!(
            synthesize_preset_ir(CabinetIrProfile::CelestionVintage30, 48000.0, &mut buf),
            557
        );
        // 512 * 96000 / 44100 = 1114 taps, capped by the 1024-tap buffer.
        assert_eq!(
            synthesize_preset_ir(CabinetIrProfile::CelestionVintage30, 96000.0, &mut buf),
            1024
        );
        // Engine partition counts follow the tap counts (64 taps each).
        assert_eq!(UpolsConvolutionEngine::new(44100.0).num_partitions, 8);
        assert_eq!(UpolsConvolutionEngine::new(48000.0).num_partitions, 9);
        assert_eq!(UpolsConvolutionEngine::new(96000.0).num_partitions, 16);
        // IR duration stays constant (~11.6 ms) instead of shrinking
        // with the rate (96 kHz is capped slightly short, 10.7 ms).
        assert!((557.0f64 / 48000.0 - 512.0 / 44100.0).abs() < 1e-4);
        assert!((1024.0f64 / 96000.0 - 512.0 / 44100.0).abs() < 1e-3);
    }

    #[test]
    fn preset_ir_thump_frequency_stable_across_sample_rates() {
        // A direct spectral comparison cannot resolve the 105 Hz thump
        // from the old hardcoded-grid 114 Hz (at 48 kHz) over an
        // 11.6 ms IR: the main-lobe width is ~86 Hz. The checkable
        // invariant is the synthesis time grid itself: samples at
        // different rates must equal the same continuous-time
        // waveform evaluated at t = i / sample_rate, which is what
        // keeps every resonance frequency rate-independent.
        for rate in [44100.0, 48000.0, 96000.0] {
            let mut buf = [0.0f64; MAX_PARTITIONS * UPOLS_BLOCK_SIZE];
            let taps = synthesize_preset_ir(CabinetIrProfile::CelestionVintage30, rate, &mut buf);
            for &i in &[0usize, 1, 17, 100, 255, 511] {
                if i >= taps {
                    continue;
                }
                let t = i as f64 / rate;
                let expected = (-120.0 * t).exp()
                    * ((2.0 * PI * 105.0 * t).sin() * 0.45
                        + (2.0 * PI * 2800.0 * t).sin() * 0.35 * (-450.0 * t).exp()
                        + (2.0 * PI * 420.0 * t).sin() * 0.25);
                assert!(
                    (buf[i] - expected).abs() < 1e-12,
                    "rate {rate} tap {i}: {} vs {expected}",
                    buf[i]
                );
            }

            // The engine stores the rate it synthesized at, and a
            // preset reload at a new rate re-synthesizes on that grid.
            let engine = UpolsConvolutionEngine::new(rate);
            assert_eq!(engine.sample_rate, rate);
            let mut reloaded = UpolsConvolutionEngine::new(44100.0);
            reloaded.load_preset(CabinetIrProfile::FenderTwinReverb, rate);
            assert_eq!(reloaded.sample_rate, rate);

            // Impulse smoke test at this rate: finite, nonzero output.
            let mut engine = UpolsConvolutionEngine::new(rate);
            let mut energy = 0.0f64;
            for n in 0..512 {
                let y = engine.process_sample(if n == 0 { 1.0 } else { 0.0 });
                assert!(y.is_finite(), "non-finite IR output at {rate} Hz");
                energy += y * y;
            }
            assert!(energy > 0.0, "silent IR at {rate} Hz");
        }
    }
}
