//! Interactive Grand Piano Acoustic Lid Geometry Widget for Vizia.
//!
//! Visualizes the physical side-profile cross section of the grand piano rim and
//! the continuous rotating acoustic lid baffle. Clicking or dragging the lid directly
//! adjusts the lid opening angle from 0° (fully closed) to 60° (full concert hall radiation).

use super::skia_compat as vg;
use super::skia_compat::CanvasExt;
use std::f32::consts::PI;
use std::sync::Arc;
use vizia_plug::vizia::prelude::*;
use vizia_plug::widgets::param_base::ParamWidgetBase;

use crate::gui::i18n::Language;
use crate::nih_plugin::PhysicsPianoParams;

pub struct PianoLidWidget {
    params: Arc<PhysicsPianoParams>,
    param_base: ParamWidgetBase,
    is_dragging: bool,
    language: Language,
}

impl PianoLidWidget {
    pub fn new(
        cx: &mut Context,
        params: Arc<PhysicsPianoParams>,
        language: Language,
    ) -> Handle<'_, Self> {
        let param_base = ParamWidgetBase::new(cx, &params.lid_angle);

        Self {
            params,
            param_base,
            is_dragging: false,
            language,
        }
        .build(cx, |_| {})
    }
}

impl View for PianoLidWidget {
    fn element(&self) -> Option<&'static str> {
        Some("piano-lid-widget")
    }

    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|window_event, meta| match window_event {
            WindowEvent::MouseDown(MouseButton::Left) => {
                self.is_dragging = true;
                cx.capture();
                self.param_base.begin_set_parameter(cx);
                meta.consume();
            }
            WindowEvent::MouseUp(MouseButton::Left) => {
                if self.is_dragging {
                    self.is_dragging = false;
                    cx.release();
                    self.param_base.end_set_parameter(cx);
                    meta.consume();
                }
            }
            WindowEvent::MouseMove(_x, y) => {
                if self.is_dragging {
                    let bounds = cx.bounds();
                    if bounds.h > 10.0 {
                        // Dragging vertically: moving up opens lid (higher angle), down closes lid (0 deg)
                        let rel_y = ((bounds.y + bounds.h * 0.85) - *y) / (bounds.h * 0.7);
                        let norm = rel_y.clamp(0.0, 1.0);
                        self.param_base.set_normalized_value(cx, norm);
                    }
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

        let angle_deg = self.params.lid_angle.value();
        let angle_rad = (angle_deg as f32).to_radians();

        // 1. Background Frame
        let mut bg_path = vg::Path::new();
        bg_path.rounded_rect(bounds.x, bounds.y, bounds.w, bounds.h, 6.0);
        let bg_paint = vg::Paint::color(vg::Color::rgb(22, 24, 32));
        canvas.fill_path(&bg_path, &bg_paint);

        let border_paint = vg::Paint::color(vg::Color::rgb(45, 48, 62)).with_line_width(1.0);
        canvas.stroke_path(&bg_path, &border_paint);

        // 2. Geometry Center and Base Coordinates
        let base_x = bounds.x + bounds.w * 0.12;
        let base_y = bounds.y + bounds.h * 0.78;
        let body_len = bounds.w * 0.72;
        let body_depth = bounds.h * 0.22;

        // 3. Piano Case Rim (Side View: Bent rim, soundboard cavity, legs)
        // Main Case
        let mut case_path = vg::Path::new();
        case_path.move_to(base_x, base_y);
        case_path.line_to(base_x + body_len, base_y);
        case_path.line_to(base_x + body_len * 0.94, base_y + body_depth);
        case_path.line_to(base_x + body_len * 0.05, base_y + body_depth);
        case_path.close();

        let case_paint = vg::Paint::color(vg::Color::rgb(35, 36, 44));
        canvas.fill_path(&case_path, &case_paint);
        let case_stroke = vg::Paint::color(vg::Color::rgb(180, 150, 80)).with_line_width(1.5);
        canvas.stroke_path(&case_path, &case_stroke);

        // Legs
        let mut legs_path = vg::Path::new();
        let leg_top_y = base_y + body_depth;
        let leg_h = bounds.h * 0.12;
        // Front leg
        legs_path.rect(base_x + body_len * 0.12, leg_top_y, 6.0, leg_h);
        // Back leg
        legs_path.rect(base_x + body_len * 0.82, leg_top_y, 6.0, leg_h);
        let leg_paint = vg::Paint::color(vg::Color::rgb(28, 29, 36));
        canvas.fill_path(&legs_path, &leg_paint);
        let leg_stroke = vg::Paint::color(vg::Color::rgb(140, 120, 60)).with_line_width(1.0);
        canvas.stroke_path(&legs_path, &leg_stroke);

        // Inner Soundboard Glow inside case aperture
        let mut sb_path = vg::Path::new();
        sb_path.rect(base_x + body_len * 0.08, base_y - 2.0, body_len * 0.84, 4.0);
        let sb_paint = vg::Paint::color(vg::Color::rgb(210, 160, 90));
        canvas.fill_path(&sb_path, &sb_paint);

        // 4. Acoustic Radiation Wave Rays (emanating from soundboard aperture)
        if angle_deg > 2.0 {
            let emission_strength = (angle_deg / 60.0).clamp(0.0, 1.0);
            let num_arcs = 3;
            for i in 1..=num_arcs {
                let r = (i as f32) * 16.0 * (0.6 + 0.4 * emission_strength);
                let alpha = (0.55 - (i as f32) * 0.12) * emission_strength;
                let mut wave_path = vg::Path::new();
                let wave_center_x = base_x + body_len * 0.55;
                let wave_center_y = base_y - 5.0;
                wave_path.arc(
                    wave_center_x,
                    wave_center_y,
                    r,
                    -PI * 0.85,
                    -PI * (0.15 + 0.25 * (1.0 - emission_strength)),
                    vg::Solidity::Hole,
                );
                let wave_paint =
                    vg::Paint::color(vg::Color::rgbaf(1.0, 0.85, 0.45, alpha)).with_line_width(1.2);
                canvas.stroke_path(&wave_path, &wave_paint);
            }
        }

        // 5. Rotating Grand Piano Lid
        // Hinge point is at back of the case: (base_x + body_len, base_y)
        let hinge_x = base_x + body_len;
        let hinge_y = base_y;
        let lid_length = body_len * 0.98;

        // Lid tip coordinates rotated by angle_rad
        // In screen coordinates: rotating counter-clockwise upwards:
        // dx = -cos(theta) * len, dy = -sin(theta) * len
        let lid_tip_x = hinge_x - angle_rad.cos() * lid_length;
        let lid_tip_y = hinge_y - angle_rad.sin() * lid_length;

        // Lid prop stick (when opened >= 10 deg)
        if angle_deg >= 8.0 {
            let prop_base_x = base_x + body_len * 0.45;
            let prop_base_y = base_y;
            let prop_top_x = hinge_x - angle_rad.cos() * (lid_length * 0.52);
            let prop_top_y = hinge_y - angle_rad.sin() * (lid_length * 0.52);

            let mut prop_path = vg::Path::new();
            prop_path.move_to(prop_base_x, prop_base_y);
            prop_path.line_to(prop_top_x, prop_top_y);
            let prop_stroke = vg::Paint::color(vg::Color::rgb(220, 190, 110)).with_line_width(2.2);
            canvas.stroke_path(&prop_path, &prop_stroke);
        }

        // Lid Panel (thick wooden polished lacquer lid with golden bevel)
        let lid_thick = 4.0;
        let mut lid_path = vg::Path::new();
        lid_path.move_to(hinge_x, hinge_y);
        lid_path.line_to(lid_tip_x, lid_tip_y);
        lid_path.line_to(
            lid_tip_x - angle_rad.sin() * lid_thick,
            lid_tip_y + angle_rad.cos() * lid_thick,
        );
        lid_path.line_to(
            hinge_x - angle_rad.sin() * lid_thick,
            hinge_y + angle_rad.cos() * lid_thick,
        );
        lid_path.close();

        let lid_fill = vg::Paint::color(vg::Color::rgb(42, 44, 55));
        canvas.fill_path(&lid_path, &lid_fill);
        let lid_stroke = vg::Paint::color(vg::Color::rgb(255, 215, 110)).with_line_width(1.8);
        canvas.stroke_path(&lid_path, &lid_stroke);

        // Hinge circle
        let mut hinge_circle = vg::Path::new();
        hinge_circle.circle(hinge_x, hinge_y, 3.5);
        let hinge_paint = vg::Paint::color(vg::Color::rgb(255, 215, 120));
        canvas.fill_path(&hinge_circle, &hinge_paint);

        // 6. Header Label & Status Readout
        let title_str = match self.language {
            Language::English => "LID BAFFLE GEOMETRY",
            Language::SimplifiedChinese => "琴盖声学开合物理示意",
        };
        let status_str = match self.language {
            Language::English => {
                if angle_deg < 5.0 {
                    format!("{angle_deg:.0}° (Fully Closed)")
                } else if angle_deg < 25.0 {
                    format!("{angle_deg:.0}° (Short Stick)")
                } else if angle_deg < 50.0 {
                    format!("{angle_deg:.0}° (Half Open)")
                } else {
                    format!("{angle_deg:.0}° (Concert Grand)")
                }
            }
            Language::SimplifiedChinese => {
                if angle_deg < 5.0 {
                    format!("{angle_deg:.0}° (完全闭合)")
                } else if angle_deg < 25.0 {
                    format!("{angle_deg:.0}° (短支柱/微开)")
                } else if angle_deg < 50.0 {
                    format!("{angle_deg:.0}° (半开支柱)")
                } else {
                    format!("{angle_deg:.0}° (音乐会全开)")
                }
            }
        };

        let mut font_paint = vg::Paint::color(vg::Color::rgb(200, 205, 220));
        font_paint.set_font_size(10.5);
        font_paint.set_text_align(vg::Align::Left);
        let _ = canvas.fill_text(bounds.x + 8.0, bounds.y + 14.0, title_str, &font_paint);

        let mut stat_paint = vg::Paint::color(vg::Color::rgb(255, 215, 120));
        stat_paint.set_font_size(10.5);
        stat_paint.set_text_align(vg::Align::Right);
        let _ = canvas.fill_text(
            bounds.x + bounds.w - 8.0,
            bounds.y + 14.0,
            &status_str,
            &stat_paint,
        );
    }
}
