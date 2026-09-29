//! Interactive 88-Key Physical Piano Keyboard Widget with Dynamic Key Travel for Vizia.
//!
//! Features:
//! - Full 88-key physical layout (A0 to C8, MIDI 21 to 108: 52 white keys, 36 black keys).
//! - 100% Lock-Free active key querying via AtomicU64 bitsets.
//! - Dynamic key travel sink (触键动态下沉与力度反馈): when a key is struck (via MIDI, mouse,
//!   or laptop keyboard), the key visibly sinks downwards by an amount proportional to
//!   the hammer strike velocity, casting dynamic shadows beneath the keybed overhang.
//! - Mouse click and glissando drag support with vertical touch sensitivity.
//! - Laptop QWERTY keyboard octave playing (A-K / W,E,T,Y,U).

use super::skia_compat as vg;
use super::skia_compat::CanvasExt;
use crossbeam_channel::Sender;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use vizia_plug::vizia::prelude::*;

use crate::engine::EngineEvent;

pub struct PianoKeyboardWidget {
    gui_tx: Sender<EngineEvent>,
    active_keys_low: Arc<AtomicU64>,
    active_keys_high: Arc<AtomicU64>,
    key_velocities: Arc<parking_lot::RwLock<[f32; 88]>>,
    held_mouse_key: Option<u8>,
}

impl PianoKeyboardWidget {
    pub fn new(
        cx: &mut Context,
        gui_tx: Sender<EngineEvent>,
        active_keys_low: Arc<AtomicU64>,
        active_keys_high: Arc<AtomicU64>,
        key_velocities: Arc<parking_lot::RwLock<[f32; 88]>>,
    ) -> Handle<'_, Self> {
        Self {
            gui_tx,
            active_keys_low,
            active_keys_high,
            key_velocities,
            held_mouse_key: None,
        }
        .build(cx, |_| {})
        .focusable(true)
    }

    #[inline]
    fn is_black(midi: u8) -> bool {
        matches!(midi % 12, 1 | 3 | 6 | 8 | 10)
    }

    fn find_key_at(&self, bounds: &BoundingBox, x: f32, y: f32) -> Option<(u8, f32)> {
        if x < bounds.x || x > bounds.x + bounds.w || y < bounds.y || y > bounds.y + bounds.h {
            return None;
        }

        let num_white_keys = 52.0;
        let white_w = bounds.w / num_white_keys;
        let black_w = white_w * 0.64;
        let black_h = bounds.h * 0.62;

        // 1. Check black keys first (top z-index)
        let mut curr_white = 0;
        for midi in 21..=108 {
            if Self::is_black(midi) {
                let bx = bounds.x + curr_white as f32 * white_w - black_w * 0.5;
                let by = bounds.y;
                if x >= bx && x <= bx + black_w && y >= by && y <= by + black_h {
                    let rel_y = ((y - by) / black_h).clamp(0.0, 1.0);
                    let vel = (0.35 + 0.62 * rel_y).clamp(0.2, 1.0);
                    return Some((midi, vel));
                }
            } else {
                curr_white += 1;
            }
        }

        // 2. Check white keys
        let rel_x = (x - bounds.x).clamp(0.0, bounds.w - 0.1);
        let white_idx = (rel_x / white_w).floor() as usize;

        let mut curr_w = 0;
        for midi in 21..=108 {
            if !Self::is_black(midi) {
                if curr_w == white_idx {
                    let rel_y = ((y - bounds.y) / bounds.h).clamp(0.0, 1.0);
                    let vel = (0.35 + 0.62 * rel_y).clamp(0.2, 1.0);
                    return Some((midi, vel));
                }
                curr_w += 1;
            }
        }

        None
    }

    fn play_note(&mut self, key: u8, velocity: f32) {
        if (21..=108).contains(&key) {
            let idx = (key - 21) as usize;
            self.key_velocities.write()[idx] = velocity;
        }
        let _ = self.gui_tx.send(EngineEvent::NoteOn {
            time: 0,
            key,
            velocity: velocity as f64,
        });
    }

    fn release_note(&mut self, key: u8) {
        if (21..=108).contains(&key) {
            let idx = (key - 21) as usize;
            // Mark GUI releases explicitly while the audio thread debounces NoteOff for
            // keyboard auto-repeat. This prevents the stale active bit from drawing a stuck key.
            self.key_velocities.write()[idx] = -1.0;
        }
        let _ = self.gui_tx.send(EngineEvent::NoteOff { time: 0, key });
    }
}

impl Drop for PianoKeyboardWidget {
    fn drop(&mut self) {
        if let Some(key) = self.held_mouse_key.take() {
            self.release_note(key);
        }
    }
}

impl View for PianoKeyboardWidget {
    fn element(&self) -> Option<&'static str> {
        Some("piano-keyboard-widget")
    }

    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|window_event, meta| match window_event {
            WindowEvent::MouseDown(MouseButton::Left) => {
                cx.focus();
                let bounds = cx.bounds();
                let mouse_x = cx.mouse().cursor_x;
                let mouse_y = cx.mouse().cursor_y;
                if let Some((key, vel)) = self.find_key_at(&bounds, mouse_x, mouse_y) {
                    if let Some(prev) = self.held_mouse_key {
                        if prev != key {
                            self.release_note(prev);
                        }
                    }
                    self.held_mouse_key = Some(key);
                    self.play_note(key, vel);
                    cx.capture();
                    cx.needs_redraw();
                    meta.consume();
                }
            }
            WindowEvent::MouseMove(x, y) => {
                if self.held_mouse_key.is_some() {
                    let bounds = cx.bounds();
                    if let Some((key, vel)) = self.find_key_at(&bounds, *x, *y) {
                        if self.held_mouse_key != Some(key) {
                            if let Some(prev) = self.held_mouse_key {
                                self.release_note(prev);
                            }
                            self.held_mouse_key = Some(key);
                            self.play_note(key, vel);
                            cx.needs_redraw();
                        }
                    }
                    meta.consume();
                }
            }
            WindowEvent::MouseUp(MouseButton::Left) => {
                if let Some(prev) = self.held_mouse_key.take() {
                    self.release_note(prev);
                    cx.release();
                    cx.needs_redraw();
                    meta.consume();
                }
            }
            WindowEvent::FocusOut => {
                if let Some(prev) = self.held_mouse_key.take() {
                    self.release_note(prev);
                    cx.needs_redraw();
                }
            }
            _ => {}
        });
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = cx.bounds();
        if bounds.w < 20.0 || bounds.h < 20.0 {
            return;
        }

        let low_mask = self.active_keys_low.load(Ordering::Relaxed);
        let high_mask = self.active_keys_high.load(Ordering::Relaxed);
        let vels = self.key_velocities.read();

        let is_active = |midi: u8| -> (bool, f32) {
            let audio_active = if (21..85).contains(&midi) {
                (low_mask & (1u64 << (midi - 21))) != 0
            } else if (85..=108).contains(&midi) {
                (high_mask & (1u64 << (midi - 85))) != 0
            } else {
                false
            };
            let vel = if (21..=108).contains(&midi) {
                vels[(midi - 21) as usize]
            } else {
                0.8
            };
            // GUI-originated notes need to react before the next audio block updates the
            // lock-free active-key masks. The local mouse state also makes the pressed key
            // visible for the full duration of a mouse hold.
            (
                vel >= 0.0 && (audio_active || self.held_mouse_key == Some(midi) || vel > 0.0),
                vel.max(0.0),
            )
        };

        // Red felt strip above the keybed
        let felt_h = 3.5;
        let mut felt_path = vg::Path::new();
        felt_path.rect(bounds.x, bounds.y, bounds.w, felt_h);
        let felt_paint = vg::Paint::color(vg::Color::rgb(160, 24, 28));
        canvas.fill_path(&felt_path, &felt_paint);

        let num_white_keys = 52.0;
        let white_w = bounds.w / num_white_keys;
        let white_h = bounds.h - felt_h;
        let black_w = white_w * 0.64;
        let black_h = white_h * 0.62;

        let key_top_y = bounds.y + felt_h;

        // 1. Draw 52 White Keys
        let mut curr_white = 0;
        for midi in 21..=108 {
            if !Self::is_black(midi) {
                let (active, vel) = is_active(midi);
                let kx = bounds.x + curr_white as f32 * white_w;
                // Dynamic Key Travel Sink: When active, key sinks downwards!
                let y_sink = if active { 2.5 + 2.5 * vel } else { 0.0 };
                let ky = key_top_y + y_sink;
                let kh = white_h - y_sink;

                // Keybed shadow behind sunken key
                if active {
                    let mut shadow_path = vg::Path::new();
                    shadow_path.rect(kx, key_top_y, white_w, y_sink + 1.0);
                    let shadow_paint = vg::Paint::color(vg::Color::rgbaf(0.0, 0.0, 0.0, 0.65));
                    canvas.fill_path(&shadow_path, &shadow_paint);
                }

                // White key surface
                let mut key_path = vg::Path::new();
                key_path.rounded_rect(kx + 0.5, ky, white_w - 1.0, kh, 2.0);

                let key_color = if active {
                    vg::Color::rgb(255, 235, 175) // Warm golden pressed ivory
                } else {
                    vg::Color::rgb(246, 246, 250) // Crisp polished ivory
                };
                let fill_paint = vg::Paint::color(key_color);
                canvas.fill_path(&key_path, &fill_paint);

                // Subtle edge stroke
                let edge_color = if active {
                    vg::Color::rgb(180, 150, 80)
                } else {
                    vg::Color::rgb(180, 185, 195)
                };
                let stroke_paint = vg::Paint::color(edge_color).with_line_width(0.75);
                canvas.stroke_path(&key_path, &stroke_paint);

                // Key bevel bottom shadow
                let mut bevel_path = vg::Path::new();
                bevel_path.rect(kx + 1.0, ky + kh - 3.5, white_w - 2.0, 3.0);
                let bevel_paint = vg::Paint::color(vg::Color::rgbaf(0.7, 0.72, 0.76, 0.45));
                canvas.fill_path(&bevel_path, &bevel_paint);

                curr_white += 1;
            }
        }

        // 2. Draw 36 Black Keys
        curr_white = 0;
        for midi in 21..=108 {
            if Self::is_black(midi) {
                let (active, vel) = is_active(midi);
                let kx = bounds.x + curr_white as f32 * white_w - black_w * 0.5;
                // Dynamic Key Travel Sink for Black Keys
                let y_sink = if active { 1.8 + 2.0 * vel } else { 0.0 };
                let ky = key_top_y + y_sink;
                let kh = black_h - y_sink;

                // Contact shadow under black key
                let mut drop_shadow = vg::Path::new();
                drop_shadow.rect(kx - 1.0, ky + kh, black_w + 2.0, 2.5);
                let shadow_paint = vg::Paint::color(vg::Color::rgbaf(0.0, 0.0, 0.0, 0.45));
                canvas.fill_path(&drop_shadow, &shadow_paint);

                // Black key body
                let mut bk_path = vg::Path::new();
                bk_path.rounded_rect(kx, ky, black_w, kh, 1.8);

                let bk_color = if active {
                    vg::Color::rgb(195, 145, 60) // Amber gold highlight when struck
                } else {
                    vg::Color::rgb(28, 29, 36) // Matte ebony black
                };
                let bk_fill = vg::Paint::color(bk_color);
                canvas.fill_path(&bk_path, &bk_fill);

                let bk_stroke = vg::Paint::color(vg::Color::rgb(15, 16, 20)).with_line_width(0.75);
                canvas.stroke_path(&bk_path, &bk_stroke);

                // Top highlight line for ebony bevel
                let mut hl_path = vg::Path::new();
                hl_path.rect(kx + 1.2, ky + 1.0, black_w - 2.4, kh * 0.88);
                let hl_paint = vg::Paint::color(vg::Color::rgbaf(
                    1.0,
                    1.0,
                    1.0,
                    if active { 0.28 } else { 0.08 },
                ));
                canvas.fill_path(&hl_path, &hl_paint);
            } else {
                curr_white += 1;
            }
        }
    }
}
