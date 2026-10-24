//! Interactive Spatial Microphone Soundstage Widget for Vizia.
//!
//! Provides an intuitive physical acoustic top-down metaphor of the 9-foot grand
//! piano soundboard, displaying:
//! 1. Stereo Close Mics (hovering directly over the treble and bass bridge unisons)
//! 2. Player Seated Binaural Mics (positioned at the pianist's binaural listening position)
//! 3. Ambient Hall Decca Tree Mics (positioned into the room space for diffuse hall reflections)
//! Each microphone features an acoustic sensitivity halo whose glowing radius and brightness
//! dynamically scale with that perspective's gain level.
//!
//! Drag any microphone up or down to adjust its gain: the close pair shares one
//! gain, the player mic has its own, and the Decca tree shares one. Each drag is
//! one bracketed gesture, so it undoes in a single step.

use super::skia_compat as vg;
use super::skia_compat::CanvasExt;
use physics_ui::{translate, translate_format};
use std::sync::Arc;
use vizia_plug::vizia::prelude::*;
use vizia_plug::widgets::param_base::ParamWidgetBase;

use crate::nice_plugin::PhysicsPianoParams;
use physics_ui::Language;

/// Which gain a grabbed microphone controls.
#[derive(Clone, Copy, PartialEq, Eq)]
enum MicGroup {
    Close,
    Player,
    Ambient,
}

/// Microphone capsule positions in window coordinates, computed from the widget
/// bounds. Single source of truth for both drawing and drag hit-testing.
struct MicPoints {
    close_l: (f32, f32),
    close_r: (f32, f32),
    player: (f32, f32),
    amb_l: (f32, f32),
    amb_r: (f32, f32),
    amb_c: (f32, f32),
}

fn mic_points(x: f32, y: f32, w: f32, h: f32) -> MicPoints {
    let pw = w * 0.44;
    let ph = h * 0.70;
    let px = x + w * 0.08;
    let py = y + h * 0.20;
    let room_center_x = x + w * 0.76;
    let room_center_y = y + h * 0.52;
    MicPoints {
        close_l: (px + pw * 0.32, py + ph * 0.50),
        close_r: (px + pw * 0.62, py + ph * 0.40),
        player: (px + pw * 0.50, py + ph + h * 0.08),
        amb_l: (room_center_x - 18.0, room_center_y + 12.0),
        amb_r: (room_center_x + 18.0, room_center_y + 12.0),
        amb_c: (room_center_x, room_center_y - 18.0),
    }
}

/// Grab radius around each capsule, generous enough to catch the 3.5px dot.
const GRAB_RADIUS: f32 = 14.0;

/// Gain change per pixel of vertical drag, matching the knob drag feel.
const DRAG_SENSITIVITY: f32 = 0.0042;

/// Mic gains span -60 dB to +6 dB.
const GAIN_MIN_DB: f32 = -60.0;
const GAIN_SPAN_DB: f32 = 66.0;

pub struct MicStageWidget {
    params: Arc<PhysicsPianoParams>,
    close_base: ParamWidgetBase,
    player_base: ParamWidgetBase,
    amb_base: ParamWidgetBase,
    drag: Option<(MicGroup, f32, f32)>,
    language: Language,
}

impl MicStageWidget {
    pub fn new(
        cx: &mut Context,
        params: Arc<PhysicsPianoParams>,
        language: Language,
    ) -> Handle<'_, Self> {
        let close_base = ParamWidgetBase::new(cx, &params.mic_close);
        let player_base = ParamWidgetBase::new(cx, &params.mic_player);
        let amb_base = ParamWidgetBase::new(cx, &params.mic_ambient);
        Self {
            params,
            close_base,
            player_base,
            amb_base,
            drag: None,
            language,
        }
        .build(cx, |_| {})
    }

    fn base_for(&self, group: MicGroup) -> ParamWidgetBase {
        match group {
            MicGroup::Close => self.close_base,
            MicGroup::Player => self.player_base,
            MicGroup::Ambient => self.amb_base,
        }
    }

    fn gain_norm(&self, group: MicGroup) -> f32 {
        let db = match group {
            MicGroup::Close => self.params.mic_close.value(),
            MicGroup::Player => self.params.mic_player.value(),
            MicGroup::Ambient => self.params.mic_ambient.value(),
        };
        (db - GAIN_MIN_DB) / GAIN_SPAN_DB
    }
}

impl View for MicStageWidget {
    fn element(&self) -> Option<&'static str> {
        Some("mic-stage-widget")
    }

    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|window_event, meta| match window_event {
            WindowEvent::MouseDown(MouseButton::Left) => {
                let bounds = cx.bounds();
                let (mx, my) = (cx.mouse().cursor_x, cx.mouse().cursor_y);
                let pts = mic_points(bounds.x, bounds.y, bounds.w, bounds.h);
                let candidates = [
                    (MicGroup::Close, pts.close_l),
                    (MicGroup::Close, pts.close_r),
                    (MicGroup::Player, pts.player),
                    (MicGroup::Ambient, pts.amb_l),
                    (MicGroup::Ambient, pts.amb_r),
                    (MicGroup::Ambient, pts.amb_c),
                ];
                let mut hit = None;
                for (group, (x, y)) in candidates {
                    let dx = mx - x;
                    let dy = my - y;
                    if dx * dx + dy * dy <= GRAB_RADIUS * GRAB_RADIUS {
                        hit = Some(group);
                        break;
                    }
                }
                if let Some(group) = hit {
                    self.drag = Some((group, self.gain_norm(group), my));
                    cx.capture();
                    self.base_for(group).begin_set_parameter(cx);
                    meta.consume();
                }
            }
            WindowEvent::MouseUp(MouseButton::Left) => {
                if let Some((group, _, _)) = self.drag.take() {
                    cx.release();
                    self.base_for(group).end_set_parameter(cx);
                    meta.consume();
                }
            }
            WindowEvent::MouseMove(_x, y) => {
                if let Some((group, start_norm, start_y)) = self.drag {
                    let norm =
                        (start_norm + (start_y - *y) * DRAG_SENSITIVITY).clamp(0.0, 1.0);
                    self.base_for(group).set_normalized_value(cx, norm);
                    meta.consume();
                }
            }
            _ => {}
        });
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = cx.bounds();
        if bounds.w < 10.0 || bounds.h < 10.0 {
            return;
        }

        // 1. Background Box
        let mut bg_path = vg::Path::new();
        bg_path.rounded_rect(bounds.x, bounds.y, bounds.w, bounds.h, 6.0);
        let bg_paint = vg::Paint::color(vg::Color::rgb(22, 24, 32));
        canvas.fill_path(&bg_path, &bg_paint);

        let border_paint = vg::Paint::color(vg::Color::rgb(45, 48, 62)).with_line_width(1.0);
        canvas.stroke_path(&bg_path, &border_paint);

        // 2. Piano Top-Down Contour Geometry (Acoustic Grand Harp Silhouette)
        let pw = bounds.w * 0.44;
        let ph = bounds.h * 0.70;
        let px = bounds.x + bounds.w * 0.08;
        let py = bounds.y + bounds.h * 0.20;

        // Draw spruce soundboard fill
        let mut sb_path = vg::Path::new();
        sb_path.move_to(px, py + ph); // Front left (keyboard bass)
        sb_path.line_to(px + pw, py + ph); // Front right (keyboard treble)
        sb_path.line_to(px + pw, py + ph * 0.60); // Straight cheek
        sb_path.quad_to(px + pw * 0.85, py + ph * 0.20, px + pw * 0.45, py); // Treble bentside curve
        sb_path.quad_to(px + pw * 0.20, py, px + pw * 0.10, py + ph * 0.15); // Tail curve
        sb_path.line_to(px, py + ph); // Long straight spine (bass side)
        sb_path.close();

        let sb_fill = vg::Paint::color(vg::Color::rgb(38, 34, 30));
        canvas.fill_path(&sb_path, &sb_fill);

        let rim_stroke = vg::Paint::color(vg::Color::rgb(190, 160, 90)).with_line_width(1.8);
        canvas.stroke_path(&sb_path, &rim_stroke);

        // Cast Iron Plate & Soundboard Bridges
        let mut bridge_path = vg::Path::new();
        // Long Bridge (Tenor/Treble)
        bridge_path.move_to(px + pw * 0.25, py + ph * 0.75);
        bridge_path.quad_to(
            px + pw * 0.65,
            py + ph * 0.45,
            px + pw * 0.72,
            py + ph * 0.25,
        );
        // Short Bass Bridge
        bridge_path.move_to(px + pw * 0.08, py + ph * 0.45);
        bridge_path.line_to(px + pw * 0.22, py + ph * 0.20);
        let bridge_paint = vg::Paint::color(vg::Color::rgb(140, 110, 60)).with_line_width(2.0);
        canvas.stroke_path(&bridge_path, &bridge_paint);

        // Keyboard strip at front
        let mut kb_strip = vg::Path::new();
        kb_strip.rect(px, py + ph - 4.0, pw, 4.0);
        let kb_paint = vg::Paint::color(vg::Color::rgb(240, 240, 245));
        canvas.fill_path(&kb_strip, &kb_paint);

        // 3. Microphone Positions & Acoustic Halo Radiations
        let close_db = self.params.mic_close.value();
        let player_db = self.params.mic_player.value();
        let amb_db = self.params.mic_ambient.value();

        let norm_gain = |db: f32| -> f32 { ((db + 40.0) / 46.0).clamp(0.05, 1.0) };

        let g_close = norm_gain(close_db);
        let g_player = norm_gain(player_db);
        let g_amb = norm_gain(amb_db);

        // A. Close Stereo Pair (over the bridge)
        let pts = mic_points(bounds.x, bounds.y, bounds.w, bounds.h);
        let (close_l_x, close_l_y) = pts.close_l;
        let (close_r_x, close_r_y) = pts.close_r;

        let draw_mic = |canvas: &Canvas,
                        mx: f32,
                        my: f32,
                        gain_norm: f32,
                        r_base: f32,
                        color_rgb: (f32, f32, f32)| {
            let halo_r = r_base * (0.6 + 0.9 * gain_norm);
            let mut halo = vg::Path::new();
            halo.circle(mx, my, halo_r);
            let halo_paint = vg::Paint::color(vg::Color::rgbaf(
                color_rgb.0,
                color_rgb.1,
                color_rgb.2,
                0.15 + 0.35 * gain_norm,
            ));
            canvas.fill_path(&halo, &halo_paint);

            // Center capsule icon
            let mut capsule = vg::Path::new();
            capsule.circle(mx, my, 3.5);
            let cap_paint = vg::Paint::color(vg::Color::rgbaf(
                color_rgb.0,
                color_rgb.1,
                color_rgb.2,
                0.95,
            ));
            canvas.fill_path(&capsule, &cap_paint);
            let cap_border = vg::Paint::color(vg::Color::rgb(255, 255, 255)).with_line_width(1.0);
            canvas.stroke_path(&capsule, &cap_border);
        };

        // Draw Close Mics (Gold/Cyan)
        draw_mic(
            canvas,
            close_l_x,
            close_l_y,
            g_close,
            14.0,
            (0.2, 0.85, 1.0),
        );
        draw_mic(
            canvas,
            close_r_x,
            close_r_y,
            g_close,
            14.0,
            (0.2, 0.85, 1.0),
        );

        // B. Player Seated Binaural Mics (in front of keyboard)
        let (player_x, player_y) = pts.player;
        draw_mic(
            canvas,
            player_x,
            player_y,
            g_player,
            16.0,
            (0.95, 0.75, 0.25),
        );

        // C. Ambient Decca Tree Mics (in the room on the right side)
        let room_center_x = bounds.x + bounds.w * 0.76;
        let room_center_y = bounds.y + bounds.h * 0.52;
        let (amb_tree_l_x, amb_tree_l_y) = pts.amb_l;
        let (amb_tree_r_x, amb_tree_r_y) = pts.amb_r;
        let (amb_tree_c_x, amb_tree_c_y) = pts.amb_c;

        // Decca Tree connection lines
        let mut tree_lines = vg::Path::new();
        tree_lines.move_to(amb_tree_l_x, amb_tree_l_y);
        tree_lines.line_to(amb_tree_c_x, amb_tree_c_y);
        tree_lines.line_to(amb_tree_r_x, amb_tree_r_y);
        let tree_stroke =
            vg::Paint::color(vg::Color::rgbaf(0.7, 0.4, 0.95, 0.35)).with_line_width(1.0);
        canvas.stroke_path(&tree_lines, &tree_stroke);

        draw_mic(
            canvas,
            amb_tree_l_x,
            amb_tree_l_y,
            g_amb,
            18.0,
            (0.75, 0.45, 1.0),
        );
        draw_mic(
            canvas,
            amb_tree_r_x,
            amb_tree_r_y,
            g_amb,
            18.0,
            (0.75, 0.45, 1.0),
        );
        draw_mic(
            canvas,
            amb_tree_c_x,
            amb_tree_c_y,
            g_amb,
            18.0,
            (0.75, 0.45, 1.0),
        );

        // 4. Labels and Gain Readouts
        let title_str = translate(
            self.language,
            "piano.mics.title",
            "SPATIAL ACOUSTIC SOUNDSTAGE",
        );

        let mut font_paint = vg::Paint::color(vg::Color::rgb(200, 205, 220));
        font_paint.set_font_size(10.5);
        font_paint.set_text_align(vg::Align::Left);
        let _ = canvas.fill_text(bounds.x + 8.0, bounds.y + 14.0, title_str, &font_paint);

        // Legend Text
        let mut legend_paint = vg::Paint::color(vg::Color::rgb(140, 145, 160));
        legend_paint.set_font_size(9.5);
        legend_paint.set_text_align(vg::Align::Right);

        let close = format!("{close_db:.1}");
        let player = format!("{player_db:.1}");
        let hall = format!("{amb_db:.1}");
        let info_str = translate_format(
            self.language,
            "piano.mics.readout",
            "Close: {close}dB | Player: {player}dB | Hall: {hall}dB",
            &[("close", &close), ("player", &player), ("hall", &hall)],
        );
        let _ = canvas.fill_text(
            bounds.x + bounds.w - 8.0,
            bounds.y + 14.0,
            &info_str,
            &legend_paint,
        );

        // Perspective Annotations
        legend_paint.set_text_align(vg::Align::Center);
        legend_paint.set_font_size(9.0);
        let _ = canvas.fill_text(
            player_x,
            player_y + 12.0,
            translate(self.language, "piano.mics.player", "Player (Binaural)"),
            &legend_paint,
        );

        let _ = canvas.fill_text(
            room_center_x,
            room_center_y + 26.0,
            translate(self.language, "piano.mics.hall", "Decca Tree (Hall)"),
            &legend_paint,
        );
    }
}
