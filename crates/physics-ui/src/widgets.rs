use nice_plug::prelude::{Param, ParamPtr};
use physics_presets::PresetManager;
use std::sync::Arc;
use vizia_plug::vizia::prelude::*;
use vizia_plug::widgets::*;

use crate::Language;

#[derive(Default)]
struct SliderRestoreState {
    active_start: Option<f32>,
    completed_start: Option<f32>,
    scrolled_lines: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct CancelParamGestureEvent {
    pub param: ParamPtr,
    pub restore_normalized: f32,
}

/// Marks a wheel update for history coalescing while preserving host gesture boundaries.
#[derive(Debug, Clone, Copy)]
struct WheelGestureEnd(ParamPtr);

/// Delivers host parameter events and wheel completion events to the history recorder.
pub fn map_param_history_event(event: &mut Event, mut callback: impl FnMut(&RawParamEvent, bool)) {
    event.map(|raw: &RawParamEvent, _| callback(raw, false));
    event.map(|wheel: &WheelGestureEnd, _| {
        callback(&RawParamEvent::EndSetParameter(wheel.0), true);
    });
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct RestorePoint {
    normalized: f32,
    completed: bool,
}

impl SliderRestoreState {
    fn begin(&mut self, value: f32) {
        self.active_start = Some(value);
        self.completed_start = None;
    }

    fn finish(&mut self) {
        if let Some(value) = self.active_start.take() {
            self.completed_start = Some(value);
        }
    }

    fn take_restore_value(&mut self) -> Option<RestorePoint> {
        let point = if let Some(normalized) = self.active_start.take() {
            Some(RestorePoint {
                normalized,
                completed: false,
            })
        } else {
            self.completed_start.take().map(|normalized| RestorePoint {
                normalized,
                completed: true,
            })
        };
        self.active_start = None;
        self.completed_start = None;
        point
    }
}

/// Selects a localized string without allocating.
pub fn ui_text(lang: Language, english: &'static str, chinese: &'static str) -> &'static str {
    match lang {
        Language::English => english,
        Language::SimplifiedChinese => chinese,
    }
}

/// Builds the shared instrument parameter slider, including right-click restore.
pub fn parameter_slider<P: Param + 'static>(
    cx: &mut Context,
    label: &'static str,
    param: &P,
    label_width: f32,
) {
    let param_ptr = param.as_ptr();
    HStack::new(cx, move |cx| {
        Label::new(cx, label).width(Pixels(label_width));
        let mut slider = ParamSlider::new(cx, param)
            .disable_scroll_wheel()
            .width(Stretch(1.0))
            .height(Pixels(20.0));
        let restore_state = Arc::new(parking_lot::Mutex::new(SliderRestoreState::default()));
        let slider_entity = slider.entity();
        slider.context().with_current(slider_entity, |cx| {
            cx.add_listener(
                move |_: &mut ParamSlider, cx: &mut EventContext, event: &mut Event| {
                    event.map(|window, meta| match window {
                        WindowEvent::MouseDown(MouseButton::Left) => {
                            let bounds = cx.bounds();
                            let mouse = cx.mouse();
                            let inside = mouse.cursor_x >= bounds.x
                                && mouse.cursor_x <= bounds.x + bounds.w
                                && mouse.cursor_y >= bounds.y
                                && mouse.cursor_y <= bounds.y + bounds.h;
                            if inside {
                                restore_state
                                    .lock()
                                    .begin(unsafe { param_ptr.unmodulated_normalized_value() });
                            }
                        }
                        WindowEvent::MouseUp(MouseButton::Left) => {
                            restore_state.lock().finish();
                        }
                        WindowEvent::MouseScroll(_, scroll_y) => {
                            let bounds = cx.bounds();
                            let mouse = cx.mouse();
                            let inside = mouse.cursor_x >= bounds.x
                                && mouse.cursor_x <= bounds.x + bounds.w
                                && mouse.cursor_y >= bounds.y
                                && mouse.cursor_y <= bounds.y + bounds.h;
                            if !inside {
                                return;
                            }

                            let finer_steps = cx.modifiers().shift();
                            let mut state = restore_state.lock();
                            state.scrolled_lines += scroll_y;

                            let initial = unsafe { param_ptr.unmodulated_normalized_value() };
                            let mut normalized = initial;

                            while state.scrolled_lines >= 1.0 {
                                let next = unsafe {
                                    param_ptr.next_normalized_step(normalized, finer_steps)
                                };
                                state.scrolled_lines -= 1.0;
                                apply_wheel_step(param_ptr, &mut normalized, next);
                            }

                            while state.scrolled_lines <= -1.0 {
                                let next = unsafe {
                                    param_ptr.previous_normalized_step(normalized, finer_steps)
                                };
                                state.scrolled_lines += 1.0;
                                apply_wheel_step(param_ptr, &mut normalized, next);
                            }

                            let changed = (normalized - initial).abs() > f32::EPSILON;
                            let dragging = state.active_start.is_some();
                            if changed {
                                state.completed_start = None;
                                if !dragging {
                                    cx.emit(RawParamEvent::BeginSetParameter(param_ptr));
                                }
                                cx.emit(RawParamEvent::SetParameterNormalized(
                                    param_ptr, normalized,
                                ));
                            }
                            if !dragging {
                                // Even fractional deltas and movement against a limit keep a
                                // continuous scroll burst alive in the history recorder.
                                cx.emit(WheelGestureEnd(param_ptr));
                                if changed {
                                    cx.emit(RawParamEvent::EndSetParameter(param_ptr));
                                }
                            }

                            // A hovered parameter slider owns the wheel, including fractional
                            // trackpad deltas that have not produced a parameter step yet.
                            meta.consume();
                        }
                        WindowEvent::MouseDown(MouseButton::Right) => {
                            let bounds = cx.bounds();
                            let mouse = cx.mouse();
                            if mouse.cursor_x < bounds.x
                                || mouse.cursor_x > bounds.x + bounds.w
                                || mouse.cursor_y < bounds.y
                                || mouse.cursor_y > bounds.y + bounds.h
                            {
                                return;
                            }
                            if let Some(point) = restore_state.lock().take_restore_value() {
                                if point.completed {
                                    // Discard the completed drag's undo record, then restore the
                                    // parameter inside a properly bracketed host gesture.
                                    cx.emit(CancelParamGestureEvent {
                                        param: param_ptr,
                                        restore_normalized: point.normalized,
                                    });
                                    cx.emit(RawParamEvent::BeginSetParameter(param_ptr));
                                    cx.emit(RawParamEvent::SetParameterNormalized(
                                        param_ptr,
                                        point.normalized,
                                    ));
                                    cx.emit(RawParamEvent::EndSetParameter(param_ptr));
                                } else {
                                    cx.emit(RawParamEvent::SetParameterNormalized(
                                        param_ptr,
                                        point.normalized,
                                    ));
                                    // Consume ParamSlider's default reset and end the active drag.
                                    cx.emit_custom(
                                        Event::new(WindowEvent::MouseUp(MouseButton::Left))
                                            .target(cx.current())
                                            .propagate(Propagation::Direct),
                                    );
                                }
                                meta.consume();
                            }
                        }
                        _ => {}
                    });
                },
            );
        });
    })
    .height(Pixels(23.0))
    .horizontal_gap(Pixels(6.0));
}

fn apply_wheel_step(param_ptr: ParamPtr, current_normalized: &mut f32, requested_normalized: f32) {
    let next_normalized = unsafe {
        let plain_value = param_ptr.preview_plain(requested_normalized);
        param_ptr.preview_normalized(plain_value)
    };

    if (next_normalized - *current_normalized).abs() <= f32::EPSILON {
        return;
    }

    *current_normalized = next_normalized;
}

/// Builds a discrete selector and delegates its parameter update to the caller.
pub fn discrete_selector<F>(
    cx: &mut Context,
    label: &'static str,
    selected: Signal<i32>,
    options: Vec<(i32, String)>,
    label_width: f32,
    on_select: F,
) where
    F: Fn(&mut EventContext, i32) + Clone + Send + Sync + 'static,
{
    let trigger_options = options.clone();
    HStack::new(cx, move |cx| {
        Label::new(cx, label).width(Pixels(label_width));
        Dropdown::new(
            cx,
            move |cx| {
                let display_options = trigger_options.clone();
                Button::new(cx, move |cx| {
                    HStack::new(cx, move |cx| {
                        Label::new(
                            cx,
                            selected.map(move |value| {
                                display_options
                                    .iter()
                                    .find(|(option, _)| *option == *value)
                                    .map(|(_, name)| name.clone())
                                    .unwrap_or_default()
                            }),
                        )
                        .width(Stretch(1.0))
                        .text_overflow(TextOverflow::Ellipsis);
                        Label::new(cx, "▾").hoverable(false);
                    })
                    .width(Stretch(1.0))
                })
                .on_press(|cx| cx.emit(PopupEvent::Switch))
                .width(Stretch(1.0))
                .height(Pixels(22.0));
            },
            move |cx| {
                for (value, name) in options.clone() {
                    let on_select = on_select.clone();
                    Button::new(cx, move |cx| {
                        Label::new(cx, name.clone())
                            .alignment(Alignment::Left)
                            .width(Stretch(1.0))
                    })
                    .on_press(move |cx| {
                        on_select(cx, value);
                        cx.emit(PopupEvent::Close);
                    })
                    .width(Stretch(1.0));
                }
            },
        )
        .width(Stretch(1.0));
    })
    .height(Pixels(23.0))
    .horizontal_gap(Pixels(6.0));
}

/// Emits begin/set/end events so parameter changes participate in host automation.
pub fn set_param<P: Param>(cx: &mut EventContext, param: &P, value: P::Plain) {
    cx.emit(ParamEvent::BeginSetParameter(param).upcast());
    cx.emit(ParamEvent::SetParameter(param, value).upcast());
    cx.emit(ParamEvent::EndSetParameter(param).upcast());
}

/// Builds the display choices from the shared preset manager.
pub fn preset_choices(manager: &PresetManager, lang: Language) -> Vec<(String, String)> {
    manager
        .presets()
        .iter()
        .map(|preset| {
            (
                preset.id.clone(),
                preset
                    .display_name(lang == Language::SimplifiedChinese)
                    .to_string(),
            )
        })
        .collect()
}

/// Requests a redraw of a named custom Vizia view, if it exists.
pub fn redraw_custom_view(cx: &mut EventContext, element: &str) {
    if let Some(entity) = cx.get_entity_by_element_id(element) {
        cx.with_current(entity, |cx| cx.needs_redraw());
    }
}

#[cfg(test)]
mod tests {
    use super::SliderRestoreState;

    #[test]
    fn right_click_restores_value_from_the_last_completed_adjustment() {
        let mut state = SliderRestoreState::default();
        state.begin(0.25);
        state.finish();
        assert_eq!(
            state.take_restore_value(),
            Some(super::RestorePoint {
                normalized: 0.25,
                completed: true,
            })
        );
    }

    #[test]
    fn right_click_during_an_active_adjustment_restores_its_start() {
        let mut state = SliderRestoreState::default();
        state.begin(0.4);
        assert_eq!(
            state.take_restore_value(),
            Some(super::RestorePoint {
                normalized: 0.4,
                completed: false,
            })
        );
        assert_eq!(state.take_restore_value(), None);
    }

    #[test]
    fn beginning_a_new_adjustment_discards_the_previous_restore_point() {
        let mut state = SliderRestoreState::default();
        state.begin(0.25);
        state.finish();
        state.begin(0.6);
        state.finish();
        assert_eq!(
            state.take_restore_value(),
            Some(super::RestorePoint {
                normalized: 0.6,
                completed: true,
            })
        );
        assert_eq!(state.take_restore_value(), None);
    }
}
