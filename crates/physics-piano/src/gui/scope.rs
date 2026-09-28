//! Real-time Lissajous Scope and Stereo Peak VU Meter Widgets for Vizia.

use atomic_float::AtomicF32;
use nih_plug_vizia::vizia::prelude::*;
use nih_plug_vizia::vizia::vg;
use std::sync::atomic::Ordering;
use std::sync::Arc;

pub struct LissajousScopeWidget {
    orbit_t: Arc<parking_lot::RwLock<Vec<f32>>>,
    orbit_p: Arc<parking_lot::RwLock<Vec<f32>>>,
}

impl LissajousScopeWidget {
    pub fn new<L1, L2>(cx: &mut Context, orbit_t: L1, orbit_p: L2) -> Handle<'_, Self>
    where
        L1: Lens<Target = Arc<parking_lot::RwLock<Vec<f32>>>>,
        L2: Lens<Target = Arc<parking_lot::RwLock<Vec<f32>>>>,
    {
        Self {
            orbit_t: orbit_t.get(cx),
            orbit_p: orbit_p.get(cx),
        }
        .build(cx, |_| {})
    }
}

impl View for LissajousScopeWidget {
    fn element(&self) -> Option<&'static str> {
        Some("lissajous-scope-widget")
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &mut Canvas) {
        let bounds = cx.bounds();
        if bounds.w < 10.0 || bounds.h < 10.0 {
            return;
        }

        let cx_mid = bounds.x + bounds.w * 0.5;
        let cy_mid = bounds.y + bounds.h * 0.5;
        let radius = (bounds.w.min(bounds.h) * 0.46).max(4.0);

        // Circular Scope Display Glass
        let mut glass_path = vg::Path::new();
        glass_path.circle(cx_mid, cy_mid, radius);
        let glass_paint = vg::Paint::color(vg::Color::rgb(14, 16, 22));
        canvas.fill_path(&glass_path, &glass_paint);
        let glass_stroke = vg::Paint::color(vg::Color::rgb(45, 50, 65)).with_line_width(1.0);
        canvas.stroke_path(&glass_path, &glass_stroke);

        // Crosshairs
        let mut grid_path = vg::Path::new();
        grid_path.move_to(cx_mid - radius * 0.85, cy_mid);
        grid_path.line_to(cx_mid + radius * 0.85, cy_mid);
        grid_path.move_to(cx_mid, cy_mid - radius * 0.85);
        grid_path.line_to(cx_mid, cy_mid + radius * 0.85);
        let grid_paint = vg::Paint::color(vg::Color::rgbaf(0.3, 0.4, 0.55, 0.25)).with_line_width(0.8);
        canvas.stroke_path(&grid_path, &grid_paint);

        // Render Orbit
        let ot_guard = self.orbit_t.read();
        let op_guard = self.orbit_p.read();
        let n = ot_guard.len().min(op_guard.len());

        if n > 1 {
            let mut orbit_path = vg::Path::new();
            let p0_x = cx_mid + ot_guard[0].clamp(-1.0, 1.0) * radius * 0.88;
            let p0_y = cy_mid - op_guard[0].clamp(-1.0, 1.0) * radius * 0.88;
            orbit_path.move_to(p0_x, p0_y);

            for i in 1..n {
                let px = cx_mid + ot_guard[i].clamp(-1.0, 1.0) * radius * 0.88;
                let py = cy_mid - op_guard[i].clamp(-1.0, 1.0) * radius * 0.88;
                orbit_path.line_to(px, py);
            }

            let orbit_paint = vg::Paint::color(vg::Color::rgbaf(1.0, 0.82, 0.35, 0.88)).with_line_width(1.2);
            canvas.stroke_path(&orbit_path, &orbit_paint);
        }
    }
}

pub struct StereoVuMeterWidget {
    peak_l: Arc<AtomicF32>,
    peak_r: Arc<AtomicF32>,
}

impl StereoVuMeterWidget {
    pub fn new<L1, L2>(cx: &mut Context, peak_l: L1, peak_r: L2) -> Handle<'_, Self>
    where
        L1: Lens<Target = Arc<AtomicF32>>,
        L2: Lens<Target = Arc<AtomicF32>>,
    {
        Self {
            peak_l: peak_l.get(cx),
            peak_r: peak_r.get(cx),
        }
        .build(cx, |_| {})
    }
}

impl View for StereoVuMeterWidget {
    fn element(&self) -> Option<&'static str> {
        Some("stereo-vu-meter-widget")
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &mut Canvas) {
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

        let draw_bar = |canvas: &mut Canvas, by: f32, norm: f32| {
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
