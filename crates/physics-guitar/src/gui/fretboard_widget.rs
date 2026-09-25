//! Interactive 6-string guitar fretboard widget with realistic fret spacing,
//! inlays, string thickness, and active note illumination.

use nih_plug_egui::egui::{
    Color32, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2,
};

pub struct GuitarFretboardWidget<'a> {
    /// Active fret for each string [String 1..=6], None if string is idle
    pub active_frets: &'a [Option<u8>; 6],
    /// Energy levels for each string [String 1..=6] in range [0.0, 1.0]
    pub string_energies: &'a [f32; 6],
    /// Callback when user clicks a string and fret: (string_index 1..=6, fret 0..=24)
    pub on_fret_clicked: Option<&'a mut dyn FnMut(u8, u8)>,
}

impl<'a> GuitarFretboardWidget<'a> {
    pub fn new(
        active_frets: &'a [Option<u8>; 6],
        string_energies: &'a [f32; 6],
    ) -> Self {
        Self {
            active_frets,
            string_energies,
            on_fret_clicked: None,
        }
    }

    pub fn with_callback(
        mut self,
        callback: &'a mut dyn FnMut(u8, u8),
    ) -> Self {
        self.on_fret_clicked = Some(callback);
        self
    }

    pub fn show(mut self, ui: &mut Ui, desired_size: Vec2) -> Response {
        let (rect, mut response) = ui.allocate_exact_size(desired_size, Sense::click_and_drag());
        let painter = ui.painter_at(rect);

        // 1. Draw Rosewood / Ebony Fretboard Background
        let fretboard_color = Color32::from_rgb(38, 28, 24); // Dark rosewood
        painter.rect_filled(rect, 4.0, fretboard_color);

        let nut_width = 12.0;
        let nut_rect = Rect::from_min_size(rect.min, Vec2::new(nut_width, rect.height()));
        // Bone nut
        painter.rect_filled(nut_rect, 2.0, Color32::from_rgb(235, 230, 215));

        let playable_width = rect.width() - nut_width - 8.0;
        let fret_count = 24;

        // Fret x positions based on 1 - 2^(-k / 17) scale visual compression
        let get_fret_x = |k: u8| -> f32 {
            if k == 0 {
                rect.min.x + nut_width
            } else {
                let frac = 1.0 - 2.0f32.powf(-(k as f32) / 16.5);
                rect.min.x + nut_width + frac * playable_width * 1.58
            }
        };

        // 2. Draw Inlay Position Markers (Dots)
        let single_dot_frets = [3, 5, 7, 9, 15, 17, 19, 21];
        let mid_y = rect.center().y;
        let dot_color = Color32::from_rgb(215, 215, 220); // Pearl

        for &f in &single_dot_frets {
            let x1 = get_fret_x(f - 1);
            let x2 = get_fret_x(f);
            let cx = (x1 + x2) * 0.5;
            painter.circle_filled(Pos2::new(cx, mid_y), 4.5, dot_color);
        }

        // Double dots at 12th and 24th frets
        for &f in &[12, 24] {
            let x1 = get_fret_x(f - 1);
            let x2 = get_fret_x(f);
            let cx = (x1 + x2) * 0.5;
            painter.circle_filled(Pos2::new(cx, mid_y - rect.height() * 0.22), 4.0, dot_color);
            painter.circle_filled(Pos2::new(cx, mid_y + rect.height() * 0.22), 4.0, dot_color);
        }

        // 3. Draw Nickel/Silver Metal Frets
        let fret_stroke = Stroke::new(2.0_f32, Color32::from_rgb(180, 185, 195));
        for f in 1..=fret_count {
            let fx = get_fret_x(f);
            if fx < rect.max.x {
                painter.line_segment(
                    [Pos2::new(fx, rect.min.y), Pos2::new(fx, rect.max.y)],
                    fret_stroke,
                );
            }
        }

        // 4. Draw the 6 Strings (String 1 high E at top, String 6 low E at bottom)
        let string_y_spacing = rect.height() / 7.0;
        let string_gauges = [1.2_f32, 1.6_f32, 2.0_f32, 2.6_f32, 3.2_f32, 3.8_f32]; // visual thicknesses

        for (i, &thickness) in string_gauges.iter().enumerate() {
            let y = rect.min.y + string_y_spacing * (i + 1) as f32;
            let str_idx = i; // 0 = string 1, 5 = string 6
            let energy = self.string_energies[str_idx];

            let string_color = if str_idx >= 3 {
                // Wound copper/bronze strings (Strings 4, 5, 6)
                Color32::from_rgb(195, 155, 110)
            } else {
                // Plain steel strings (Strings 1, 2, 3)
                Color32::from_rgb(210, 215, 225)
            };

            // String vibration glow if energetic
            if energy > 0.05 {
                let glow_alpha = (energy * 180.0).clamp(0.0, 180.0) as u8;
                let glow_stroke = Stroke::new(thickness + 3.0_f32, Color32::from_rgba_unmultiplied(255, 210, 120, glow_alpha));
                painter.line_segment([Pos2::new(rect.min.x, y), Pos2::new(rect.max.x, y)], glow_stroke);
            }

            painter.line_segment(
                [Pos2::new(rect.min.x, y), Pos2::new(rect.max.x, y)],
                Stroke::new(thickness, string_color),
            );

            // 5. Draw Active Pressed Note Indicator on Fretboard
            if let Some(fret) = self.active_frets[str_idx] {
                let note_x = if fret == 0 {
                    rect.min.x + nut_width * 0.5
                } else {
                    let x_prev = get_fret_x(fret - 1);
                    let x_curr = get_fret_x(fret);
                    (x_prev + x_curr) * 0.5
                };

                let radius = 6.0;
                let note_pos = Pos2::new(note_x, y);
                // Glowing illuminated fingered note
                painter.circle_filled(note_pos, radius + 2.0, Color32::from_rgba_unmultiplied(255, 180, 50, 220));
                painter.circle_filled(note_pos, radius, Color32::from_rgb(255, 240, 190));
            }
        }

        // 6. Handle Mouse Click / Touch Input to trigger notes
        if response.clicked() || (response.dragged() && ui.input(|i| i.pointer.primary_down())) {
            if let Some(click_pos) = response.interact_pointer_pos() {
                if rect.contains(click_pos) {
                    // Determine which string was clicked (1..=6)
                    let rel_y = click_pos.y - rect.min.y;
                    let str_clicked = ((rel_y / string_y_spacing).round() as u8).clamp(1, 6);

                    // Determine which fret was clicked (0..=24)
                    let mut fret_clicked = 0;
                    for f in 1..=fret_count {
                        if click_pos.x <= get_fret_x(f) {
                            fret_clicked = f;
                            break;
                        }
                    }

                    if let Some(ref mut cb) = self.on_fret_clicked {
                        cb(str_clicked, fret_clicked);
                    }
                    response.mark_changed();
                }
            }
        }

        response
    }
}
