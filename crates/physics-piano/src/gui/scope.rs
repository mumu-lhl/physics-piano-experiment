//! Real-time Lissajous Scope and Stereo Peak VU Meter Widgets for Vizia.

use super::skia_compat as vg;
use super::skia_compat::CanvasExt;
use std::sync::Arc;
use vizia_plug::vizia::prelude::*;

pub struct LissajousScopeWidget {
    orbit_t: Arc<parking_lot::RwLock<Vec<f32>>>,
    orbit_p: Arc<parking_lot::RwLock<Vec<f32>>>,
}

impl LissajousScopeWidget {
    pub fn new(
        cx: &mut Context,
        orbit_t: Arc<parking_lot::RwLock<Vec<f32>>>,
        orbit_p: Arc<parking_lot::RwLock<Vec<f32>>>,
    ) -> Handle<'_, Self> {
        Self { orbit_t, orbit_p }.build(cx, |_| {})
    }
}

impl View for LissajousScopeWidget {
    fn element(&self) -> Option<&'static str> {
        Some("lissajous-scope-widget")
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
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
        let grid_paint =
            vg::Paint::color(vg::Color::rgbaf(0.3, 0.4, 0.55, 0.25)).with_line_width(0.8);
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

            let orbit_paint =
                vg::Paint::color(vg::Color::rgbaf(1.0, 0.82, 0.35, 0.88)).with_line_width(1.2);
            canvas.stroke_path(&orbit_path, &orbit_paint);
        }
    }
}
