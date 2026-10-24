//! A parameter knob: a rotary control bound to a plugin parameter.
//!
//! The knob itself is Vizia's built-in [`Knob`] view used as a pure display:
//! it is not hoverable, so it never handles input itself. All interaction
//! lives on this wrapper, mirroring [`ParamSlider`]'s gesture discipline:
//!
//! - Vertical drag changes the value (Shift for fine adjustment), bracketed
//!   by [`ParamWidgetBase::begin_set_parameter`] / `end_set_parameter` so the
//!   host records one undo step per drag.
//! - Scroll wheel steps the value, one bracketed gesture per tick.
//! - Double-click resets to the parameter default, bracketed.
//! - Arrow keys step the value when the knob has focus (focus is taken on
//!   mouse-down, like the slider).
//! - Right-click resets to the parameter default, bracketed (matching the
//!   slider's base behaviour; the richer right-click restore lives in the
//!   physics-ui helpers, not here).
//!
//! Value snapping for stepped parameters happens in
//! [`ParamWidgetBase::set_normalized_value`], so drag and wheel code can pass
//! continuous values.

use nice_plug_core::params::Param;
use vizia::prelude::*;

use super::param_base::ParamWidgetBase;

/// Drag sensitivity: fraction of the full range per vertical pixel, matching
/// Vizia's knob default. Shift multiplies it by [`FINE_DRAG_SCALAR`].
const DRAG_SCALAR: f32 = 0.0042;
/// Fine-drag multiplier while Shift is held, matching Vizia's knob default.
const FINE_DRAG_SCALAR: f32 = 0.04;
/// Wheel sensitivity: fraction of the full range per scroll unit, matching
/// Vizia's knob default.
const WHEEL_SCALAR: f32 = 0.005;
/// Arrow-key step: fraction of the full range, matching Vizia's knob default.
const ARROW_SCALAR: f32 = 0.1;

pub struct ParamKnob {
    param_base: ParamWidgetBase,
    dragging: bool,
    prev_drag_y: f32,
    continuous_normal: f32,
}

impl ParamKnob {
    pub fn new<'c, 'p, P>(cx: &'c mut Context, param: &'p P) -> Handle<'c, Self>
    where
        'p: 'c,
        P: Param + 'static,
    {
        let param_base = ParamWidgetBase::new(cx, param);
        Self {
            param_base,
            dragging: false,
            prev_drag_y: 0.0,
            continuous_normal: param_base.default_normalized_value(),
        }
        .build(
            cx,
            ParamWidgetBase::build_view(param, move |cx, param_data| {
                let default_normal = param_base.default_normalized_value();
                let centered = (default_normal - 0.5).abs() < 0.05;
                Knob::new(
                    cx,
                    default_normal,
                    param_data.unmodulated_signal(cx),
                    centered,
                )
                .hoverable(false)
                .class("knob-display")
                .width(Percentage(100.0))
                .height(Percentage(100.0));
            }),
        )
    }

    fn apply_drag(&mut self, cx: &mut EventContext, cursor_y: f32) {
        let mut delta = (cursor_y - self.prev_drag_y) * DRAG_SCALAR;
        self.prev_drag_y = cursor_y;
        if cx.modifiers().shift() {
            delta *= FINE_DRAG_SCALAR;
        }
        self.continuous_normal = (self.continuous_normal - delta).clamp(0.0, 1.0);
        self.param_base
            .set_normalized_value(cx, self.continuous_normal);
    }

    fn bracketed_set(&mut self, cx: &mut EventContext, normalized: f32) {
        self.param_base.begin_set_parameter(cx);
        self.param_base.set_normalized_value(cx, normalized);
        self.param_base.end_set_parameter(cx);
    }
}

impl View for ParamKnob {
    fn element(&self) -> Option<&'static str> {
        Some("param-knob")
    }

    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|window_event, meta| match window_event {
            WindowEvent::MouseDown(MouseButton::Left) => {
                if !self.dragging {
                    self.dragging = true;
                    self.prev_drag_y = cx.mouse().cursor_y;
                    self.continuous_normal =
                        self.param_base.unmodulated_normalized_value();
                    cx.capture();
                    cx.focus();
                    self.param_base.begin_set_parameter(cx);
                    meta.consume();
                }
            }
            WindowEvent::MouseMove(_, y) => {
                if self.dragging {
                    self.apply_drag(cx, *y);
                    meta.consume();
                }
            }
            WindowEvent::MouseUp(MouseButton::Left) => {
                if self.dragging {
                    self.dragging = false;
                    cx.release();
                    self.param_base.end_set_parameter(cx);
                    meta.consume();
                }
            }
            WindowEvent::MouseScroll(_, y) => {
                if *y != 0.0 {
                    self.continuous_normal =
                        self.param_base.unmodulated_normalized_value();
                    let delta = -*y * WHEEL_SCALAR;
                    let next = (self.continuous_normal - delta).clamp(0.0, 1.0);
                    self.continuous_normal = next;
                    self.bracketed_set(cx, next);
                    meta.consume();
                }
            }
            WindowEvent::MouseDoubleClick(MouseButton::Left) => {
                self.dragging = false;
                self.bracketed_set(cx, self.param_base.default_normalized_value());
                meta.consume();
            }
            WindowEvent::MouseDown(MouseButton::Right) => {
                self.bracketed_set(cx, self.param_base.default_normalized_value());
                meta.consume();
            }
            WindowEvent::KeyDown(Code::ArrowUp | Code::ArrowRight, _) => {
                self.continuous_normal =
                    self.param_base.unmodulated_normalized_value();
                let next = (self.continuous_normal + ARROW_SCALAR).clamp(0.0, 1.0);
                self.continuous_normal = next;
                self.bracketed_set(cx, next);
                meta.consume();
            }
            WindowEvent::KeyDown(Code::ArrowDown | Code::ArrowLeft, _) => {
                self.continuous_normal =
                    self.param_base.unmodulated_normalized_value();
                let next = (self.continuous_normal - ARROW_SCALAR).clamp(0.0, 1.0);
                self.continuous_normal = next;
                self.bracketed_set(cx, next);
                meta.consume();
            }
            _ => {}
        });
    }
}
