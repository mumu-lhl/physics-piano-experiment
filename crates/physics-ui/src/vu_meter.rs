//! Stereo peak VU meter widget (shared by all instruments).
//!
//! Reads two lock-free peak atoms (left/right, linear gain) written by the
//! plugin's audio thread and draws two horizontal bars scaled in dB
//! (-50 dB .. 0 dB, green / yellow / red near clipping). Previously this lived
//! in the piano crate only; guitar and bass now use the same widget.

use crate::skia_compat as vg;
use crate::skia_compat::CanvasExt;
use atomic_float::AtomicF32;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use vizia_plug::vizia::prelude::*;

pub struct StereoVuMeterWidget {
    peak_l: Arc<AtomicF32>,
    peak_r: Arc<AtomicF32>,
}

impl StereoVuMeterWidget {
    pub fn new(
        cx: &mut Context,
        peak_l: Arc<AtomicF32>,
        peak_r: Arc<AtomicF32>,
    ) -> Handle<'_, Self> {
        Self { peak_l, peak_r }.build(cx, |_| {})
    }
}

impl View for StereoVuMeterWidget {
    fn element(&self) -> Option<&'static str> {
        Some("stereo-vu-meter-widget")
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = cx.bounds();
        if bounds.w < 10.0 || bounds.h < 10.0 {
            return;
        }

        let pl = self.peak_l.load(Ordering::Relaxed).clamp(0.0, 1.5);
        let pr = self.peak_r.load(Ordering::Relaxed).clamp(0.0, 1.5);

        let gain_to_norm = |g: f32| -> f32 {
            if g <= 1e-4 {
                0.0
            } else {
                let db = 20.0 * g.log10();
                ((db + 50.0) / 50.0).clamp(0.0, 1.0)
            }
        };

        let norm_l = gain_to_norm(pl);
        let norm_r = gain_to_norm(pr);

        let bar_h = (bounds.h - 4.0) * 0.5;
        let bar_w = bounds.w;

        let draw_bar = |canvas: &Canvas, by: f32, norm: f32| {
            // Track background
            let mut track_path = vg::Path::new();
            track_path.rounded_rect(bounds.x, by, bar_w, bar_h, 2.0);
            let track_paint = vg::Paint::color(vg::Color::rgb(18, 20, 26));
            canvas.fill_path(&track_path, &track_paint);

            // Active bar
            let fill_w = bar_w * norm;
            if fill_w > 1.0 {
                let mut bar_path = vg::Path::new();
                bar_path.rounded_rect(bounds.x, by, fill_w, bar_h, 2.0);

                let bar_color = if norm > 0.90 {
                    vg::Color::rgb(240, 70, 70) // Red (near clipping)
                } else if norm > 0.70 {
                    vg::Color::rgb(245, 195, 60) // Yellow
                } else {
                    vg::Color::rgb(70, 200, 120) // Green
                };

                let bar_paint = vg::Paint::color(bar_color);
                canvas.fill_path(&bar_path, &bar_paint);
            }

            let border_paint = vg::Paint::color(vg::Color::rgb(35, 40, 52)).with_line_width(0.75);
            canvas.stroke_path(&track_path, &border_paint);
        };

        draw_bar(canvas, bounds.y, norm_l);
        draw_bar(canvas, bounds.y + bar_h + 3.0, norm_r);
    }
}
