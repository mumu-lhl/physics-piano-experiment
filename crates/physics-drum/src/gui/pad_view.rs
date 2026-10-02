//! Interactive drum-kit pad view with realistic physical components,
//! radial sweet-spot strike dynamics, and computer keyboard performance support.

use crate::nice_plugin::GuiDrumEvent;
use physics_ui::skia_compat as vg;
use physics_ui::skia_compat::CanvasExt;
use physics_ui::{Language, translate};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU32, Ordering};
use vizia_plug::vizia::prelude::*;
use vizia_plug::widgets::util::ModifiersExt;

const VOICE_COUNT: usize = 10;

#[derive(Clone, Copy, PartialEq, Eq)]
enum PadKind {
    Drum,
    Cymbal,
    Pedal,
}

#[derive(Clone, Copy)]
struct Pad {
    note: u8,
    label: &'static str,
    hotkey: &'static str,
    x: f32,
    y: f32,
    radius: f32,
    kind: PadKind,
    color: (u8, u8, u8),
}

const PADS: [Pad; VOICE_COUNT] = [
    // 0: Crash Cymbal (Note 49)
    Pad {
        note: 49,
        label: "Crash Cymbal",
        hotkey: "E",
        x: 0.13,
        y: 0.20,
        radius: 0.105,
        kind: PadKind::Cymbal,
        color: (224, 157, 69),
    },
    // 1: Ride Cymbal (Note 51)
    Pad {
        note: 51,
        label: "Ride Cymbal",
        hotkey: "I",
        x: 0.87,
        y: 0.20,
        radius: 0.110,
        kind: PadKind::Cymbal,
        color: (210, 148, 62),
    },
    // 2: Open Hi-Hat (Note 46)
    Pad {
        note: 46,
        label: "Open Hi-Hat",
        hotkey: "G",
        x: 0.22,
        y: 0.46,
        radius: 0.088,
        kind: PadKind::Cymbal,
        color: (218, 178, 88),
    },
    // 3: Closed Hi-Hat (Note 42)
    Pad {
        note: 42,
        label: "Closed Hi-Hat",
        hotkey: "F",
        x: 0.22,
        y: 0.70,
        radius: 0.084,
        kind: PadKind::Cymbal,
        color: (180, 150, 78),
    },
    // 4: High Tom (Note 50) - 10" Rack Tom 1
    Pad {
        note: 50,
        label: "High Tom",
        hotkey: "J",
        x: 0.44,
        y: 0.28,
        radius: 0.095,
        kind: PadKind::Drum,
        color: (158, 92, 70),
    },
    // 5: Mid Tom (Note 45) - 12" Rack Tom 2
    Pad {
        note: 45,
        label: "Mid Tom",
        hotkey: "K",
        x: 0.60,
        y: 0.28,
        radius: 0.105,
        kind: PadKind::Drum,
        color: (165, 88, 66),
    },
    // 6: Floor Tom (Note 41) - 16" Floor Tom
    Pad {
        note: 41,
        label: "Floor Tom",
        hotkey: "L",
        x: 0.78,
        y: 0.55,
        radius: 0.125,
        kind: PadKind::Drum,
        color: (138, 72, 60),
    },
    // 7: Acoustic Snare (Note 38) - 14" Snare
    Pad {
        note: 38,
        label: "Acoustic Snare",
        hotkey: "D",
        x: 0.39,
        y: 0.57,
        radius: 0.110,
        kind: PadKind::Drum,
        color: (120, 146, 172),
    },
    // 8: Bass Drum (Note 36) - 22" Kick
    Pad {
        note: 36,
        label: "Bass Drum",
        hotkey: "B",
        x: 0.58,
        y: 0.74,
        radius: 0.145,
        kind: PadKind::Drum,
        color: (88, 108, 138),
    },
    // 9: Pedal Hi-Hat (Note 44)
    Pad {
        note: 44,
        label: "Pedal Hi-Hat",
        hotkey: "C",
        x: 0.12,
        y: 0.88,
        radius: 0.065,
        kind: PadKind::Pedal,
        color: (112, 126, 146),
    },
];

fn voice_index(note: u8) -> usize {
    PADS.iter().position(|pad| pad.note == note).unwrap_or(0)
}

pub struct DrumPadWidget {
    voice_energies: Arc<[AtomicU32; VOICE_COUNT]>,
    gui_tx: crossbeam_channel::Sender<GuiDrumEvent>,
    language: Arc<AtomicU8>,
    held_note: Option<u8>,
}

impl DrumPadWidget {
    pub fn new(
        cx: &mut Context,
        voice_energies: Arc<[AtomicU32; VOICE_COUNT]>,
        gui_tx: crossbeam_channel::Sender<GuiDrumEvent>,
        language: Arc<AtomicU8>,
    ) -> Handle<'_, Self> {
        Self {
            voice_energies,
            gui_tx,
            language,
            held_note: None,
        }
        .build(cx, |_| {})
    }

    fn pad_at(&self, bounds: &BoundingBox, x: f32, y: f32) -> Option<(Pad, f32)> {
        for pad in PADS {
            let center_x = bounds.x + pad.x * bounds.w;
            let center_y = bounds.y + pad.y * bounds.h;
            let radius = pad.radius * bounds.h.min(bounds.w * 0.55);
            let dx = x - center_x;
            let dy = y - center_y;
            let dist_sq = dx * dx + dy * dy;
            if dist_sq <= radius * radius {
                let norm_dist = (dist_sq.sqrt() / radius.max(1.0)).clamp(0.0, 1.0);
                return Some((pad, norm_dist));
            }
        }
        None
    }

    fn hit(&self, pad: Pad, velocity: f32) {
        let _ = self.gui_tx.try_send(GuiDrumEvent::Hit {
            note: pad.note,
            velocity: velocity.clamp(0.15, 1.0) as f64,
        });
    }
}

impl View for DrumPadWidget {
    fn element(&self) -> Option<&'static str> {
        Some("drum-pad-widget")
    }

    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|window_event, meta| match window_event {
            WindowEvent::MouseDown(MouseButton::Left) => {
                let bounds = cx.bounds();
                if let Some((pad, norm_dist)) = self.pad_at(&bounds, cx.mouse().cursor_x, cx.mouse().cursor_y) {
                    // Radial sweet-spot velocity: center = 1.0 (accent strike), edge = 0.35 (ghost note/rim tap)
                    let velocity = (1.0 - 0.65 * norm_dist).clamp(0.35, 1.0);
                    self.hit(pad, velocity);
                    self.held_note = Some(pad.note);
                    cx.capture();
                    cx.needs_redraw();
                    meta.consume();
                }
            }
            WindowEvent::MouseMove(x, y) if self.held_note.is_some() => {
                let bounds = cx.bounds();
                let next = self.pad_at(&bounds, *x, *y).map(|(pad, _)| pad.note);
                if next != self.held_note {
                    if let Some(note) = next {
                        let pad = PADS[voice_index(note)];
                        self.hit(pad, 0.85);
                    }
                    self.held_note = next;
                    if next.is_none() {
                        cx.release();
                    }
                    cx.needs_redraw();
                }
                meta.consume();
            }
            WindowEvent::MouseUp(MouseButton::Left) => {
                if self.held_note.take().is_some() {
                    cx.release();
                    cx.needs_redraw();
                    meta.consume();
                }
            }
            WindowEvent::FocusOut if self.held_note.take().is_some() => {
                cx.release();
                cx.needs_redraw();
            }
            WindowEvent::KeyDown(code, _) => {
                if !cx.modifiers().command() && !cx.modifiers().alt() {
                    let note_opt = match code {
                        Code::KeyB | Code::Space => Some(36), // Kick
                        Code::KeyD | Code::KeyS => Some(38),  // Snare
                        Code::KeyF => Some(42),               // Closed Hat
                        Code::KeyG => Some(46),               // Open Hat
                        Code::KeyC => Some(44),               // Pedal Hat
                        Code::KeyJ => Some(50),               // High Tom
                        Code::KeyK => Some(45),               // Mid Tom
                        Code::KeyL => Some(41),               // Floor Tom
                        Code::KeyE | Code::KeyR => Some(49),  // Crash
                        Code::KeyU | Code::KeyI => Some(51),  // Ride
                        _ => None,
                    };
                    if let Some(note) = note_opt {
                        let pad = PADS[voice_index(note)];
                        self.hit(pad, 0.88);
                        self.held_note = Some(note);
                        cx.needs_redraw();
                        meta.consume();
                    }
                }
            }
            WindowEvent::KeyUp(code, _) => {
                let note_opt = match code {
                    Code::KeyB | Code::Space => Some(36),
                    Code::KeyD | Code::KeyS => Some(38),
                    Code::KeyF => Some(42),
                    Code::KeyG => Some(46),
                    Code::KeyC => Some(44),
                    Code::KeyJ => Some(50),
                    Code::KeyK => Some(45),
                    Code::KeyL => Some(41),
                    Code::KeyE | Code::KeyR => Some(49),
                    Code::KeyU | Code::KeyI => Some(51),
                    _ => None,
                };
                if let Some(note) = note_opt {
                    if self.held_note == Some(note) {
                        self.held_note = None;
                        cx.needs_redraw();
                        meta.consume();
                    }
                }
            }
            _ => {}
        });
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = cx.bounds();
        if bounds.w < 220.0 || bounds.h < 160.0 {
            return;
        }
        let mut background = vg::Path::new();
        background.rounded_rect(bounds.x, bounds.y, bounds.w, bounds.h, 8.0);
        canvas.fill_path(&background, &vg::Paint::color(vg::Color::rgb(13, 16, 22)));

        let language = Language::from_index(self.language.load(Ordering::Relaxed));
        let mut title = vg::Paint::color(vg::Color::rgb(136, 149, 172));
        title.set_font_size(10.0);
        title.set_text_align(vg::Align::Left);
        canvas.fill_text(
            bounds.x + 14.0,
            bounds.y + 18.0,
            translate(language, "drum.pad-map.title", "MIDI DRUM MAP"),
            &title,
        );

        for pad in PADS {
            let center_x = bounds.x + pad.x * bounds.w;
            let center_y = bounds.y + pad.y * bounds.h;
            let radius = pad.radius * bounds.h.min(bounds.w * 0.55);
            let energy =
                f32::from_bits(self.voice_energies[voice_index(pad.note)].load(Ordering::Relaxed))
                    .clamp(0.0, 1.0);
            let pressed = self.held_note == Some(pad.note);
            let glow_radius = radius + (if pressed { 10.0 } else { energy * 12.0 });

            if energy > 0.02 || pressed {
                let alpha = if pressed { 140 } else { (35.0 + energy * 125.0) as u8 };
                let mut glow = vg::Path::new();
                glow.circle(center_x, center_y, glow_radius);
                canvas.fill_path(
                    &glow,
                    &vg::Paint::color(vg::Color::rgba(
                        pad.color.0,
                        pad.color.1,
                        pad.color.2,
                        alpha,
                    )),
                );
            }

            match pad.kind {
                PadKind::Drum => {
                    // Outer Metal Hoop / Rim
                    let mut rim = vg::Path::new();
                    rim.circle(center_x, center_y, radius);
                    let rim_color = if pressed {
                        vg::Color::rgb(246, 206, 120)
                    } else {
                        vg::Color::rgb(48, 54, 65)
                    };
                    canvas.fill_path(&rim, &vg::Paint::color(rim_color));
                    let mut rim_edge = vg::Paint::color(if pressed {
                        vg::Color::rgb(255, 230, 160)
                    } else {
                        vg::Color::rgb(190, 200, 215)
                    });
                    rim_edge.set_line_width(2.0);
                    canvas.stroke_path(&rim, &rim_edge);

                    // Inner Drumhead Membrane
                    let head_radius = radius * 0.88;
                    let mut head = vg::Path::new();
                    head.circle(center_x, center_y, head_radius);
                    let head_fill = if pressed {
                        vg::Color::rgb(246, 196, 103)
                    } else {
                        vg::Color::rgb(
                            (pad.color.0 as f32 * (0.50 + energy * 0.50)) as u8,
                            (pad.color.1 as f32 * (0.50 + energy * 0.50)) as u8,
                            (pad.color.2 as f32 * (0.50 + energy * 0.50)) as u8,
                        )
                    };
                    canvas.fill_path(&head, &vg::Paint::color(head_fill));
                    let mut head_rim = vg::Paint::color(vg::Color::rgba(255, 255, 255, 45));
                    head_rim.set_line_width(1.0);
                    canvas.stroke_path(&head, &head_rim);

                    // Center Sweet-Spot Strike Target (Dotted accent zone)
                    let sweet_radius = head_radius * 0.38;
                    let mut sweet = vg::Path::new();
                    sweet.circle(center_x, center_y, sweet_radius);
                    let mut sweet_paint = vg::Paint::color(vg::Color::rgba(
                        255,
                        255,
                        255,
                        if pressed { 120 } else { 40 },
                    ));
                    sweet_paint.set_line_width(1.0);
                    canvas.stroke_path(&sweet, &sweet_paint);
                }
                PadKind::Cymbal => {
                    // Bronze Disc Body
                    let mut cymbal = vg::Path::new();
                    cymbal.circle(center_x, center_y, radius);
                    let cymbal_fill = if pressed {
                        vg::Color::rgb(246, 196, 103)
                    } else {
                        vg::Color::rgb(
                            (pad.color.0 as f32 * (0.50 + energy * 0.50)) as u8,
                            (pad.color.1 as f32 * (0.50 + energy * 0.50)) as u8,
                            (pad.color.2 as f32 * (0.50 + energy * 0.50)) as u8,
                        )
                    };
                    canvas.fill_path(&cymbal, &vg::Paint::color(cymbal_fill));
                    let mut cymbal_edge = vg::Paint::color(if pressed {
                        vg::Color::rgb(255, 235, 170)
                    } else {
                        vg::Color::rgb(245, 205, 125)
                    });
                    cymbal_edge.set_line_width(1.5);
                    canvas.stroke_path(&cymbal, &cymbal_edge);

                    // Lathe Grooves (Concentric sound rings)
                    for &ratio in &[0.72, 0.52] {
                        let mut lathe = vg::Path::new();
                        lathe.circle(center_x, center_y, radius * ratio);
                        let mut lathe_paint = vg::Paint::color(vg::Color::rgba(0, 0, 0, 35));
                        lathe_paint.set_line_width(1.0);
                        canvas.stroke_path(&lathe, &lathe_paint);
                    }

                    // Center Bell / Cup (Raised dome)
                    let bell_radius = radius * 0.28;
                    let mut bell = vg::Path::new();
                    bell.circle(center_x, center_y, bell_radius);
                    let bell_fill = if pressed {
                        vg::Color::rgb(255, 220, 130)
                    } else {
                        vg::Color::rgb(
                            pad.color.0.saturating_add(30),
                            pad.color.1.saturating_add(30),
                            pad.color.2.saturating_add(20),
                        )
                    };
                    canvas.fill_path(&bell, &vg::Paint::color(bell_fill));
                    let mut bell_edge = vg::Paint::color(vg::Color::rgba(255, 255, 255, 80));
                    bell_edge.set_line_width(1.0);
                    canvas.stroke_path(&bell, &bell_edge);
                }
                PadKind::Pedal => {
                    // Pedal body: rounded metallic footplate
                    let mut pedal = vg::Path::new();
                    pedal.circle(center_x, center_y, radius);
                    let pedal_fill = if pressed {
                        vg::Color::rgb(246, 196, 103)
                    } else {
                        vg::Color::rgb(
                            (pad.color.0 as f32 * (0.50 + energy * 0.50)) as u8,
                            (pad.color.1 as f32 * (0.50 + energy * 0.50)) as u8,
                            (pad.color.2 as f32 * (0.50 + energy * 0.50)) as u8,
                        )
                    };
                    canvas.fill_path(&pedal, &vg::Paint::color(pedal_fill));
                    let mut pedal_edge = vg::Paint::color(if pressed {
                        vg::Color::rgb(255, 230, 160)
                    } else {
                        vg::Color::rgb(180, 195, 215)
                    });
                    pedal_edge.set_line_width(1.5);
                    canvas.stroke_path(&pedal, &pedal_edge);
                }
            }

            let (label_r, label_g, label_b) = pad_label_rgb(pressed);
            let mut label = vg::Paint::color(vg::Color::rgb(label_r, label_g, label_b));
            let localized_label = match pad.note {
                36 => translate(language, "drum.pad.36", "Bass Drum"),
                38 => translate(language, "drum.pad.38", "Acoustic Snare"),
                41 => translate(language, "drum.pad.41", "Floor Tom"),
                42 => translate(language, "drum.pad.42", "Closed Hi-Hat"),
                44 => translate(language, "drum.pad.44", "Pedal Hi-Hat"),
                45 => translate(language, "drum.pad.45", "Mid Tom"),
                46 => translate(language, "drum.pad.46", "Open Hi-Hat"),
                49 => translate(language, "drum.pad.49", "Crash Cymbal"),
                50 => translate(language, "drum.pad.50", "High Tom"),
                51 => translate(language, "drum.pad.51", "Ride Cymbal"),
                _ => pad.label,
            };
            label.set_font_size(if radius > 36.0 { 11.0 } else { 9.5 });
            label.set_text_align(vg::Align::Center);
            canvas.fill_text(center_x, center_y - 2.0, localized_label, &label);

            let badge_text = format!("{} · [{}]", pad.note, pad.hotkey);
            let mut badge_paint = vg::Paint::color(if pressed {
                vg::Color::rgb(45, 52, 65)
            } else {
                vg::Color::rgba(220, 230, 245, 180)
            });
            badge_paint.set_text_align(vg::Align::Center);
            badge_paint.set_font_size(8.5);
            canvas.fill_text(center_x, center_y + 11.0, &badge_text, &badge_paint);
        }

        let mut footer = vg::Paint::color(vg::Color::rgb(112, 124, 144));
        footer.set_font_size(9.0);
        footer.set_text_align(vg::Align::Right);
        canvas.fill_text(
            bounds.x + bounds.w - 14.0,
            bounds.y + bounds.h - 10.0,
            translate(
                language,
                "drum.pad-map.hint",
                "35/36 kick · 38 snare · 41–51 kit",
            ),
            &footer,
        );
    }
}

pub fn pad_label_rgb(pressed: bool) -> (u8, u8, u8) {
    if pressed {
        (16, 19, 25)
    } else {
        (245, 247, 250)
    }
}

#[cfg(test)]
mod tests {
    use super::pad_label_rgb;

    #[test]
    fn pressed_pad_uses_dark_text_on_the_gold_highlight() {
        assert_eq!(pad_label_rgb(true), (16, 19, 25));
        assert_eq!(pad_label_rgb(false), (245, 247, 250));
    }
}
