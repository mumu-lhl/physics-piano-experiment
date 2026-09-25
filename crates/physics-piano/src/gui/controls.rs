//! Controls, Voicing Sliders, and Real-Time Acoustic Visualizers in egui.

use nih_plug_egui::egui::{
    self, Color32, Pos2, Rect, Stroke, Ui, Vec2,
};

pub fn render_vu_meter(ui: &mut Ui, peak_left: f32, peak_right: f32, height: f32) {
    let width = 16.0;
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width * 2.0 + 4.0, height), egui::Sense::hover());
    let painter = ui.painter_at(rect);

    painter.rect_filled(rect, 2.0, Color32::from_rgb(25, 25, 30));

    let draw_channel = |x_offset: f32, peak: f32| {
        let db = 20.0 * peak.max(1e-4).log10();
        let norm = ((db + 48.0) / 48.0).clamp(0.0, 1.0);
        let bar_h = height * norm;
        let bar_rect = Rect::from_min_max(
            Pos2::new(rect.min.x + x_offset, rect.max.y - bar_h),
            Pos2::new(rect.min.x + x_offset + width, rect.max.y),
        );
        let color = if norm > 0.85 {
            Color32::from_rgb(255, 70, 70)
        } else if norm > 0.65 {
            Color32::from_rgb(255, 200, 50)
        } else {
            Color32::from_rgb(60, 200, 100)
        };
        painter.rect_filled(bar_rect, 1.0, color);
    };

    draw_channel(0.0, peak_left);
    draw_channel(width + 4.0, peak_right);
}

pub fn render_lissajous_scope(ui: &mut Ui, u_t: &[f32], u_p: &[f32], size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(size, size), egui::Sense::hover());
    let painter = ui.painter_at(rect);

    painter.rect_filled(rect, 4.0, Color32::from_rgb(18, 20, 26));
    painter.rect_stroke(rect, 4.0, Stroke::new(1.0_f32, Color32::from_rgb(45, 50, 65)), egui::StrokeKind::Inside);

    let center = rect.center();
    let radius = size * 0.45;

    // Crosshairs
    painter.line_segment(
        [Pos2::new(rect.min.x + 8.0, center.y), Pos2::new(rect.max.x - 8.0, center.y)],
        Stroke::new(1.0_f32, Color32::from_rgb(35, 40, 50)),
    );
    painter.line_segment(
        [Pos2::new(center.x, rect.min.y + 8.0), Pos2::new(center.x, rect.max.y - 8.0)],
        Stroke::new(1.0_f32, Color32::from_rgb(35, 40, 50)),
    );

    let len = u_t.len().min(u_p.len());
    if len > 1 {
        let mut points = Vec::with_capacity(len);
        for i in 0..len {
            let x = center.x + u_p[i] * radius;
            let y = center.y - u_t[i] * radius;
            points.push(Pos2::new(x, y));
        }
        for i in 0..(points.len() - 1) {
            painter.line_segment(
                [points[i], points[i + 1]],
                Stroke::new(1.5_f32, Color32::from_rgb(80, 200, 255)),
            );
        }
    }
}
