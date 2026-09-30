//! Interactive drum-kit pad view.
//!
//! Each pad is a real General-MIDI trigger.  The widget only sends fixed-size
//! hit events; all synthesis remains on the audio thread.

use crate::nice_plugin::GuiDrumEvent;
use physics_ui::skia_compat as vg;
use physics_ui::skia_compat::CanvasExt;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
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
        label: "CRASH",
        x: 0.12,
        y: 0.20,
        radius: 0.105,
        color: (224, 157, 69),
    },
    Pad {
        note: 51,
        label: "RIDE",
        x: 0.88,
        y: 0.20,
        radius: 0.105,
        color: (195, 137, 61),
    },
    Pad {
        note: 46,
        label: "OPEN HAT",
        x: 0.22,
        y: 0.48,
        radius: 0.09,
        color: (213, 178, 88),
    },
    Pad {
        note: 42,
        label: "CLOSED HAT",
        x: 0.22,
        y: 0.73,
        radius: 0.085,
        color: (166, 143, 79),
    },
    Pad {
        note: 43,
        label: "TOM 1",
        x: 0.43,
        y: 0.30,
        radius: 0.105,
        color: (155, 91, 69),
    },
    Pad {
        note: 45,
        label: "TOM 2",
        x: 0.59,
        y: 0.30,
        radius: 0.11,
        color: (164, 89, 68),
    },
    Pad {
        note: 41,
        label: "FLOOR TOM",
        x: 0.77,
        y: 0.57,
        radius: 0.125,
        color: (137, 74, 61),
    },
    Pad {
        note: 38,
        label: "SNARE",
        x: 0.39,
        y: 0.58,
        radius: 0.105,
        color: (119, 145, 170),
    },
    Pad {
        note: 36,
        label: "KICK",
        x: 0.57,
        y: 0.73,
        radius: 0.145,
        color: (90, 109, 139),
    },
    Pad {
        note: 44,
        label: "PEDAL",
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
    held_note: Option<u8>,
}

impl DrumPadWidget {
    pub fn new(
        cx: &mut Context,
        voice_energies: Arc<[AtomicU32; VOICE_COUNT]>,
        gui_tx: crossbeam_channel::Sender<GuiDrumEvent>,
    ) -> Handle<'_, Self> {
        Self {
            voice_energies,
            gui_tx,
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

        let mut title = vg::Paint::color(vg::Color::rgb(136, 149, 172));
        title.set_font_size(10.0);
        title.set_text_align(vg::Align::Left);
        canvas.fill_text(
            bounds.x + 14.0,
            bounds.y + 18.0,
            "CLICK A PAD TO PLAY · MIDI DRUM MAP",
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

            let mut label = vg::Paint::color(vg::Color::rgb(245, 247, 250));
            label.set_font_size(if pad.label.len() > 8 { 8.0 } else { 9.0 });
            label.set_text_align(vg::Align::Center);
            canvas.fill_text(center_x, center_y + 3.0, pad.label, &label);
        }

        let mut footer = vg::Paint::color(vg::Color::rgb(112, 124, 144));
        footer.set_font_size(9.0);
        footer.set_text_align(vg::Align::Right);
        canvas.fill_text(
            bounds.x + bounds.w - 14.0,
            bounds.y + bounds.h - 10.0,
            "35/36 kick · 38 snare · 41–51 kit",
            &footer,
        );
    }
}
