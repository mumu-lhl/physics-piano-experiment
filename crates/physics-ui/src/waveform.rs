//! Real-time waveform (oscilloscope) widget, shared by all instruments.
//!
//! Reads the same shared history buffer as the spectrum analyzer and draws
//! the latest samples as a time-domain trace. Snapshots are taken with the
//! same non-blocking try-lock so the audio thread is never stalled, and
//! redraws ride the existing display refresh, so the trace follows playback
//! with no timer of its own.

use crate::skia_compat as vg;
use crate::skia_compat::CanvasExt;
use std::sync::Arc;
use vizia_plug::vizia::prelude::*;

/// Samples drawn, taken from the tail of the shared history.
pub const WAVEFORM_WINDOW: usize = 1024;

/// Downsample `samples` to one min/max envelope pair per column: each
/// column covers a contiguous slice of the input and reports its minimum
/// and maximum, so peaks survive decimation instead of being skipped by
/// striding. Empty input or zero columns yield an empty vec; fewer
/// samples than columns keep every sample (as a degenerate pair), and
/// the draw step stretches those pairs across the full widget width.
pub fn downsample_waveform(samples: &[f32], columns: usize) -> Vec<(f32, f32)> {
    if samples.is_empty() || columns == 0 {
        return Vec::new();
    }
    if samples.len() <= columns {
        return samples.iter().map(|&v| (v, v)).collect();
    }
    (0..columns)
        .map(|i| {
            let start = i * samples.len() / columns;
            let end = (i + 1) * samples.len() / columns;
            let slice = &samples[start..end];
            let mut min = f32::INFINITY;
            let mut max = f32::NEG_INFINITY;
            for &v in slice {
                min = min.min(v);
                max = max.max(v);
            }
            (min, max)
        })
        .collect()
}

pub struct WaveformWidget {
    history: Arc<parking_lot::RwLock<Vec<f32>>>,
}

impl WaveformWidget {
    pub fn new(cx: &mut Context, history: Arc<parking_lot::RwLock<Vec<f32>>>) -> Handle<'_, Self> {
        Self { history }.build(cx, |_| {})
    }
}

impl View for WaveformWidget {
    fn element(&self) -> Option<&'static str> {
        Some("waveform-widget")
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = cx.bounds();
        if bounds.w < 20.0 || bounds.h < 10.0 {
            return;
        }

        // Background track, same as the spectrum analyzer.
        let mut bg_path = vg::Path::new();
        bg_path.rounded_rect(bounds.x, bounds.y, bounds.w, bounds.h, 3.0);
        canvas.fill_path(&bg_path, &vg::Paint::color(vg::Color::rgb(14, 16, 22)));
        canvas.stroke_path(
            &bg_path,
            &vg::Paint::color(vg::Color::rgb(45, 50, 65)).with_line_width(0.75),
        );

        // Zero line.
        let mid_y = bounds.y + bounds.h / 2.0;
        let mut mid_path = vg::Path::new();
        mid_path.move_to(bounds.x + 2.0, mid_y);
        mid_path.line_to(bounds.x + bounds.w - 2.0, mid_y);
        canvas.stroke_path(
            &mid_path,
            &vg::Paint::color(vg::Color::rgb(45, 50, 65)).with_line_width(0.75),
        );

        let Some(hist) = self.history.try_read() else {
            return;
        };
        let n = hist.len().min(WAVEFORM_WINDOW);
        if n == 0 {
            return;
        }
        let columns = (bounds.w as usize).max(1);
        let values = downsample_waveform(&hist[hist.len() - n..], columns);
        let half_h = bounds.h / 2.0 - 3.0;
        // One vertical segment per column, centered in its slot so the
        // trace stretches across the full width even when there are
        // fewer samples than pixels (high zoom / narrow window).
        let mut trace = vg::Path::new();
        let count = values.len() as f32;
        for (i, (min, max)) in values.iter().enumerate() {
            let x = bounds.x + (i as f32 + 0.5) * bounds.w / count;
            let y_top = (mid_y - max * half_h).clamp(bounds.y + 2.0, bounds.y + bounds.h - 2.0);
            let mut y_bottom =
                (mid_y - min * half_h).clamp(bounds.y + 2.0, bounds.y + bounds.h - 2.0);
            if y_bottom - y_top < 1.0 {
                // A flat column still needs a visible mark.
                y_bottom = y_top + 1.0;
            }
            trace.move_to(x, y_top);
            trace.line_to(x, y_bottom);
        }
        canvas.stroke_path(
            &trace,
            &vg::Paint::color(vg::Color::rgb(229, 184, 66)).with_line_width(1.25),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downsample_empty_and_zero_columns() {
        assert!(downsample_waveform(&[], 10).is_empty());
        assert!(downsample_waveform(&[0.5, -0.5], 0).is_empty());
    }

    #[test]
    fn downsample_keeps_short_inputs_unchanged() {
        let samples = vec![0.25, -0.75, 0.5];
        let expected: Vec<(f32, f32)> = samples.iter().map(|&v| (v, v)).collect();
        assert_eq!(downsample_waveform(&samples, 10), expected);
        assert_eq!(downsample_waveform(&samples, 3), expected);
    }

    #[test]
    fn downsample_reports_min_max_envelope_per_column() {
        let samples: Vec<f32> = (0..10).map(|i| i as f32).collect();
        assert_eq!(
            downsample_waveform(&samples, 5),
            vec![(0.0, 1.0), (2.0, 3.0), (4.0, 5.0), (6.0, 7.0), (8.0, 9.0)]
        );
        // A lone peak inside a column survives decimation (striding
        // would skip it whenever it fell between stride centers).
        let mut spiked = vec![0.0; 100];
        spiked[37] = 1.0;
        spiked[63] = -1.0;
        let down = downsample_waveform(&spiked, 10);
        assert_eq!(down.len(), 10);
        assert_eq!(down[3], (0.0, 1.0));
        assert_eq!(down[6], (-1.0, 0.0));
        assert!(down.iter().all(|&(lo, hi)| lo <= hi));
    }

    #[test]
    fn downsample_envelope_covers_every_sample() {
        // 7 samples into 3 columns: slices are [0..2], [2..4], [4..7]
        // (integer partition, no sample skipped or double-counted).
        let samples = vec![1.0, -2.0, 3.0, -4.0, 5.0, -6.0, 7.0];
        assert_eq!(
            downsample_waveform(&samples, 3),
            vec![(-2.0, 1.0), (-4.0, 3.0), (-6.0, 7.0)]
        );
    }
}
