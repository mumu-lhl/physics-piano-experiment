//! Interactive 88-Key Virtual Piano Keyboard Widget for egui.

use nih_plug_egui::egui::{
    self, Color32, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2,
};

pub struct PianoKeyboardWidget<'a> {
    pub is_key_active: &'a dyn Fn(u8) -> bool,
    pub held_mouse_key: &'a mut Option<u8>,
    pub pressed_keys: Vec<(u8, f32)>, // (midi_key, velocity) triggered this frame
    pub released_keys: Vec<u8>,       // midi_key released this frame
}

impl<'a> PianoKeyboardWidget<'a> {
    pub fn new(is_key_active: &'a dyn Fn(u8) -> bool, held_mouse_key: &'a mut Option<u8>) -> Self {
        Self {
            is_key_active,
            held_mouse_key,
            pressed_keys: Vec::new(),
            released_keys: Vec::new(),
        }
    }

    /// Renders the 88-key piano keyboard and captures mouse interactions.
    pub fn show(&mut self, ui: &mut Ui, width: f32, height: f32) -> Response {
        let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click_and_drag());
        let painter = ui.painter_at(rect);

        // 88 keys range from MIDI 21 (A0) to 108 (C8).
        // Total white keys: 52.
        let num_white_keys = 52usize;
        let white_w = width / num_white_keys as f32;
        let white_h = height;
        let black_w = white_w * 0.64;
        let black_h = height * 0.62;

        let is_black = |midi: u8| -> bool {
            matches!(midi % 12, 1 | 3 | 6 | 8 | 10)
        };

        // Precompute geometry of white keys
        let mut white_key_rects = Vec::with_capacity(52);
        let mut white_key_midis = Vec::with_capacity(52);
        let mut black_key_rects = Vec::with_capacity(36);
        let mut black_key_midis = Vec::with_capacity(36);

        let mut white_idx = 0;
        for midi in 21..=108 {
            if !is_black(midi) {
                let x0 = rect.min.x + white_idx as f32 * white_w;
                let r = Rect::from_min_size(Pos2::new(x0, rect.min.y), Vec2::new(white_w, white_h));
                white_key_rects.push(r);
                white_key_midis.push(midi);
                white_idx += 1;
            }
        }

        // Precompute geometry of black keys based on white key boundaries
        let mut curr_white = 0;
        for midi in 21..=108 {
            if is_black(midi) {
                // Black key sits between (curr_white - 1) and curr_white
                let boundary_x = rect.min.x + curr_white as f32 * white_w;
                let r = Rect::from_min_size(
                    Pos2::new(boundary_x - black_w * 0.5, rect.min.y),
                    Vec2::new(black_w, black_h),
                );
                black_key_rects.push(r);
                black_key_midis.push(midi);
            } else {
                curr_white += 1;
            }
        }

        let is_primary_down = ui.input(|i| i.pointer.primary_down());
        let is_primary_released = ui.input(|i| i.pointer.primary_released());
        let pointer_pos = ui.input(|i| i.pointer.latest_pos());

        let mut hovered_key = None;
        let mut click_vel = 0.8f32;

        if let Some(mouse_pos) = pointer_pos {
            if rect.contains(mouse_pos) {
                // Check black keys first (top layer)
                for (i, r) in black_key_rects.iter().enumerate() {
                    if r.contains(mouse_pos) {
                        hovered_key = Some(black_key_midis[i]);
                        let rel_y = (mouse_pos.y - r.min.y) / r.height();
                        click_vel = (0.3 + 0.68 * rel_y).clamp(0.2, 1.0);
                        break;
                    }
                }
                // If not black key, check white keys
                if hovered_key.is_none() {
                    for (i, r) in white_key_rects.iter().enumerate() {
                        if r.contains(mouse_pos) {
                            hovered_key = Some(white_key_midis[i]);
                            let rel_y = (mouse_pos.y - r.min.y) / r.height();
                            click_vel = (0.3 + 0.68 * rel_y).clamp(0.2, 1.0);
                            break;
                        }
                    }
                }
            }
        }

        // Target key to hold down: primary button actively down, not released this frame, pointer in rect
        let is_in_keyboard = pointer_pos.map_or(false, |pos| rect.contains(pos));
        let target_held = if is_primary_down && !is_primary_released && is_in_keyboard {
            hovered_key
        } else {
            None
        };

        if *self.held_mouse_key != target_held {
            if let Some(old_k) = self.held_mouse_key.take() {
                self.released_keys.push(old_k);
            }
            if let Some(new_k) = target_held {
                self.pressed_keys.push((new_k, click_vel));
                *self.held_mouse_key = Some(new_k);
            }
        } else if is_primary_released {
            if let Some(old_k) = self.held_mouse_key.take() {
                self.released_keys.push(old_k);
            }
        }

        let currently_held = *self.held_mouse_key;

        // Draw White Keys
        for (i, r) in white_key_rects.iter().enumerate() {
            let midi = white_key_midis[i];
            let is_active = (self.is_key_active)(midi) || currently_held == Some(midi);

            let fill = if is_active {
                Color32::from_rgb(255, 215, 110) // Warm active glow
            } else if hovered_key == Some(midi) {
                Color32::from_rgb(240, 243, 248)
            } else {
                Color32::from_rgb(250, 248, 245) // Natural ivory
            };

            painter.rect_filled(*r, 3.0, fill);
            painter.rect_stroke(
                *r,
                3.0,
                Stroke::new(1.0_f32, Color32::from_rgb(70, 70, 75)),
                egui::StrokeKind::Inside,
            );

            // Draw note label on middle C (C4) and octave marks
            if midi % 12 == 0 {
                let oct = (midi as i32 / 12) - 1;
                painter.text(
                    Pos2::new(r.center().x, r.max.y - 12.0),
                    egui::Align2::CENTER_CENTER,
                    format!("C{}", oct),
                    egui::FontId::proportional(10.0),
                    Color32::from_rgb(120, 120, 130),
                );
            }
        }

        // Draw Black Keys on top
        for (i, r) in black_key_rects.iter().enumerate() {
            let midi = black_key_midis[i];
            let is_active = (self.is_key_active)(midi) || currently_held == Some(midi);

            let fill = if is_active {
                Color32::from_rgb(230, 160, 40) // Amber active glow
            } else if hovered_key == Some(midi) {
                Color32::from_rgb(50, 50, 58)
            } else {
                Color32::from_rgb(22, 22, 26) // Deep ebony
            };

            painter.rect_filled(*r, 2.5, fill);
            painter.rect_stroke(
                *r,
                2.5,
                Stroke::new(1.0_f32, Color32::from_rgb(15, 15, 18)),
                egui::StrokeKind::Inside,
            );
        }

        response
    }
}
