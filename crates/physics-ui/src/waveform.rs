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

/// Downsample `samples` to one value per column by striding: each column
/// takes the sample nearest its center. Empty input or zero columns yield
/// an empty vec; fewer samples than columns keeps every sample unchanged.
pub fn downsample_waveform(samples: &[f32], columns: usize) -> Vec<f32> {
    if samples.is_empty() || columns == 0 {
        return Vec::new();
    }
    if samples.len() <= columns {
        return samples.to_vec();
    }
    let stride = samples.len() as f32 / columns as f32;
    (0..columns)
        .map(|i| samples[(i as f32 * stride) as usize])
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
        let mut trace = vg::Path::new();
        for (i, v) in values.iter().enumerate() {
            let x = bounds.x + i as f32;
            let y = (mid_y - v * half_h).clamp(bounds.y + 2.0, bounds.y + bounds.h - 2.0);
            if i == 0 {
                trace.move_to(x, y);
            } else {
                trace.line_to(x, y);
            }
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
        assert_eq!(downsample_waveform(&samples, 10), samples);
        assert_eq!(downsample_waveform(&samples, 3), samples);
    }

    #[test]
    fn downsample_strides_to_column_count() {
        let samples: Vec<f32> = (0..10).map(|i| i as f32).collect();
        assert_eq!(
            downsample_waveform(&samples, 5),
            vec![0.0, 2.0, 4.0, 6.0, 8.0]
        );
        let sine: Vec<f32> = (0..1024)
            .map(|i| (2.0 * std::f32::consts::PI * i as f32 / 1024.0).sin())
            .collect();
        let down = downsample_waveform(&sine, 512);
        assert_eq!(down.len(), 512);
        // Quarter-period peak survives striding.
        assert!((down[128] - 1.0).abs() < 1e-6);
        assert!(down[384] < -0.99);
    }
}
