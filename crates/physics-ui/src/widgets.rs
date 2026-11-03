use nice_plug::prelude::{Param, ParamPtr};
use physics_presets::{Preset, PresetManager};
use std::sync::Arc;
use vizia_plug::vizia::prelude::*;
use vizia_plug::widgets::param_base::ParamWidgetBase;
use vizia_plug::widgets::*;

use crate::midi_learn::LearnArm;
use crate::preset_panel::attach_dropdown_keyboard_nav;
use crate::{Language, translate};

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

/// Builds the shared instrument parameter slider, including right-click restore.
/// No MIDI Learn arming: the only shipped caller is the deprecated drum
/// editor; instruments that want Learn on a slider use
/// [`parameter_slider_learn`].
pub fn parameter_slider<P: Param + 'static>(
    cx: &mut Context,
    label: &'static str,
    param: &P,
    label_width: f32,
) {
    parameter_slider_learn(cx, label, param, label_width, None);
}

/// Builds the shared instrument parameter slider, including right-click restore,
/// with optional MIDI Learn arming following the `parameter_knob` pattern:
/// in Learn mode a left-click arms the parameter (no drag) and a right-click
/// queues a binding clear (no restore, no undo record).
pub fn parameter_slider_learn<P: Param + 'static>(
    cx: &mut Context,
    label: &'static str,
    param: &P,
    label_width: f32,
    learn: Option<LearnArm>,
) {
    let param_ptr = param.as_ptr();
    let slider_base = ParamWidgetBase::new(cx, param);
    let slider_value = slider_base.unmodulated_signal(cx);
    HStack::new(cx, move |cx| {
        Label::new(cx, label).width(Pixels(label_width));
        let mut slider = ParamSlider::new(cx, param)
            .disable_scroll_wheel()
            .width(Stretch(1.0))
            .height(Pixels(20.0))
            // Tab-reachable with the same accessible name/role/value
            // as the knobs (see `parameter_knob`). Arrow-key stepping
            // is deliberately NOT added here: the slider owns an
            // inline text-entry textbox, and arrow keys must keep
            // editing text while it is active.
            .navigable(true)
            .role(Role::Slider)
            .name(label.to_string())
            .numeric_value(slider_value.map(|v| (*v as f64 * 100.0).round()));
        let restore_state = Arc::new(parking_lot::Mutex::new(SliderRestoreState::default()));
        let slider_entity = slider.entity();
        slider.context().with_current(slider_entity, |cx| {
            cx.add_listener(
                move |_: &mut ParamSlider, cx: &mut EventContext, event: &mut Event| {
                    event.map(|window, meta| match window {
                        WindowEvent::MouseDown(MouseButton::Left) => {
                            // Bounds first: listeners see every event in
                            // the tree, so an unguarded arm() here would
                            // arm a hash-order-random slider for a click
                            // anywhere in the window. Only a click
                            // inside this slider arms it, mirroring
                            // parameter_knob.
                            let bounds = cx.bounds();
                            let mouse = cx.mouse();
                            let inside = mouse.cursor_x >= bounds.x
                                && mouse.cursor_x <= bounds.x + bounds.w
                                && mouse.cursor_y >= bounds.y
                                && mouse.cursor_y <= bounds.y + bounds.h;
                            if let Some(learn) = &learn {
                                if inside && learn.arm(param_ptr) {
                                    meta.consume();
                                    return;
                                }
                            }
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
                            // Same bounds gate as the left-click arm
                            // above: only a right-click inside this
                            // slider queues its clear in Learn mode.
                            // In Learn mode a right-click only queues a
                            // binding clear and must not restore the
                            // value or write an undo record — including
                            // when nothing is bound, in which case the
                            // click is consumed silently.
                            let bounds = cx.bounds();
                            let mouse = cx.mouse();
                            if mouse.cursor_x < bounds.x
                                || mouse.cursor_x > bounds.x + bounds.w
                                || mouse.cursor_y < bounds.y
                                || mouse.cursor_y > bounds.y + bounds.h
                            {
                                return;
                            }
                            if let Some(learn) = &learn {
                                if learn.request_clear(param_ptr) {
                                    meta.consume();
                                    return;
                                }
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

/// Parse typed knob-entry text through the parameter's own parser and
/// clamp to the unit range. The parser accepts the formatted display
/// value with its unit ("-3 dB", "70 %") or a bare number; anything
/// else yields `None` and the entry changes nothing.
fn parse_knob_entry(param_base: ParamWidgetBase, text: &str) -> Option<f32> {
    param_base
        .string_to_normalized_value(text)
        .map(|normalized| normalized.clamp(0.0, 1.0))
}

/// Builds a compact rotary parameter control: label above, knob, formatted
/// value readout below. The knob itself is 40px; `cell_width` sets the cell
/// (piano/bass groups use 120px for two cells per row, guitar uses 84px for
/// three). Right-click restore mirrors the slider: during a drag it restores
/// the pre-drag value and ends the drag; after a completed drag it discards
/// that drag's undo record and restores inside a bracketed gesture.
///
/// Double-clicking the readout opens text entry (the slider's
/// Alt+click pattern, moved to the readout so the knob's own
/// double-click keeps resetting to the default): type the parameter's
/// formatted value with units, Enter commits inside one bracketed
/// gesture (one undo entry, clamped to range), Escape cancels. While
/// MIDI Learn mode is on, readout double-clicks do nothing — clicks
/// belong to arming. When a knob is the pending Learn target its cell
/// gets the `knob-learn-armed` class, and a bound knob's readout
/// carries its `CCn` suffix (see `LearnArm::bindings`).
pub fn parameter_knob<P: Param + 'static>(
    cx: &mut Context,
    label: &'static str,
    param: &P,
    cell_width: f32,
    tooltip: &'static str,
    learn: Option<LearnArm>,
) {
    let param_base = ParamWidgetBase::new(cx, param);
    let unmodulated = param_base.unmodulated_signal(cx);
    let display_value: Memo<String> = Memo::new(move |_| {
        let current = unmodulated.get();
        param_base.normalized_value_to_string(current, true)
    });
    // Knob labels sit above the knob with the value below, so the trailing
    // colon the slider rows used as a separator is dropped for display.
    let label = label.strip_suffix(':').unwrap_or(label);
    let param_ptr = param.as_ptr();
    // Learn display state, resolved once from the shared arm: the
    // parameter id for the armed highlight and the bound-CC badge.
    let param_id: Option<String> = learn
        .as_ref()
        .and_then(|arm| arm.ptr_to_id.get(&param_ptr).cloned());
    let armed_learn = learn.clone();
    let armed_param_id = param_id.clone();
    let armed: Memo<bool> = Memo::new(move |_| match &armed_learn {
        Some(arm) => arm.mode.get() && arm.pending.get().as_deref() == armed_param_id.as_deref(),
        None => false,
    });
    let badge_learn = learn.clone();
    let badge_param_id = param_id.clone();
    let readout_text: Memo<String> = Memo::new(move |_| {
        let text = display_value.get();
        if let (Some(arm), Some(id)) = (&badge_learn, &badge_param_id) {
            if let Some(cc) = arm.bindings.get().get(id) {
                return format!("{text}  CC{cc}");
            }
        }
        text
    });
    // Whether this cell's readout is showing the text-entry textbox.
    let editing = Signal::new(false);
    let learn_mode = learn.as_ref().map(|arm| arm.mode.clone());
    VStack::new(cx, move |cx| {
        Label::new(cx, label).class("knob-label");
        let mut knob = ParamKnob::new(cx, param)
            .width(Pixels(40.0))
            .height(Pixels(40.0))
            // Tab-reachable: ParamKnob already steps on arrow keys once
            // focused (focus is taken on mouse-down); `focusable`
            // alone does not put a view in the Tab order — only
            // `navigable` does. The focus ring is drawn by the
            // `param-knob:focus-visible knob .knob-head` rule in
            // vizia-plug's widgets.css. The accessible
            // name is the visible label (already localized at the
            // call site) and the numeric value tracks the live
            // parameter on Vizia's 0..100 slider scale.
            .navigable(true)
            .role(Role::Slider)
            .name(label.to_string())
            .numeric_value(unmodulated.map(|v| (*v as f64 * 100.0).round()));
        let restore_state = Arc::new(parking_lot::Mutex::new(SliderRestoreState::default()));
        let knob_entity = knob.entity();
        knob.context().with_current(knob_entity, |cx| {
            cx.add_listener(
                move |_: &mut ParamKnob, cx: &mut EventContext, event: &mut Event| {
                    event.map(|window, meta| match window {
                        WindowEvent::MouseDown(MouseButton::Left) => {
                            // Bounds first: listeners see every event in
                            // the tree, so an unguarded arm() here would
                            // arm a hash-order-random knob for a click
                            // anywhere in the window. Only a click
                            // inside this knob arms it; in Learn mode
                            // such a click only arms and must not start
                            // a drag gesture.
                            let bounds = cx.bounds();
                            let mouse = cx.mouse();
                            let inside = mouse.cursor_x >= bounds.x
                                && mouse.cursor_x <= bounds.x + bounds.w
                                && mouse.cursor_y >= bounds.y
                                && mouse.cursor_y <= bounds.y + bounds.h;
                            if let Some(learn) = &learn {
                                if inside && learn.arm(param_ptr) {
                                    meta.consume();
                                    return;
                                }
                            }
                            if inside {
                                restore_state
                                    .lock()
                                    .begin(unsafe { param_ptr.unmodulated_normalized_value() });
                            }
                        }
                        WindowEvent::MouseUp(MouseButton::Left) => {
                            restore_state.lock().finish();
                        }
                        WindowEvent::MouseDown(MouseButton::Right) => {
                            // Same bounds gate as the left-click arm
                            // above: only a right-click inside this
                            // knob queues its clear in Learn mode.
                            let bounds = cx.bounds();
                            let mouse = cx.mouse();
                            if mouse.cursor_x < bounds.x
                                || mouse.cursor_x > bounds.x + bounds.w
                                || mouse.cursor_y < bounds.y
                                || mouse.cursor_y > bounds.y + bounds.h
                            {
                                return;
                            }
                            // MIDI Learn clear, mirroring parameter_knob:
                            // consume without restoring in Learn mode.
                            if let Some(learn) = &learn {
                                if learn.request_clear(param_ptr) {
                                    meta.consume();
                                    return;
                                }
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
                                    // Consume ParamKnob's default reset and end the active drag.
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
        // The readout lives in a fixed-height box instead of directly in
        // the cell: a Memo-bound label can measure as zero-height in
        // flow layout, which let the next knob row start too high and
        // overlap the readout. The box reserves the row's height no
        // matter what the label measures. Double-clicking the readout
        // swaps the label for the value-entry textbox (the textbox's
        // own submit/cancel callbacks apply or discard the typed
        // value); the box listener below catches
        // double-clicks on the box padding, and the label's own
        // handler catches double-clicks on the text itself.
        let editing_for_box = editing.clone();
        let learn_mode_for_box = learn_mode.clone();
        let mut value_box = VStack::new(cx, |cx| {
            let editing_binding = editing.clone();
            Binding::new(cx, editing_binding, move |cx| {
                if editing.get() {
                    // The submit path applies the typed value directly
                    // from the textbox's own submit callback (view phase,
                    // like `ParamSlider`'s `on_submit` emitting its event):
                    // one bracketed host gesture, one undo entry.
                    // Unparseable text changes nothing and writes no
                    // undo entry. Cancel (Escape / focus loss) just
                    // closes the entry.
                    let editing_close = editing.clone();
                    let editing_cancel = editing.clone();
                    Textbox::new(cx, display_value)
                        .class("knob-value-entry")
                        .on_submit(move |cx, text: String, success: bool| {
                            if success {
                                if let Some(normalized) = parse_knob_entry(param_base, &text) {
                                    // Emit the gesture targeted at the
                                    // root: closing the entry below
                                    // rebuilds the Binding and destroys
                                    // this textbox before queued events
                                    // dispatch, and events targeted at
                                    // a removed entity never propagate
                                    // up to ParamModel. Root-targeted
                                    // events survive the rebuild (the
                                    // ParamModel lives on the root).
                                    cx.with_current(Entity::root(), |cx| {
                                        param_base.begin_set_parameter(cx);
                                        param_base.set_normalized_value(cx, normalized);
                                        param_base.end_set_parameter(cx);
                                    });
                                }
                            }
                            editing_close.set(false);
                        })
                        .on_cancel(move |_cx| {
                            editing_cancel.set(false);
                        })
                        .on_build(|cx| {
                            cx.emit(TextEvent::StartEdit);
                            cx.emit(TextEvent::SelectAll);
                        })
                        .height(Stretch(1.0))
                        .width(Stretch(1.0));
                } else {
                    // The listener below is invoked for every event in
                    // the tree, so it must only act on double-clicks
                    // actually targeted at this label (`meta.target ==
                    // cx.current()`): an unguarded listener would open
                    // this cell's entry on any double-click anywhere
                    // and, by consuming the event, would swallow the
                    // knob's own double-click reset.
                    let editing_open = editing.clone();
                    let learn_mode_signal = learn_mode.clone();
                    let mut readout_label = Label::new(cx, readout_text)
                        .class("knob-value")
                        .width(Stretch(1.0));
                    let label_entity = readout_label.entity();
                    readout_label.context().with_current(label_entity, |cx| {
                        cx.add_listener(
                            move |_: &mut Label, cx: &mut EventContext, event: &mut Event| {
                                event.map(|window, meta| {
                                    if matches!(
                                        window,
                                        WindowEvent::MouseDoubleClick(MouseButton::Left)
                                    ) && meta.target == cx.current()
                                    {
                                        // Learn mode is read at click time:
                                        // the label is built once, but Learn
                                        // can toggle later.
                                        let learn_on = learn_mode_signal
                                            .as_ref()
                                            .map_or(false, |mode| mode.get());
                                        if !learn_on {
                                            editing_open.set(true);
                                            meta.consume();
                                        }
                                    }
                                });
                            },
                        );
                    });
                }
            });
        })
        .class("knob-value-box")
        .width(Stretch(1.0))
        .height(Pixels(14.0));
        let value_box_entity = value_box.entity();
        value_box.context().with_current(value_box_entity, |cx| {
            let editing_open = editing_for_box;
            let learn_mode_signal = learn_mode_for_box;
            cx.add_listener(
                move |_: &mut VStack, cx: &mut EventContext, event: &mut Event| {
                    event.map(|window, meta| {
                        // Target-checked like the label listener: this
                        // box listener only covers double-clicks on the
                        // box padding (target == the box itself), never
                        // clicks on the knob or other cells.
                        if matches!(window, WindowEvent::MouseDoubleClick(MouseButton::Left))
                            && meta.target == cx.current()
                        {
                            let learn_on =
                                learn_mode_signal.as_ref().map_or(false, |mode| mode.get());
                            if !learn_on {
                                editing_open.set(true);
                                meta.consume();
                            }
                        }
                    });
                },
            );
        });
    })
    .class("knob-cell")
    .toggle_class("knob-learn-armed", armed)
    .width(Pixels(cell_width))
    .tooltip(move |cx| {
        Tooltip::new(cx, |cx| {
            // Fixed width so long descriptions wrap: Vizia's tooltip placement
            // measures the box before unwrapped text lays out, and an
            // unconstrained label overflows the window edge on right-side cells.
            Label::new(cx, tooltip).width(Pixels(240.0));
        })
    });
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
/// `learn`, when present, carries the `LearnArm` and the bound parameter's
/// `ParamPtr`: in Learn mode a left-click on the trigger arms the parameter
/// (the popup does not open) and a right-click queues a binding clear,
/// following the `parameter_knob` pattern. Selectors with no bindable
/// parameter (e.g. the guitar chord picker, which edits plain signals)
/// pass `None`.
pub fn discrete_selector<F>(
    cx: &mut Context,
    label: &'static str,
    selected: Signal<i32>,
    options: Vec<(i32, String)>,
    label_width: f32,
    on_select: F,
    learn: Option<(LearnArm, ParamPtr)>,
) where
    F: Fn(&mut EventContext, i32) + Clone + Send + Sync + 'static,
{
    let trigger_options = options.clone();
    let trigger_learn = learn.clone();
    HStack::new(cx, move |cx| {
        Label::new(cx, label).width(Pixels(label_width));
        // Keyboard navigation (same helper as the preset/language
        // dropdowns): the nav closures below read clones made here,
        // before the popup closure moves `options`/`on_select`.
        let nav_options = options.clone();
        let nav_selected = selected;
        let nav_on_select = on_select.clone();
        let dropdown_open = Signal::new(false);
        let dropdown_highlighted: Signal<Option<usize>> = Signal::new(None);
        let selector_dropdown = Dropdown::new(
            cx,
            move |cx| {
                let display_options = trigger_options.clone();
                let mut trigger = Button::new(cx, move |cx| {
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
                // MIDI Learn arming on the trigger: a listener runs
                // before the Button's own press handling and a consumed
                // event skips it, so arming never opens the popup.
                if let Some((learn, ptr)) = trigger_learn.clone() {
                    let entity = trigger.entity();
                    trigger.context().with_current(entity, |cx| {
                        cx.add_listener(
                            move |_: &mut Button, cx: &mut EventContext, event: &mut Event| {
                                // Bounds-gated like the knob/slider arm
                                // listeners: listeners see every event
                                // in the tree, so an unguarded arm()
                                // would arm this selector for a click
                                // anywhere in the window.
                                let bounds = cx.bounds();
                                let mouse = cx.mouse();
                                let inside = mouse.cursor_x >= bounds.x
                                    && mouse.cursor_x <= bounds.x + bounds.w
                                    && mouse.cursor_y >= bounds.y
                                    && mouse.cursor_y <= bounds.y + bounds.h;
                                event.map(|window, meta| match window {
                                    WindowEvent::MouseDown(MouseButton::Left) => {
                                        if inside && learn.arm(ptr) {
                                            meta.consume();
                                        }
                                    }
                                    WindowEvent::MouseDown(MouseButton::Right) => {
                                        if inside && learn.request_clear(ptr) {
                                            meta.consume();
                                        }
                                    }
                                    _ => {}
                                });
                            },
                        );
                    });
                }
            },
            {
                let highlighted = dropdown_highlighted;
                move |cx| {
                    for (index, (value, name)) in options.clone().into_iter().enumerate() {
                        let on_select = on_select.clone();
                        Button::new(cx, move |cx| {
                            Label::new(cx, name.clone())
                                .alignment(Alignment::Left)
                                .width(Stretch(1.0))
                                .text_overflow(TextOverflow::Ellipsis)
                        })
                        .toggle_class(
                            "preset-item-highlight",
                            highlighted.map(move |current| *current == Some(index)),
                        )
                        .on_hover(move |_| highlighted.set(Some(index)))
                        .on_press(move |cx| {
                            on_select(cx, value);
                            cx.emit(PopupEvent::Close);
                        })
                        .width(Stretch(1.0))
                        .max_width(Pixels(320.0));
                    }
                }
            },
        )
        .width(Stretch(1.0));
        attach_dropdown_keyboard_nav(
            selector_dropdown,
            dropdown_open,
            dropdown_highlighted,
            {
                let nav_options = nav_options.clone();
                move || {
                    if nav_options.is_empty() {
                        None
                    } else {
                        nav_options
                            .iter()
                            .position(|(option, _)| *option == nav_selected.get())
                            .or(Some(0))
                    }
                }
            },
            {
                let nav_options = nav_options.clone();
                move || nav_options.len()
            },
            {
                let nav_options = nav_options.clone();
                let nav_on_select = nav_on_select.clone();
                move |cx: &mut EventContext, index: usize| {
                    if let Some((value, _)) = nav_options.get(index) {
                        nav_on_select(cx, *value);
                    }
                }
            },
        );
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
        .map(|preset| (preset.id.clone(), preset_display_name(preset, lang)))
        .collect()
}

/// Resolves a factory preset through gettext while leaving user-authored names unchanged.
pub fn preset_display_name(preset: &Preset, lang: Language) -> String {
    if preset.is_factory {
        factory_preset_name(&preset.id, lang)
            .unwrap_or(&preset.name)
            .to_owned()
    } else {
        preset.name.clone()
    }
}

fn factory_preset_name(id: &str, lang: Language) -> Option<&'static str> {
    Some(match id {
        "steinway_concert_d" => translate(
            lang,
            "preset.name.steinway_concert_d",
            "Steinway D Concert Grand",
        ),
        "bright_pop_grand" => translate(
            lang,
            "preset.name.bright_pop_grand",
            "Bright Pop & Jazz Grand",
        ),
        "warm_intimate_chamber" => translate(
            lang,
            "preset.name.warm_intimate_chamber",
            "Warm Intimate Chamber",
        ),
        "vintage_upright" => translate(
            lang,
            "preset.name.vintage_upright",
            "Vintage Honky-Tonk Upright",
        ),
        "cinematic_dream" => translate(
            lang,
            "preset.name.cinematic_dream",
            "Cinematic Ambient Dream",
        ),
        "classical_pure_solo" => translate(
            lang,
            "preset.name.classical_pure_solo",
            "Classical Pure Solo",
        ),
        "strat_clean_chime" => {
            translate(lang, "preset.name.strat_clean_chime", "Strat Clean Chime")
        }
        "dreadnought_acoustic_fingerstyle" => translate(
            lang,
            "preset.name.dreadnought_acoustic_fingerstyle",
            "Martin D-28 Fingerstyle",
        ),
        "blues_overdrive_crunch" => translate(
            lang,
            "preset.name.blues_overdrive_crunch",
            "Texas Blues Breakup",
        ),
        "warm_jazz_archtop" => {
            translate(lang, "preset.name.warm_jazz_archtop", "Les Paul Warm Jazz")
        }
        "heavy_palm_mute_metal" => translate(
            lang,
            "preset.name.heavy_palm_mute_metal",
            "Heavy Metal Chug",
        ),
        "ambient_dreamy_pluck" => translate(
            lang,
            "preset.name.ambient_dreamy_pluck",
            "Dreadnought Strummer",
        ),
        "bass_finger_punch" => translate(lang, "preset.name.bass_finger_punch", "Finger Punch"),
        "bass_pick_attack" => translate(lang, "preset.name.bass_pick_attack", "Picked Attack"),
        "bass_slap_five" => translate(lang, "preset.name.bass_slap_five", "Five String Slap"),
        "bass_upright_body" => translate(lang, "preset.name.bass_upright_body", "Upright Body"),
        "drum_studio_kit" => translate(lang, "preset.name.drum_studio_kit", "Studio Kit"),
        "drum_dry_tight" => translate(lang, "preset.name.drum_dry_tight", "Dry Tight"),
        "drum_big_room" => translate(lang, "preset.name.drum_big_room", "Big Room"),
        "drum_low_punch" => translate(lang, "preset.name.drum_low_punch", "Low Punch"),
        _ => return None,
    })
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
    use super::{ParamWidgetBase, parse_knob_entry};
    use nice_plug::prelude::*;
    use vizia_plug::vizia::prelude::*;

    #[derive(Params)]
    struct EntryTestParams {
        #[id = "gain"]
        pub gain: FloatParam,
    }

    impl Default for EntryTestParams {
        fn default() -> Self {
            Self {
                gain: FloatParam::new(
                    "Gain",
                    0.0,
                    FloatRange::Linear {
                        min: -12.0,
                        max: 12.0,
                    },
                )
                .with_unit(" dB"),
            }
        }
    }

    #[test]
    fn knob_entry_accepts_formatted_units_and_bare_numbers() {
        let params = EntryTestParams::default();
        let cx = Context::default();
        let base = ParamWidgetBase::new(&cx, &params.gain);
        // Formatted display value, with the unit the readout shows.
        assert!((parse_knob_entry(base, "-3 dB").unwrap() - 0.375).abs() < 1e-6);
        // Bare number in plain units.
        assert!((parse_knob_entry(base, "0.5").unwrap() - 0.520_833_3).abs() < 1e-6);
        // Out-of-range input clamps to the ends instead of rejecting.
        assert_eq!(parse_knob_entry(base, "999 dB"), Some(1.0));
        assert_eq!(parse_knob_entry(base, "-999"), Some(0.0));
        // Garbage changes nothing.
        assert_eq!(parse_knob_entry(base, "abc"), None);
        assert_eq!(parse_knob_entry(base, ""), None);
        assert_eq!(parse_knob_entry(base, "dB"), None);
    }

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
