use nice_plug::prelude::Param;
use physics_presets::PresetManager;
use std::sync::Arc;
use vizia_plug::vizia::prelude::*;
use vizia_plug::widgets::*;

use crate::Language;

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
            .width(Stretch(1.0))
            .height(Pixels(20.0));
        let drag_start = Arc::new(parking_lot::Mutex::new(None::<f32>));
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
                                *drag_start.lock() =
                                    Some(unsafe { param_ptr.unmodulated_normalized_value() });
                            }
                        }
                        WindowEvent::MouseUp(MouseButton::Left) => {
                            *drag_start.lock() = None;
                        }
                        WindowEvent::MouseDown(MouseButton::Right) => {
                            if let Some(value) = *drag_start.lock() {
                                cx.emit(RawParamEvent::SetParameterNormalized(param_ptr, value));
                                *drag_start.lock() = None;
                                // Consume ParamSlider's default reset and end the active drag.
                                cx.emit_custom(
                                    Event::new(WindowEvent::MouseUp(MouseButton::Left))
                                        .target(cx.current())
                                        .propagate(Propagation::Direct),
                                );
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
