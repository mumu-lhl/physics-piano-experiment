//! Interactive five-string bass fretboard rendered with Vizia/Skia.
//!
//! The widget is deliberately independent of the audio engine: mouse gestures
//! enqueue tiny, copy-only events and the audio thread consumes them with
//! `try_recv()`.  The vibration glow is read from atomics written after each
//! processed audio block.

use crate::nice_plugin::{GuiBassEvent, PhysicsBassParams};
use physics_ui::skia_compat as vg;
use physics_ui::skia_compat::CanvasExt;
use physics_ui::{Language, translate};
use std::sync::Arc;
use std::sync::atomic::{AtomicI32, AtomicU8, AtomicU32, Ordering};
use vizia_plug::vizia::prelude::*;

const STRING_COUNT: usize = 5;
const FRETS: u8 = 24;
const STRING_NAMES: [&str; STRING_COUNT] = ["B0", "E1", "A1", "D2", "G2"];

pub struct BassFretboardWidget {
    params: Arc<PhysicsBassParams>,
    active_frets: Arc<[AtomicU8; STRING_COUNT]>,
    string_energies: Arc<[AtomicU32; STRING_COUNT]>,
    string_profiles: Arc<[[AtomicI32; 32]; STRING_COUNT]>,
    gui_tx: crossbeam_channel::Sender<GuiBassEvent>,
    language: Arc<AtomicU8>,
    held: Option<(usize, u8)>,
}

impl Drop for BassFretboardWidget {
    fn drop(&mut self) {
        if let Some((string, _fret)) = self.held.take() {
            let _ = self.gui_tx.try_send(GuiBassEvent::NoteOff {
                string_index: string as u8,
            });
        }
    }
}

impl BassFretboardWidget {
    pub fn new(
        cx: &mut Context,
        params: Arc<PhysicsBassParams>,
        active_frets: Arc<[AtomicU8; STRING_COUNT]>,
        string_energies: Arc<[AtomicU32; STRING_COUNT]>,
        string_profiles: Arc<[[AtomicI32; 32]; STRING_COUNT]>,
        gui_tx: crossbeam_channel::Sender<GuiBassEvent>,
        language: Arc<AtomicU8>,
    ) -> Handle<'_, Self> {
        Self {
            params,
            active_frets,
            string_energies,
            string_profiles,
            gui_tx,
            language,
            held: None,
        }
        .build(cx, |_| {})
    }

    fn board_geometry(bounds: &BoundingBox) -> (f32, f32, f32, f32) {
        let label_width = 42.0;
        let nut_width = 18.0;
        let board_x = bounds.x + label_width;
        let board_width = (bounds.w - label_width - 8.0).max(80.0);
        let fret_span = (board_width - nut_width - 8.0).max(30.0) / 0.75;
        (board_x, nut_width, fret_span, board_width)
    }

    fn fret_at_x(&self, bounds: &BoundingBox, mouse_x: f32) -> Option<u8> {
        let (board_x, nut_width, fret_span, board_width) = Self::board_geometry(bounds);
        if mouse_x < board_x || mouse_x > board_x + board_width {
            return None;
        }
        let rel_x = mouse_x - board_x;
        if rel_x < nut_width {
            return Some(0);
        }
        for fret in 1..=FRETS {
            let left = nut_width + fret_span * (1.0 - 2.0_f32.powf(-((fret - 1) as f32) / 12.0));
            let right = nut_width + fret_span * (1.0 - 2.0_f32.powf(-(fret as f32) / 12.0));
            if rel_x >= left && rel_x < right {
                return Some(fret);
            }
        }
        Some(FRETS)
    }

    fn target_at(&self, bounds: &BoundingBox, mouse_x: f32, mouse_y: f32) -> Option<(usize, u8, f64)> {
        if mouse_x < bounds.x
            || mouse_x > bounds.x + bounds.w
            || mouse_y < bounds.y
            || mouse_y > bounds.y + bounds.h
        {
            return None;
        }
        let row_height = bounds.h / STRING_COUNT as f32;
        let string = ((mouse_y - bounds.y) / row_height) as usize;
        let string = string.min(STRING_COUNT - 1);
        if string == 0 && !self.params.five_string.value() {
            return None;
        }
        let string_center_y = bounds.y + row_height * (string as f32 + 0.5);
        let norm_dist = (mouse_y - string_center_y).abs() / (0.5 * row_height);
        let velocity = (0.98 - 0.50 * norm_dist.clamp(0.0, 1.0)).clamp(0.35, 1.0) as f64;

        self.fret_at_x(bounds, mouse_x).map(|fret| (string, fret, velocity))
    }

    fn emit_note_on(&self, target: (usize, u8, f64)) {
        let _ = self.gui_tx.try_send(GuiBassEvent::NoteOn {
            string_index: target.0 as u8,
            fret: target.1,
            velocity: target.2,
        });
    }

    fn emit_note_off(&self, target: (usize, u8)) {
        let _ = self.gui_tx.try_send(GuiBassEvent::NoteOff {
            string_index: target.0 as u8,
        });
    }

    fn release_held(&mut self, cx: &mut EventContext) {
        if let Some(target) = self.held.take() {
            self.emit_note_off(target);
            cx.release();
            cx.needs_redraw();
        }
    }
}

impl View for BassFretboardWidget {
    fn element(&self) -> Option<&'static str> {
        Some("bass-fretboard-widget")
    }

    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|window_event, meta| match window_event {
            WindowEvent::MouseDown(MouseButton::Left) => {
                let bounds = cx.bounds();
                let target = self.target_at(&bounds, cx.mouse().cursor_x, cx.mouse().cursor_y);
                if let Some((str_idx, fret, vel)) = target {
                    if let Some(previous) = self.held.replace((str_idx, fret)) {
                        self.emit_note_off(previous);
                    }
                    self.emit_note_on((str_idx, fret, vel));
                    cx.capture();
                    cx.needs_redraw();
                    meta.consume();
                }
            }
            WindowEvent::MouseMove(x, y) if self.held.is_some() => {
                let bounds = cx.bounds();
                if let Some((str_idx, fret, vel)) = self.target_at(&bounds, *x, *y) {
                    if self.held != Some((str_idx, fret)) {
                        if let Some(previous) = self.held.replace((str_idx, fret)) {
                            self.emit_note_off(previous);
                        }
                        self.emit_note_on((str_idx, fret, vel));
                        cx.needs_redraw();
                    }
                } else {
                    // Releasing outside the widget must not leave a stuck note.
                    self.release_held(cx);
                }
                meta.consume();
            }
            WindowEvent::MouseUp(MouseButton::Left) => {
                if self.held.is_some() {
                    self.release_held(cx);
                    meta.consume();
                }
            }
            WindowEvent::FocusOut if self.held.is_some() => self.release_held(cx),
            _ => {}
        });
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = cx.bounds();
        if bounds.w < 160.0 || bounds.h < 80.0 {
            return;
        }
        let (board_x, nut_width, fret_span, board_width) = Self::board_geometry(&bounds);
        let row_height = bounds.h / STRING_COUNT as f32;
        let five_string = self.params.five_string.value();
        let language = Language::from_index(self.language.load(Ordering::Relaxed));

        let mut background = vg::Path::new();
        background.rounded_rect(bounds.x, bounds.y, bounds.w, bounds.h, 8.0);
        canvas.fill_path(&background, &vg::Paint::color(vg::Color::rgb(16, 19, 25)));

        let mut board = vg::Path::new();
        board.rounded_rect(board_x, bounds.y + 2.0, board_width, bounds.h - 4.0, 6.0);
        canvas.fill_path(&board, &vg::Paint::color(vg::Color::rgb(42, 29, 25)));
        let mut board_border = vg::Paint::color(vg::Color::rgb(91, 66, 50));
        board_border.set_line_width(1.0);
        canvas.stroke_path(&board, &board_border);

        let mut nut = vg::Path::new();
        nut.rounded_rect(board_x, bounds.y + 2.0, nut_width, bounds.h - 4.0, 3.0);
        canvas.fill_path(&nut, &vg::Paint::color(vg::Color::rgb(224, 215, 190)));

        let fret_paint = vg::Paint::color(vg::Color::rgb(129, 137, 150));
        for fret in 1..=FRETS {
            let x = board_x + nut_width + fret_span * (1.0 - 2.0_f32.powf(-(fret as f32) / 12.0));
            let mut line = vg::Path::new();
            line.move_to(x, bounds.y + 2.0);
            line.line_to(x, bounds.y + bounds.h - 2.0);
            canvas.stroke_path(&line, &fret_paint);

            if matches!(fret, 3 | 5 | 7 | 9 | 15 | 17 | 19 | 21 | 12 | 24) {
                let left = if fret == 1 {
                    board_x + nut_width
                } else {
                    board_x
                        + nut_width
                        + fret_span * (1.0 - 2.0_f32.powf(-((fret - 1) as f32) / 12.0))
                };
                let dot_x = (left + x) * 0.5;
                let mut dot = vg::Path::new();
                if fret == 12 || fret == 24 {
                    dot.circle(dot_x, bounds.y + bounds.h * 0.36, 3.0);
                    dot.circle(dot_x, bounds.y + bounds.h * 0.64, 3.0);
                } else {
                    dot.circle(dot_x, bounds.y + bounds.h * 0.5, 3.0);
                }
                canvas.fill_path(&dot, &vg::Paint::color(vg::Color::rgb(168, 143, 111)));
            }
        }

        let mut hint_paint = vg::Paint::color(vg::Color::rgb(110, 120, 137));
        hint_paint.set_font_size(9.0);
        hint_paint.set_text_align(vg::Align::Right);
        canvas.fill_text(
            bounds.x + 34.0,
            bounds.y + 14.0,
            if five_string {
                translate(language, "bass.fretboard.five-string", "5-STRING")
            } else {
                translate(language, "bass.fretboard.four-string", "4-STRING")
            },
            &hint_paint,
        );

        let gauges = [4.0, 3.1, 2.5, 1.9, 1.5];
        for string in 0..STRING_COUNT {
            let y = bounds.y + row_height * (string as f32 + 0.5);
            let enabled = string != 0 || five_string;
            let energy = f32::from_bits(self.string_energies[string].load(Ordering::Relaxed))
                .clamp(0.0, 1.0);
            let active = self.active_frets[string].load(Ordering::Relaxed);
            let fret = self
                .held
                .filter(|(held_string, _)| *held_string == string)
                .map(|(_, fret)| fret)
                .unwrap_or(active);

            let mut shadow = vg::Path::new();
            shadow.move_to(board_x, y);
            shadow.line_to(board_x + board_width, y);
            let mut shadow_paint = vg::Paint::color(vg::Color::rgba(0, 0, 0, 120));
            shadow_paint.set_line_width(gauges[string] + 4.0 * energy);
            canvas.stroke_path(&shadow, &shadow_paint);

            let is_vibrating = enabled && energy > 0.005;
            let string_color = if !enabled {
                vg::Color::rgb(76, 78, 84)
            } else if is_vibrating {
                vg::Color::rgb(255, 193, 87)
            } else {
                vg::Color::rgb(201, 208, 220)
            };

            // Calculate active speaking string start x
            let start_x = if fret > 0 && fret <= FRETS {
                board_x + nut_width + fret_span * (1.0 - 2.0_f32.powf(-(fret as f32) / 12.0))
            } else {
                board_x + nut_width
            };
            let end_x = board_x + board_width;

            // Draw muted portion of string before fret as straight wire
            if fret > 0 && fret <= FRETS {
                let mut quiet_wire = vg::Path::new();
                quiet_wire.move_to(board_x, y);
                quiet_wire.line_to(start_x, y);
                let mut wire_paint = vg::Paint::color(vg::Color::rgb(140, 145, 155));
                wire_paint.set_line_width(gauges[string]);
                canvas.stroke_path(&quiet_wire, &wire_paint);
            }

            // Draw vibrating physical FDTD wave curve
            let mut string_path = vg::Path::new();
            string_path.move_to(start_x, y);
            if is_vibrating {
                let speaking_length = end_x - start_x;
                let amp_scale = (energy.sqrt() * 16.0).min(row_height * 0.45);
                const NUM_PTS: usize = 32;
                for k in 0..NUM_PTS {
                    let frac = (k as f32 + 1.0) / (NUM_PTS as f32 + 1.0);
                    let px = start_x + speaking_length * frac;
                    let profile_raw = self.string_profiles[string][k].load(Ordering::Relaxed);
                    let profile_disp = f32::from_bits(profile_raw as u32);
                    let py = y + profile_disp.clamp(-1.0, 1.0) * amp_scale;
                    string_path.line_to(px, py);
                }
                string_path.line_to(end_x, y);
            } else {
                string_path.line_to(end_x, y);
            }

            if is_vibrating {
                let mut glow = vg::Paint::color(vg::Color::rgba(
                    255,
                    175,
                    45,
                    (85.0 * energy).min(90.0) as u8,
                ));
                glow.set_line_width(gauges[string] + 6.0 * energy);
                canvas.stroke_path(&string_path, &glow);
            }

            let mut string_paint = vg::Paint::color(string_color);
            string_paint.set_line_width(gauges[string]);
            canvas.stroke_path(&string_path, &string_paint);

            let mut name_paint = if enabled {
                vg::Paint::color(vg::Color::rgb(213, 218, 229))
            } else {
                vg::Paint::color(vg::Color::rgb(88, 91, 99))
            };
            name_paint.set_font_size(11.0);
            name_paint.set_text_align(vg::Align::Right);
            canvas.fill_text(bounds.x + 34.0, y + 4.0, STRING_NAMES[string], &name_paint);

            if enabled && fret <= FRETS {
                let dot_x = if fret == 0 {
                    board_x + nut_width * 0.5
                } else {
                    let left = board_x
                        + nut_width
                        + fret_span * (1.0 - 2.0_f32.powf(-((fret - 1) as f32) / 12.0));
                    let right = board_x
                        + nut_width
                        + fret_span * (1.0 - 2.0_f32.powf(-(fret as f32) / 12.0));
                    (left + right) * 0.5
                };
                let mut marker = vg::Path::new();
                marker.circle(dot_x, y, 6.0 + 3.0 * energy);
                canvas.fill_path(
                    &marker,
                    &vg::Paint::color(vg::Color::rgba(255, 161, 62, 230)),
                );
            }
        }

        let mut footer = vg::Paint::color(vg::Color::rgb(121, 129, 145));
        footer.set_font_size(10.0);
        footer.set_text_align(vg::Align::Left);
        canvas.fill_text(
            bounds.x + 8.0,
            bounds.y + bounds.h - 8.0,
            translate(
                language,
                "bass.fretboard.hint",
                "click or drag strings to play · open notes + 24 frets",
            ),
            &footer,
        );
    }
}
