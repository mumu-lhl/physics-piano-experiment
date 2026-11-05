//! A parameter knob: a rotary control bound to a plugin parameter.
//!
//! The knob itself is Vizia's built-in [`Knob`] view used as a pure display:
//! it is not hoverable, and it is made non-focusable and non-navigable here,
//! so it never handles input itself and never takes a Tab stop of its own —
//! the wrapper is the knob's single Tab stop. All interaction lives on this
//! wrapper, mirroring [`ParamSlider`]'s gesture discipline:
//!
//! - Vertical drag changes the value (Shift for fine adjustment), bracketed
//!   by [`ParamWidgetBase::begin_set_parameter`] / `end_set_parameter` so the
//!   host records one undo step per drag.
//! - Scroll wheel steps the value, one bracketed gesture per tick.
//! - Double-click resets to the parameter default, bracketed.
//! - Arrow keys step the value when the knob has focus (focus is taken on
//!   mouse-down, like the slider); Shift+arrows step finely, PageUp/PageDown
//!   step by a quarter range, Home/End jump to the ends.
//! - Right-click resets to the parameter default, bracketed (matching the
//!   slider's base behaviour; the richer right-click restore lives in the
//!   physics-ui helpers, not here).
//!
//! Center-detent parameters (those drawn with a centered arc, i.e. whose
//! default sits at the middle of the range) snap to their default while a
//! drag or key step passes within [`DETENT_HALF_WIDTH`] of it; releasing a
//! drag inside the zone leaves exactly the default. See
//! [`snap_to_detent`] / [`snap_key_step`].
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
/// Shift+arrow step: fraction of the full range. 0.01 is smaller than the
/// detent zone width (2 * [`DETENT_HALF_WIDTH`]), so fine steps can never
/// jump over the detent: walking toward center with Shift+arrows always
/// lands on it (see [`snap_key_step`]).
const SHIFT_ARROW_SCALAR: f32 = 0.01;
/// PageUp/PageDown step: a quarter of the full range.
const PAGE_SCALAR: f32 = 0.25;
/// Half-width of the center detent zone, in normalized units. A drag or key
/// step whose value falls within this distance of the detent center snaps
/// to the center exactly. 0.02 is ~5 drag pixels (0.0042/px) / ~4 wheel
/// ticks, ±0.48 dB on the ±12 dB EQs, ±2 points on 0..100% params.
const DETENT_HALF_WIDTH: f32 = 0.02;
/// How close the default must be to the middle of the range for the knob
/// to count as centered (centered arc + detent). Matches the `centered`
/// test already used for the arc.
const CENTERED_TOLERANCE: f32 = 0.05;

/// The detent center for a parameter whose default sits at the middle of
/// its range, or `None` for every other parameter (which is unchanged by
/// the detent). The center is the parameter's own default, not necessarily
/// 0.5: a centered default normalizes to within [`CENTERED_TOLERANCE`] of
/// 0.5, and snapping to the exact default is what makes the plain value
/// land exactly on its neutral point (e.g. 0.0 dB).
fn detent_center_for(default_normalized: f32) -> Option<f32> {
    if (default_normalized - 0.5).abs() < CENTERED_TOLERANCE {
        Some(default_normalized)
    } else {
        None
    }
}

/// Snap `value` to the detent center when it lies within the detent zone.
/// Used for drags: the raw drag position keeps accumulating in
/// `continuous_normal`, so the value pins at the center while the pointer
/// crosses the zone and resumes from the raw position once it leaves it —
/// the detent resists, but never traps, a drag.
fn snap_to_detent(detent_center: Option<f32>, value: f32) -> f32 {
    match detent_center {
        // The 1e-6 slack keeps the zone boundary inclusive despite f32
        // representation error (e.g. 0.48f32 is 0.02000001 from 0.5).
        Some(center) if (value - center).abs() <= DETENT_HALF_WIDTH + 1e-6 => center,
        _ => value,
    }
}

/// Snap a keyboard step to the detent center, except when the step starts
/// exactly at the center: from the center a step must escape (otherwise
/// every fine step out of the center would snap straight back and the
/// center would trap the keyboard). From anywhere else, a step landing in
/// the zone lands exactly on the center, so Shift+arrows (whose 0.01 step
/// cannot jump the 0.04-wide zone) always reach the center.
fn snap_key_step(detent_center: Option<f32>, current: f32, raw: f32) -> f32 {
    match detent_center {
        Some(center) if (current - center).abs() > 1e-6 => snap_to_detent(Some(center), raw),
        _ => raw,
    }
}

/// Snap a wheel step to the detent center, but only when the step moves
/// toward the center: the raw step must land in the zone strictly
/// closer to the center than the value it started from. A plain
/// [`snap_to_detent`] on the emitted value would trap the wheel: the
/// handler re-syncs from the parameter on every tick, so stepping
/// down from exactly the center (0.5 -> 0.495) would snap back to
/// 0.5 on every tick and never escape, and the key-step escape
/// exception alone would oscillate (0.5 -> 0.495 -> snap back to
/// 0.5) for the same reason, because a 0.005 wheel step cannot
/// leave the 0.04-wide zone in one tick. With the toward-center
/// test, a step out of the center escapes immediately, a step
/// further out of the zone never snaps back, and a step into the
/// zone still lands exactly on the center — the wheel passes
/// through the center exactly once and continues past it, in
/// either direction, using the same center and zone as drag/keys.
fn snap_wheel_step(detent_center: Option<f32>, current: f32, raw: f32) -> f32 {
    match detent_center {
        Some(center)
            if (raw - center).abs() <= DETENT_HALF_WIDTH + 1e-6
                && (raw - center).abs() < (current - center).abs() =>
        {
            center
        }
        _ => raw,
    }
}

pub struct ParamKnob {
    param_base: ParamWidgetBase,
    dragging: bool,
    prev_drag_y: f32,
    continuous_normal: f32,
    /// `Some(center)` for center-detent parameters, `None` otherwise.
    detent_center: Option<f32>,
}

impl ParamKnob {
    pub fn new<'c, 'p, P>(cx: &'c mut Context, param: &'p P) -> Handle<'c, Self>
    where
        'p: 'c,
        P: Param + 'static,
    {
        let param_base = ParamWidgetBase::new(param);
        let default_normal = param_base.default_normalized_value();
        Self {
            param_base,
            dragging: false,
            prev_drag_y: 0.0,
            continuous_normal: default_normal,
            detent_center: detent_center_for(default_normal),
        }
        .build(
            cx,
            ParamWidgetBase::build_view(param, move |cx, param_data| {
                let default_normal = param_base.default_normalized_value();
                let centered = detent_center_for(default_normal).is_some();
                Knob::new(
                    cx,
                    default_normal,
                    param_data.unmodulated_signal(cx),
                    centered,
                )
                .hoverable(false)
                // Display only: `Knob::new` makes the inner knob navigable
                // (and focusable), which gave every knob a second Tab stop
                // whose arrow handling bypassed this wrapper's bracketed
                // gestures. `focusable(false)` clears both abilities in
                // Vizia (an unfocusable view cannot be navigable), and
                // `navigable(false)` states the Tab-order intent directly;
                // neither affects rendering. The wrapper (made navigable
                // by the editor-side cell) remains the single Tab stop.
                .focusable(false)
                .navigable(false)
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
        // The raw position keeps accumulating in `continuous_normal`; only
        // the emitted value snaps, so releasing inside the detent zone
        // leaves exactly the center while the next drag re-syncs from it.
        let emitted = snap_to_detent(self.detent_center, self.continuous_normal);
        self.param_base.set_normalized_value(cx, emitted);
    }

    /// One bracketed keyboard step of `delta` normalized units from the
    /// current value, with detent snapping (see [`snap_key_step`]).
    fn apply_key_step(&mut self, cx: &mut EventContext, delta: f32) {
        let current = self.param_base.unmodulated_normalized_value();
        let raw = (current + delta).clamp(0.0, 1.0);
        let next = snap_key_step(self.detent_center, current, raw);
        self.continuous_normal = next;
        self.bracketed_set(cx, next);
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

    fn accessibility(&self, _cx: &mut AccessContext, node: &mut AccessNode) {
        // Numeric range for the 0..100 value bound by the editor-side
        // `.numeric_value(...)` modifier (mirrors Vizia's built-in Knob).
        node.set_min_numeric_value(0.0);
        node.set_max_numeric_value(100.0);
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
                    let raw = (self.continuous_normal - delta).clamp(0.0, 1.0);
                    // Same detent center/zone as drag/keys, via the
                    // wheel-specific snap (see `snap_wheel_step`): the
                    // handler re-syncs from the parameter each tick,
                    // so a plain snap would trap steps out of the
                    // center.
                    let next = snap_wheel_step(self.detent_center, self.continuous_normal, raw);
                    self.continuous_normal = next;
                    self.bracketed_set(cx, next);
                    meta.consume();
                }
            }
            WindowEvent::MouseDoubleClick(MouseButton::Left) => {
                // Do NOT clear dragging here: the second press already began
                // a gesture, and the following MouseUp ends it. Clearing
                // would orphan that gesture (begin without end) and leak
                // the pointer capture; the bracketed reset below is
                // self-contained and leaves the drag balanced.
                self.bracketed_set(cx, self.param_base.default_normalized_value());
                meta.consume();
            }
            WindowEvent::MouseDown(MouseButton::Right) => {
                self.bracketed_set(cx, self.param_base.default_normalized_value());
                meta.consume();
            }
            WindowEvent::KeyDown(Code::ArrowUp | Code::ArrowRight, _) => {
                // Plain arrows keep the long-standing ±0.1 step; Shift
                // refines it to ±0.01 (see SHIFT_ARROW_SCALAR).
                let step = if cx.modifiers().shift() {
                    SHIFT_ARROW_SCALAR
                } else {
                    ARROW_SCALAR
                };
                self.apply_key_step(cx, step);
                meta.consume();
            }
            WindowEvent::KeyDown(Code::ArrowDown | Code::ArrowLeft, _) => {
                let step = if cx.modifiers().shift() {
                    SHIFT_ARROW_SCALAR
                } else {
                    ARROW_SCALAR
                };
                self.apply_key_step(cx, -step);
                meta.consume();
            }
            WindowEvent::KeyDown(Code::PageUp, _) => {
                // A quarter of the range; Shift does not refine paging.
                self.apply_key_step(cx, PAGE_SCALAR);
                meta.consume();
            }
            WindowEvent::KeyDown(Code::PageDown, _) => {
                self.apply_key_step(cx, -PAGE_SCALAR);
                meta.consume();
            }
            WindowEvent::KeyDown(Code::Home, _) => {
                self.bracketed_set(cx, 0.0);
                meta.consume();
            }
            WindowEvent::KeyDown(Code::End, _) => {
                self.bracketed_set(cx, 1.0);
                meta.consume();
            }
            WindowEvent::FocusIn | WindowEvent::FocusOut => {
                // Repaint on focus change so the knob head's border
                // (gold while keyboard-focused, see the
                // `param-knob:focus-visible knob .knob-head` rule)
                // follows focus: the head is repainted as part of this
                // view's subtree, which is what erases the previous
                // focus indication. Not consumed: other listeners may
                // also need the event.
                cx.needs_redraw();
            }
            _ => {}
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detent_center_only_for_mid_range_defaults() {
        // EQ-style default: exactly centered.
        assert_eq!(detent_center_for(0.5), Some(0.5));
        // Within tolerance: still centered (arc + detent).
        assert_eq!(detent_center_for(0.54), Some(0.54));
        assert_eq!(detent_center_for(0.46), Some(0.46));
        // Outside tolerance: no detent (e.g. master gain ~0.80, tone 0.72).
        assert_eq!(detent_center_for(0.55), None);
        assert_eq!(detent_center_for(0.72), None);
        assert_eq!(detent_center_for(0.0), None);
        assert_eq!(detent_center_for(1.0), None);
    }

    #[test]
    fn drag_detent_snaps_inside_zone_only() {
        let center = Some(0.5);
        // Inside the ±0.02 zone: exactly the center.
        assert_eq!(snap_to_detent(center, 0.49), 0.5);
        assert_eq!(snap_to_detent(center, 0.51), 0.5);
        // Zone boundary is inclusive.
        assert_eq!(snap_to_detent(center, 0.48), 0.5);
        assert_eq!(snap_to_detent(center, 0.52), 0.5);
        // Outside: untouched, so a drag resumes from its raw position.
        assert_eq!(snap_to_detent(center, 0.47), 0.47);
        assert_eq!(snap_to_detent(center, 0.53), 0.53);
        // Non-centered parameters are never snapped.
        assert_eq!(snap_to_detent(None, 0.49), 0.49);
        assert_eq!(snap_to_detent(None, 0.5), 0.5);
    }

    #[test]
    fn key_steps_land_on_center_and_escape_it() {
        let center = Some(0.5);
        // Plain arrows landing exactly on the grid center stay exact.
        assert_eq!(snap_key_step(center, 0.4, 0.5), 0.5);
        // A fine step into the zone lands exactly on the center.
        assert_eq!(snap_key_step(center, 0.53, 0.52), 0.5);
        assert_eq!(snap_key_step(center, 0.47, 0.48), 0.5);
        // A step starting exactly at the center escapes: the center must
        // not trap the keyboard.
        assert_eq!(snap_key_step(center, 0.5, 0.51), 0.51);
        assert_eq!(snap_key_step(center, 0.5, 0.49), 0.49);
        assert_eq!(snap_key_step(center, 0.5, 0.6), 0.6);
        // A step that stays outside the zone is untouched.
        assert_eq!(snap_key_step(center, 0.6, 0.7), 0.7);
        // No detent: untouched.
        assert_eq!(snap_key_step(None, 0.53, 0.52), 0.52);
    }

    #[test]
    fn shift_arrow_walk_always_reaches_center() {
        // Fine steps (0.01) cannot jump the 0.04-wide detent zone, so a
        // walk from an off-grid value reaches exactly 0.5 — the case
        // plain ±0.1 arrows alone cannot reach.
        let center = Some(0.5);
        let mut value = 0.523f32;
        for _ in 0..20 {
            let raw = (value - SHIFT_ARROW_SCALAR).clamp(0.0, 1.0);
            value = snap_key_step(center, value, raw);
            if value == 0.5 {
                break;
            }
        }
        assert_eq!(value, 0.5);

        let mut value = 0.477f32;
        for _ in 0..20 {
            let raw = (value + SHIFT_ARROW_SCALAR).clamp(0.0, 1.0);
            value = snap_key_step(center, value, raw);
            if value == 0.5 {
                break;
            }
        }
        assert_eq!(value, 0.5);
    }

    #[test]
    fn key_step_sizes_cover_the_range() {
        // Plain arrows: 10 steps end to end. Pages: 4 steps end to end.
        assert!((ARROW_SCALAR * 10.0 - 1.0).abs() < 1e-6);
        assert!((PAGE_SCALAR * 4.0 - 1.0).abs() < 1e-6);
        // Fine steps are strictly smaller than the detent zone width.
        assert!(SHIFT_ARROW_SCALAR < 2.0 * DETENT_HALF_WIDTH);
    }

    #[test]
    fn wheel_step_snaps_only_toward_the_center() {
        let center = Some(0.5);
        // Approaching the center from either side lands on it.
        assert_eq!(snap_wheel_step(center, 0.53, 0.52), 0.5);
        assert_eq!(snap_wheel_step(center, 0.47, 0.48), 0.5);
        // Leaving the center escapes in either direction: no snap-back.
        assert_eq!(snap_wheel_step(center, 0.5, 0.505), 0.505);
        assert_eq!(snap_wheel_step(center, 0.5, 0.495), 0.495);
        // Already inside the zone but moving away keeps moving away:
        // the next tick must not snap back to the center.
        assert_eq!(snap_wheel_step(center, 0.495, 0.49), 0.49);
        assert_eq!(snap_wheel_step(center, 0.505, 0.51), 0.51);
        // A step that stays outside the zone is untouched.
        assert_eq!(snap_wheel_step(center, 0.6, 0.595), 0.595);
        // No detent: untouched.
        assert_eq!(snap_wheel_step(None, 0.53, 0.52), 0.52);
    }

    #[test]
    fn wheel_walk_passes_through_center_exactly_once_and_continues() {
        // Wheel steps (0.005) are smaller than the detent zone
        // width, so a naive snap would trap the walk; the
        // toward-center snap must let a repeated walk pass through
        // the center exactly once and continue past it, in either
        // direction, from either side — including starting exactly
        // at the center, where the walk must escape and keep going
        // instead of oscillating.
        assert!(WHEEL_SCALAR < 2.0 * DETENT_HALF_WIDTH);
        let center = Some(0.5);

        // Wheel-up from below the center.
        let mut value = 0.46f32;
        let mut center_hits = 0;
        for _ in 0..30 {
            let raw = (value + WHEEL_SCALAR).clamp(0.0, 1.0);
            value = snap_wheel_step(center, value, raw);
            if value == 0.5 {
                center_hits += 1;
            }
            if value > 0.54 {
                break;
            }
        }
        assert_eq!(center_hits, 1);
        assert!(value > 0.54);

        // Wheel-down from above the center.
        let mut value = 0.54f32;
        let mut center_hits = 0;
        for _ in 0..30 {
            let raw = (value - WHEEL_SCALAR).clamp(0.0, 1.0);
            value = snap_wheel_step(center, value, raw);
            if value == 0.5 {
                center_hits += 1;
            }
            if value < 0.46 {
                break;
            }
        }
        assert_eq!(center_hits, 1);
        assert!(value < 0.46);

        // Wheel-down starting exactly at the center: escapes and
        // keeps going (12 ticks clear the zone entirely).
        let mut value = 0.5f32;
        for _ in 0..12 {
            let raw = (value - WHEEL_SCALAR).clamp(0.0, 1.0);
            value = snap_wheel_step(center, value, raw);
        }
        assert!(value < 0.46);

        // Wheel-up starting exactly at the center: same, upward.
        let mut value = 0.5f32;
        for _ in 0..12 {
            let raw = (value + WHEEL_SCALAR).clamp(0.0, 1.0);
            value = snap_wheel_step(center, value, raw);
        }
        assert!(value > 0.54);
    }
}
