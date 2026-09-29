//! Interactive Spatial Microphone Soundstage Widget for Vizia.
//!
//! Provides an intuitive physical acoustic top-down metaphor of the 9-foot grand
//! piano soundboard, displaying:
//! 1. Stereo Close Mics (hovering directly over the treble and bass bridge unisons)
//! 2. Player Seated Binaural Mics (positioned at the pianist's binaural listening position)
//! 3. Ambient Hall Decca Tree Mics (positioned into the room space for diffuse hall reflections)
//! Each microphone features an acoustic sensitivity halo whose glowing radius and brightness
//! dynamically scale with that perspective's gain level.

use super::skia_compat as vg;
use super::skia_compat::CanvasExt;
use std::sync::Arc;
use vizia_plug::vizia::prelude::*;

use crate::gui::i18n::Language;
use crate::nice_plugin::PhysicsPianoParams;

pub struct MicStageWidget {
    params: Arc<PhysicsPianoParams>,
    language: Language,
}

impl MicStageWidget {
    pub fn new(
        cx: &mut Context,
        params: Arc<PhysicsPianoParams>,
        language: Language,
    ) -> Handle<'_, Self> {
        Self { params, language }.build(cx, |_| {})
    }
}

impl View for MicStageWidget {
    fn element(&self) -> Option<&'static str> {
        Some("mic-stage-widget")
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
        let close_l_x = px + pw * 0.32;
        let close_l_y = py + ph * 0.50;
        let close_r_x = px + pw * 0.62;
        let close_r_y = py + ph * 0.40;

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
        let player_x = px + pw * 0.50;
        let player_y = py + ph + bounds.h * 0.08;
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
        let amb_tree_l_x = room_center_x - 18.0;
        let amb_tree_l_y = room_center_y + 12.0;
        let amb_tree_r_x = room_center_x + 18.0;
        let amb_tree_r_y = room_center_y + 12.0;
        let amb_tree_c_x = room_center_x;
        let amb_tree_c_y = room_center_y - 18.0;

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
        let title_str = match self.language {
            Language::English => "SPATIAL ACOUSTIC SOUNDSTAGE",
            Language::SimplifiedChinese => "多麦位空间声场物理布局",
        };

        let mut font_paint = vg::Paint::color(vg::Color::rgb(200, 205, 220));
        font_paint.set_font_size(10.5);
        font_paint.set_text_align(vg::Align::Left);
        let _ = canvas.fill_text(bounds.x + 8.0, bounds.y + 14.0, title_str, &font_paint);

        // Legend Text
        let mut legend_paint = vg::Paint::color(vg::Color::rgb(140, 145, 160));
        legend_paint.set_font_size(9.5);
        legend_paint.set_text_align(vg::Align::Right);

        let info_str = match self.language {
            Language::English => format!(
                "Close: {:.1}dB | Player: {:.1}dB | Hall: {:.1}dB",
                close_db, player_db, amb_db
            ),
            Language::SimplifiedChinese => format!(
                "近场: {:.1}dB | 演奏者: {:.1}dB | 空间厅堂: {:.1}dB",
                close_db, player_db, amb_db
            ),
        };
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
            match self.language {
                Language::English => "Player (Binaural)",
                Language::SimplifiedChinese => "演奏者耳位",
            },
            &legend_paint,
        );

        let _ = canvas.fill_text(
            room_center_x,
            room_center_y + 26.0,
            match self.language {
                Language::English => "Decca Tree (Hall)",
                Language::SimplifiedChinese => "空间厅堂阵列",
            },
            &legend_paint,
        );
    }
}
