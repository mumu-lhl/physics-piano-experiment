//! A zoom (UI scale) stepper for plugin editors.
//!
//! The control sets the editor's user scale factor, which is persisted by
//! [`ViziaState`][super::ViziaState] across sessions. Applying the new factor
//! needs no further calls from the view: the vizia-plug idle loop notices the
//! changed factor and emits `WindowEvent::SetUserScale`, and the windowing
//! backend then asks the host to resize the parent window to match the new
//! scaled size (hosts that refuse simply keep the old window size).
//!
//! A compact `− value +` stepper (rather than one button per zoom step) keeps
//! the preset bar from overflowing on the narrower editor windows.

use std::sync::Arc;

use vizia::prelude::*;

use super::ViziaState;

/// Zoom steps offered by the control, as scale factors.
pub const ZOOM_FACTORS: [f64; 5] = [0.75, 1.0, 1.25, 1.5, 2.0];

/// Index of the step nearest to `factor`. Used to step up/down from a
/// persisted factor even if it is not exactly one of the offered steps.
fn nearest_step(factor: f64) -> usize {
    ZOOM_FACTORS
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| {
            (*a - factor).abs().partial_cmp(&(*b - factor).abs()).unwrap()
        })
        .map(|(index, _)| index)
        .unwrap_or(1)
}

/// Build a compact zoom stepper: a label, a `−` button, the current zoom
/// percentage, and a `+` button. The end buttons disable at the extremes.
pub fn zoom_control(cx: &mut Context, state: Arc<ViziaState>, label: &str) {
    let label = label.to_owned();
    let current = SyncSignal::new(state.user_scale_factor());
    HStack::new(cx, move |cx| {
        Label::new(cx, label).class("zoom-label");
        let state_down = state.clone();
        let current_down = current;
        Button::new(cx, move |cx| Label::new(cx, "−"))
            .class("btn-zoom")
            .disabled(current.map(|v| nearest_step(*v) == 0))
            .toggle_class(
                "btn-disabled",
                current.map(|v| nearest_step(*v) == 0),
            )
            .on_press(move |_| {
                let next = ZOOM_FACTORS[nearest_step(current_down.get()).saturating_sub(1)];
                current_down.set(next);
                state_down.set_user_scale_factor(next);
            })
            .width(Pixels(28.0));
        Label::new(
            cx,
            current.map(|v| format!("{}%", (*v * 100.0) as u32)),
        )
        .class("zoom-value")
        .width(Pixels(44.0));
        let state_up = state.clone();
        let current_up = current;
        Button::new(cx, move |cx| Label::new(cx, "+"))
            .class("btn-zoom")
            .disabled(current.map(|v| nearest_step(*v) == ZOOM_FACTORS.len() - 1))
            .toggle_class(
                "btn-disabled",
                current.map(|v| nearest_step(*v) == ZOOM_FACTORS.len() - 1),
            )
            .on_press(move |_| {
                let next =
                    ZOOM_FACTORS[(nearest_step(current_up.get()) + 1).min(ZOOM_FACTORS.len() - 1)];
                current_up.set(next);
                state_up.set_user_scale_factor(next);
            })
            .width(Pixels(28.0));
    })
    .class("zoom-control");
}
