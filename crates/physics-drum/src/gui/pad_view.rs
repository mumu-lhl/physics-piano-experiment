//! Interactive drum-kit pad view.
//!
//! Each pad is a real General-MIDI trigger.  The widget only sends fixed-size
//! hit events; all synthesis remains on the audio thread.

use crate::nice_plugin::GuiDrumEvent;
use physics_ui::skia_compat as vg;
use physics_ui::skia_compat::CanvasExt;
use physics_ui::{Language, translate};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU32, Ordering};
use vizia_plug::vizia::prelude::*;

const VOICE_COUNT: usize = 10;

#[derive(Clone, Copy)]
struct Pad {
    note: u8,
    label: &'static str,
    x: f32,
    y: f32,
    radius: f32,
    color: (u8, u8, u8),
}

const PADS: [Pad; VOICE_COUNT] = [
    Pad {
        note: 49,
        label: "Crash Cymbal",
        x: 0.12,
        y: 0.20,
        radius: 0.105,
        color: (224, 157, 69),
    },
    Pad {
        note: 51,
        label: "Ride Cymbal",
        x: 0.88,
        y: 0.20,
        radius: 0.105,
        color: (195, 137, 61),
    },
    Pad {
        note: 46,
        label: "Open Hi-Hat",
        x: 0.22,
        y: 0.48,
        radius: 0.09,
        color: (213, 178, 88),
    },
    Pad {
        note: 42,
        label: "Closed Hi-Hat",
        x: 0.22,
        y: 0.73,
        radius: 0.085,
        color: (166, 143, 79),
    },
    Pad {
        note: 43,
        label: "High Floor Tom",
        x: 0.43,
        y: 0.30,
        radius: 0.105,
        color: (155, 91, 69),
    },
    Pad {
        note: 45,
        label: "Low Tom",
        x: 0.59,
        y: 0.30,
        radius: 0.11,
        color: (164, 89, 68),
    },
    Pad {
        note: 41,
        label: "Low Floor Tom",
        x: 0.77,
        y: 0.57,
        radius: 0.125,
        color: (137, 74, 61),
    },
    Pad {
        note: 38,
        label: "Acoustic Snare",
        x: 0.39,
        y: 0.58,
        radius: 0.105,
        color: (119, 145, 170),
    },
    Pad {
        note: 36,
        label: "Bass Drum",
        x: 0.57,
        y: 0.73,
        radius: 0.145,
        color: (90, 109, 139),
    },
    Pad {
        note: 44,
        label: "Pedal Hi-Hat",
        x: 0.12,
        y: 0.90,
        radius: 0.065,
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

    fn pad_at(&self, bounds: &BoundingBox, x: f32, y: f32) -> Option<Pad> {
        let nx = (x - bounds.x) / bounds.w.max(1.0);
        let ny = (y - bounds.y) / bounds.h.max(1.0);
        PADS.iter().copied().find(|pad| {
            let dx = nx - pad.x;
            let dy = ny - pad.y;
            dx * dx + dy * dy <= pad.radius * pad.radius
        })
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
                if let Some(pad) = self.pad_at(&bounds, cx.mouse().cursor_x, cx.mouse().cursor_y) {
                    let velocity = 1.0
                        - ((cx.mouse().cursor_y - bounds.y) / bounds.h.max(1.0)).clamp(0.0, 1.0)
                            * 0.55;
                    self.hit(pad, velocity);
                    self.held_note = Some(pad.note);
                    cx.capture();
                    cx.needs_redraw();
                    meta.consume();
                }
            }
            WindowEvent::MouseMove(x, y) if self.held_note.is_some() => {
                let bounds = cx.bounds();
                let next = self.pad_at(&bounds, *x, *y).map(|pad| pad.note);
                if next != self.held_note {
                    if let Some(note) = next {
                        let pad = PADS[voice_index(note)];
                        self.hit(pad, 0.82);
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
            let glow_radius = radius + energy * 9.0;

            if energy > 0.01 || pressed {
                let mut glow = vg::Path::new();
                glow.circle(center_x, center_y, glow_radius);
                canvas.fill_path(
                    &glow,
                    &vg::Paint::color(vg::Color::rgba(
                        pad.color.0,
                        pad.color.1,
                        pad.color.2,
                        (35.0 + energy * 80.0) as u8,
                    )),
                );
            }

            let mut shape = vg::Path::new();
            shape.circle(center_x, center_y, radius);
            let fill = if pressed {
                vg::Color::rgb(246, 196, 103)
            } else {
                vg::Color::rgb(
                    (pad.color.0 as f32 * (0.55 + energy * 0.45)) as u8,
                    (pad.color.1 as f32 * (0.55 + energy * 0.45)) as u8,
                    (pad.color.2 as f32 * (0.55 + energy * 0.45)) as u8,
                )
            };
            canvas.fill_path(&shape, &vg::Paint::color(fill));
            let mut outline = vg::Paint::color(vg::Color::rgb(221, 226, 235));
            outline.set_line_width(if pressed { 2.0 } else { 1.0 });
            canvas.stroke_path(&shape, &outline);

            let (label_r, label_g, label_b) = pad_label_rgb(pressed);
            let mut label = vg::Paint::color(vg::Color::rgb(label_r, label_g, label_b));
            let localized_label = match pad.note {
                36 => translate(language, "drum.pad.36", "Bass Drum"),
                38 => translate(language, "drum.pad.38", "Acoustic Snare"),
                41 => translate(language, "drum.pad.41", "Low Floor Tom"),
                42 => translate(language, "drum.pad.42", "Closed Hi-Hat"),
                43 => translate(language, "drum.pad.43", "High Floor Tom"),
                44 => translate(language, "drum.pad.44", "Pedal Hi-Hat"),
                45 => translate(language, "drum.pad.45", "Low Tom"),
                46 => translate(language, "drum.pad.46", "Open Hi-Hat"),
                49 => translate(language, "drum.pad.49", "Crash Cymbal"),
                51 => translate(language, "drum.pad.51", "Ride Cymbal"),
                _ => pad.label,
            };
            let (top_line, bottom_line) = localized_label
                .rsplit_once(' ')
                .unwrap_or((localized_label, ""));
            label.set_font_size(if top_line.chars().count() > 9 {
                10.0
            } else {
                11.0
            });
            label.set_text_align(vg::Align::Center);
            canvas.fill_text(center_x, center_y - 2.0, top_line, &label);
            canvas.fill_text(center_x, center_y + 10.0, bottom_line, &label);
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

fn pad_label_rgb(pressed: bool) -> (u8, u8, u8) {
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
