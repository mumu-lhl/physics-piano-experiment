//! Real-time spectrum analyzer widget (shared by all instruments).
//!
//! The plugin's audio thread appends mono output samples to a shared history
//! buffer (same try-lock pattern as the piano orbit vectors); this widget
//! snapshots the latest 1024 samples on each redraw, runs a Hann-windowed
//! real FFT, and draws log-frequency bars in dB. All FFT scratch buffers are
//! allocated once at construction, so redraws allocate nothing.

use crate::skia_compat as vg;
use crate::skia_compat::CanvasExt;
use num_complex::Complex32;
use realfft::{RealFftPlanner, RealToComplex};
use std::cell::RefCell;
use std::sync::Arc;
use vizia_plug::vizia::prelude::*;

/// Samples kept in the shared history (audio thread appends, GUI reads).
pub const SPECTRUM_HISTORY_LEN: usize = 2048;
/// FFT length; also the snapshot size taken from the history.
pub const SPECTRUM_FFT_LEN: usize = 1024;
/// Number of bars drawn.
pub const SPECTRUM_BAR_COUNT: usize = 32;
/// Displayed frequency range (log-spaced bars).
pub const SPECTRUM_FREQ_MIN: f32 = 40.0;
pub const SPECTRUM_FREQ_MAX: f32 = 12000.0;
/// dB floor mapped to zero bar height.
pub const SPECTRUM_DB_FLOOR: f32 = -60.0;

/// Log-spaced bar center frequencies between `f_min` and `f_max`.
pub fn bar_center_frequencies(num_bars: usize, f_min: f32, f_max: f32) -> Vec<f32> {
    assert!(num_bars >= 1 && f_min > 0.0 && f_max > f_min);
    if num_bars == 1 {
        return vec![(f_min * f_max).sqrt()];
    }
    let log_min = f_min.ln();
    let log_max = f_max.ln();
    (0..num_bars)
        .map(|i| (log_min + (log_max - log_min) * (i as f32 + 0.5) / num_bars as f32).exp())
        .collect()
}

/// dB value mapped to a 0..1 bar height (`floor` dB and below map to 0).
pub fn db_to_norm(db: f32, floor: f32) -> f32 {
    ((db - floor) / -floor).clamp(0.0, 1.0)
}

/// FFT analyzer with reusable buffers. `compute` returns per-bar 0..1 norms
/// borrowed from its internal buffer.
pub struct SpectrumAnalyzer {
    fft: Arc<dyn RealToComplex<f32>>,
    input: Vec<f32>,
    output: Vec<Complex32>,
    scratch: Vec<Complex32>,
    bars: Vec<f32>,
}

impl SpectrumAnalyzer {
    pub fn new() -> Self {
        let mut planner = RealFftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(SPECTRUM_FFT_LEN);
        Self {
            input: fft.make_input_vec(),
            output: fft.make_output_vec(),
            scratch: fft.make_scratch_vec(),
            bars: vec![0.0; SPECTRUM_BAR_COUNT],
            fft,
        }
    }

    pub fn compute(&mut self, samples: &[f32], sample_rate: f32) -> &[f32] {
        if sample_rate <= 0.0 {
            self.bars.fill(0.0);
            return &self.bars;
        }
        // Latest samples first, zero-padded at the front when short.
        self.input.fill(0.0);
        let n = samples.len().min(SPECTRUM_FFT_LEN);
        let start = SPECTRUM_FFT_LEN - n;
        self.input[start..].copy_from_slice(&samples[samples.len() - n..]);
        // Hann window.
        for (i, v) in self.input.iter_mut().enumerate() {
            let w = 0.5
                * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / SPECTRUM_FFT_LEN as f32).cos());
            *v *= w;
        }
        if self
            .fft
            .process_with_scratch(&mut self.input, &mut self.output, &mut self.scratch)
            .is_err()
        {
            self.bars.fill(0.0);
            return &self.bars;
        }
        // Per-bar magnitude: peak over the bar's bin range, normalized so a
        // full-scale sine reads ~0 dB (Hann coherent gain 0.5, real FFT).
        // Peak (not mean) keeps narrow resonances visible instead of washing
        // them out across a log-wide bar.
        let centers =
            bar_center_frequencies(SPECTRUM_BAR_COUNT, SPECTRUM_FREQ_MIN, SPECTRUM_FREQ_MAX);
        let bin_hz = sample_rate / SPECTRUM_FFT_LEN as f32;
        let norm_scale = 2.0 / SPECTRUM_FFT_LEN as f32;
        for (i, f) in centers.iter().enumerate() {
            let f_lo = if i == 0 {
                SPECTRUM_FREQ_MIN
            } else {
                (centers[i - 1] * f).sqrt()
            };
            let f_hi = if i + 1 == centers.len() {
                SPECTRUM_FREQ_MAX
            } else {
                (f * centers[i + 1]).sqrt()
            };
            let b_lo = ((f_lo / bin_hz) as usize).min(self.output.len() - 1);
            let b_hi = ((f_hi / bin_hz) as usize)
                .min(self.output.len() - 1)
                .max(b_lo);
            let mut peak = 0.0f32;
            for b in b_lo..=b_hi {
                peak = peak.max(self.output[b].norm());
            }
            let mag = peak * norm_scale;
            let db = 20.0 * (mag + 1e-6).log10();
            self.bars[i] = db_to_norm(db, SPECTRUM_DB_FLOOR);
        }
        &self.bars
    }
}

impl Default for SpectrumAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

pub struct SpectrumAnalyzerWidget {
    history: Arc<parking_lot::RwLock<Vec<f32>>>,
    sample_rate: f32,
    analyzer: RefCell<SpectrumAnalyzer>,
    prev_bars: RefCell<Vec<f32>>,
}

impl SpectrumAnalyzerWidget {
    pub fn new(
        cx: &mut Context,
        history: Arc<parking_lot::RwLock<Vec<f32>>>,
        sample_rate: f32,
    ) -> Handle<'_, Self> {
        Self {
            history,
            sample_rate,
            analyzer: RefCell::new(SpectrumAnalyzer::new()),
            prev_bars: RefCell::new(vec![0.0; SPECTRUM_BAR_COUNT]),
        }
        .build(cx, |_| {})
    }
}

impl View for SpectrumAnalyzerWidget {
    fn element(&self) -> Option<&'static str> {
        Some("spectrum-analyzer-widget")
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = cx.bounds();
        if bounds.w < 20.0 || bounds.h < 10.0 {
            return;
        }

        // Background track.
        let mut bg_path = vg::Path::new();
        bg_path.rounded_rect(bounds.x, bounds.y, bounds.w, bounds.h, 3.0);
        canvas.fill_path(&bg_path, &vg::Paint::color(vg::Color::rgb(14, 16, 22)));
        canvas.stroke_path(
            &bg_path,
            &vg::Paint::color(vg::Color::rgb(45, 50, 65)).with_line_width(0.75),
        );

        let Some(hist) = self.history.try_read() else {
            return;
        };
        let n = hist.len().min(SPECTRUM_FFT_LEN);
        let mut analyzer = self.analyzer.borrow_mut();
        let fresh = if n == 0 {
            analyzer.compute(&[], self.sample_rate)
        } else {
            analyzer.compute(&hist[hist.len() - n..], self.sample_rate)
        };
        // Fast attack, slow release so bars don't flicker.
        let mut prev = self.prev_bars.borrow_mut();
        let mut smoothed = [0.0f32; SPECTRUM_BAR_COUNT];
        for i in 0..SPECTRUM_BAR_COUNT {
            smoothed[i] = fresh[i].max(prev[i] * 0.72);
            prev[i] = smoothed[i];
        }
        drop(prev);

        let gap = 2.0f32;
        let bar_w =
            (bounds.w - gap * (SPECTRUM_BAR_COUNT as f32 - 1.0)) / SPECTRUM_BAR_COUNT as f32;
        if bar_w <= 0.0 {
            return;
        }
        for i in 0..SPECTRUM_BAR_COUNT {
            let h = smoothed[i] * (bounds.h - 4.0);
            if h < 1.0 {
                continue;
            }
            let x = bounds.x + i as f32 * (bar_w + gap);
            let mut bar_path = vg::Path::new();
            bar_path.rounded_rect(x, bounds.y + bounds.h - 2.0 - h, bar_w, h, 1.0);
            canvas.fill_path(&bar_path, &vg::Paint::color(vg::Color::rgb(229, 184, 66)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bar_frequencies_are_log_spaced_and_bounded() {
        let freqs =
            bar_center_frequencies(SPECTRUM_BAR_COUNT, SPECTRUM_FREQ_MIN, SPECTRUM_FREQ_MAX);
        assert_eq!(freqs.len(), SPECTRUM_BAR_COUNT);
        assert!(freqs[0] >= SPECTRUM_FREQ_MIN && freqs[0] < freqs[1]);
        assert!(freqs[SPECTRUM_BAR_COUNT - 1] <= SPECTRUM_FREQ_MAX);
        // Log spacing: consecutive ratios are (nearly) equal.
        let r0 = freqs[1] / freqs[0];
        for w in freqs.windows(2) {
            assert!((w[1] / w[0] - r0).abs() < 1e-4);
        }
    }

    #[test]
    fn db_mapping_hits_floor_and_ceiling() {
        assert_eq!(db_to_norm(0.0, SPECTRUM_DB_FLOOR), 1.0);
        assert_eq!(db_to_norm(SPECTRUM_DB_FLOOR, SPECTRUM_DB_FLOOR), 0.0);
        assert_eq!(db_to_norm(-100.0, SPECTRUM_DB_FLOOR), 0.0);
        assert_eq!(db_to_norm(6.0, SPECTRUM_DB_FLOOR), 1.0);
        assert!((db_to_norm(-30.0, SPECTRUM_DB_FLOOR) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn sine_peak_lands_in_expected_bar() {
        let sample_rate = 44100.0;
        let freq = 1000.0;
        let samples: Vec<f32> = (0..SPECTRUM_FFT_LEN)
            .map(|i| (2.0 * std::f32::consts::PI * freq * i as f32 / sample_rate).sin())
            .collect();
        let mut analyzer = SpectrumAnalyzer::new();
        let bars = analyzer.compute(&samples, sample_rate).to_vec();
        let peak = bars
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .unwrap()
            .0;
        let centers =
            bar_center_frequencies(SPECTRUM_BAR_COUNT, SPECTRUM_FREQ_MIN, SPECTRUM_FREQ_MAX);
        // Peak bar's center within one bar of 1 kHz.
        assert!(
            (centers[peak] / freq > 0.7) && (centers[peak] / freq < 1.4),
            "peak bar {} center {} for 1kHz sine",
            peak,
            centers[peak]
        );
        assert!(bars[peak] > 0.8, "full-scale sine should read near 0 dB");
    }

    #[test]
    fn silence_reads_zero() {
        let mut analyzer = SpectrumAnalyzer::new();
        let bars = analyzer
            .compute(&vec![0.0; SPECTRUM_FFT_LEN], 44100.0)
            .to_vec();
        assert!(bars.iter().all(|&b| b == 0.0));
    }
}
