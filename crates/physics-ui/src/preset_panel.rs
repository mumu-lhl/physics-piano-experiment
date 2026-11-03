use vizia_plug::vizia::context::TreeProps;
use vizia_plug::vizia::prelude::*;

use crate::Language;
use crate::translate;

/// Keyboard navigation for a `Dropdown` popup.
///
/// Shared by this panel's preset/language dropdowns and by the
/// same-shape siblings in `widgets.rs` (`discrete_selector`) and
/// `device_row.rs` (`device_dropdown`): callers supply only the
/// initial highlight, the row count, and the select action, and
/// this helper owns the open/highlight signals' listener, so the
/// navigation behavior stays identical across all dropdowns.
///
/// Vizia's `Dropdown` has no keyboard handling of its own: opening the popup
/// leaves focus on the trigger button, the list items never receive focus,
/// and no view handles `ArrowDown`/`ArrowUp`/`Enter`/`Escape` for the list.
/// Without this, arrow keys do nothing visible and `Enter` just re-toggles
/// the trigger (the `Press` synthesized for the focused button).
///
/// The fix keeps two small signals per dropdown: `open`, a mirror of the
/// `Dropdown`'s internal open state kept in sync by observing the
/// `PopupEvent`s the trigger, the items, and the `Popover` blur handling
/// emit, and `highlighted`, the index of the visibly highlighted row. A
/// listener on the `Dropdown` entity sees every window event before its
/// target (vizia runs local listeners first); it only acts on
/// arrow/enter/escape keys while its own popup is open and keyboard focus
/// is inside its subtree, so other views (knobs, the piano keyboard
/// handler) keep their keys. The initial highlight is the currently
/// selected row, falling back to the first row — the same choice vizia's
/// own `ComboBox` makes.
pub(crate) fn attach_dropdown_keyboard_nav(
    mut dropdown: Handle<'_, Dropdown>,
    open: Signal<bool>,
    highlighted: Signal<Option<usize>>,
    initial: impl Fn() -> Option<usize> + 'static,
    count: impl Fn() -> usize + 'static,
    on_select: impl Fn(&mut EventContext, usize) + 'static,
) {
    let entity = dropdown.entity();
    dropdown.context().with_current(entity, |cx| {
        cx.add_listener(
            move |dropdown: &mut Dropdown, cx: &mut EventContext, event: &mut Event| {
                event.map(|popup_event: &PopupEvent, meta| {
                    if !entity_is_within(cx, meta.target, entity) {
                        return;
                    }
                    match popup_event {
                        PopupEvent::Open => open.set(true),
                        PopupEvent::Close => {
                            open.set(false);
                            highlighted.set(None);
                        }
                        PopupEvent::Switch => {
                            // Read the Dropdown's own state, not the mirror:
                            // listeners run before the Dropdown handles
                            // the event, so its pre-event value tells us
                            // what the switch is about to do. Toggling the
                            // mirror instead drifts when an Open emitted
                            // by the closed-trigger Down path is still
                            // queued ahead of the Switch.
                            if dropdown.is_open.get() {
                                open.set(false);
                                highlighted.set(None);
                            } else {
                                open.set(true);
                                highlighted.set(initial());
                            }
                        }
                    }
                });
                event.map(|window_event: &WindowEvent, meta| {
                    let code = match window_event {
                        WindowEvent::KeyDown(code, _) => *code,
                        _ => return,
                    };
                    match code {
                        Code::ArrowDown
                        | Code::ArrowUp
                        | Code::Home
                        | Code::End
                        | Code::Enter
                        | Code::NumpadEnter
                        | Code::Escape => {}
                        _ => return,
                    }
                    let len = count();
                    if !open.get() {
                        // Closed trigger: Down/Up opens the list at the
                        // current selection (ComboBox parity). Enter/Space
                        // keep their native trigger behavior.
                        if matches!(code, Code::ArrowDown | Code::ArrowUp)
                            && entity_is_within(cx, cx.focused(), entity)
                        {
                            highlighted.set(initial());
                            // Optimistic: a second arrow key queued behind
                            // the Open event must already see the popup as
                            // open, or it re-initializes the highlight
                            // instead of moving it.
                            open.set(true);
                            cx.emit(PopupEvent::Open);
                            meta.consume();
                        }
                        return;
                    }
                    if !entity_is_within(cx, cx.focused(), entity) {
                        return;
                    }
                    match code {
                        Code::ArrowDown => {
                            highlighted.set(highlight_next(highlighted.get(), len));
                            meta.consume();
                        }
                        Code::ArrowUp => {
                            highlighted.set(highlight_prev(highlighted.get(), len));
                            meta.consume();
                        }
                        Code::Home => {
                            highlighted.set(if len == 0 { None } else { Some(0) });
                            meta.consume();
                        }
                        Code::End => {
                            highlighted.set(if len == 0 { None } else { Some(len - 1) });
                            meta.consume();
                        }
                        Code::Escape => {
                            highlighted.set(None);
                            open.set(false);
                            cx.emit(PopupEvent::Close);
                            meta.consume();
                        }
                        Code::Enter | Code::NumpadEnter => {
                            if let Some(index) = highlighted.get() {
                                if index < len {
                                    on_select(cx, index);
                                    highlighted.set(None);
                                    open.set(false);
                                    cx.emit(PopupEvent::Close);
                                    meta.consume();
                                }
                            }
                        }
                        _ => {}
                    }
                });
            },
        );
    });
}

/// Whether `entity` is `ancestor` or descends from it, walking layout
/// parents. The event context exposes no tree handle, so the walk goes
/// through `with_current`/`parent`; it is bounded and stops at the root.
fn entity_is_within(cx: &mut EventContext, mut entity: Entity, ancestor: Entity) -> bool {
    for _ in 0..256 {
        if entity == ancestor {
            return true;
        }
        if entity == Entity::root() || entity == Entity::null() {
            return false;
        }
        entity = cx.with_current(entity, |cx| cx.parent());
    }
    false
}

/// Initial highlight: the row matching the trigger label, or the first row
/// when the selection is not in the list. `None` only for an empty list.
fn initial_highlight(choices: &[(String, String)], selected_name: &str) -> Option<usize> {
    if choices.is_empty() {
        return None;
    }
    choices
        .iter()
        .position(|(_, name)| name == selected_name)
        .or(Some(0))
}

/// Next highlight, wrapping past the last row (vizia `ComboBox` parity). A
/// stale or missing highlight restarts at the first row.
fn highlight_next(current: Option<usize>, len: usize) -> Option<usize> {
    if len == 0 {
        return None;
    }
    Some(match current.filter(|index| *index < len) {
        Some(index) => (index + 1) % len,
        None => 0,
    })
}

/// Previous highlight, wrapping past the first row. A stale or missing
/// highlight starts at the last row.
fn highlight_prev(current: Option<usize>, len: usize) -> Option<usize> {
    if len == 0 {
        return None;
    }
    Some(match current.filter(|index| *index < len) {
        Some(0) | None => len - 1,
        Some(index) => index - 1,
    })
}

#[derive(Debug, Clone)]
pub enum PresetPanelAction {
    Previous,
    Next,
    SelectLanguage(Language),
    Select(String),
    SetName(String),
    SaveAs,
    Rename,
    Overwrite,
    Delete,
    Undo,
    Redo,
}

#[derive(Clone)]
pub struct PresetPanelSignals {
    pub selected_name: Signal<String>,
    pub preset_choices: Signal<Vec<(String, String)>>,
    pub language: Signal<Language>,
    pub name_input: Signal<String>,
    pub is_user_preset: Signal<bool>,
    pub can_undo: Signal<bool>,
    pub can_redo: Signal<bool>,
}

#[derive(Debug, Clone, Copy)]
pub struct PresetPanelLayout {
    pub preset_label: Option<&'static str>,
    pub preset_width: f32,
    pub name_width: f32,
    pub save_as_width: f32,
    pub rename_width: f32,
    pub overwrite_width: f32,
    pub delete_width: f32,
}

/// Builds the shared preset navigation, language, save, and history controls.
/// Instrument-specific state handling remains with the caller via emitted actions.
pub fn preset_panel(
    cx: &mut Context,
    lang: Language,
    signals: PresetPanelSignals,
    layout: PresetPanelLayout,
    right_accessory: impl FnOnce(&mut Context),
) {
    let selected_name = signals.selected_name;
    let preset_choices = signals.preset_choices;
    let name_input = signals.name_input;
    let is_user_preset = signals.is_user_preset;
    let can_undo = signals.can_undo;
    let can_redo = signals.can_redo;
    let language = signals.language;

    HStack::new(cx, move |cx| {
        Element::new(cx)
            .width(Stretch(1.0))
            .pointer_events(PointerEvents::None);
        HStack::new(cx, move |cx| {
            if let Some(label) = layout.preset_label {
                Label::new(cx, label).class("param-label");
            }
            Button::new(cx, |cx| Label::new(cx, "<"))
                .class("btn-cycle")
                .on_press(|cx| cx.emit(PresetPanelAction::Previous))
                .width(Pixels(28.0));
            let preset_open = Signal::new(false);
            let preset_highlighted: Signal<Option<usize>> = Signal::new(None);
            let preset_dropdown = Dropdown::new(
                cx,
                {
                    let selected_name = selected_name.clone();
                    move |cx| {
                        Button::new(cx, move |cx| {
                            HStack::new(cx, |cx| {
                                Label::new(cx, selected_name.clone())
                                    .class("preset-dropdown-label")
                                    .width(Stretch(1.0))
                                    .text_overflow(TextOverflow::Ellipsis);
                                Label::new(cx, "▾").hoverable(false);
                            })
                            .width(Stretch(1.0))
                        })
                        .class("preset-dropdown")
                        .on_press(|cx| cx.emit(PopupEvent::Switch))
                        .width(Stretch(1.0));
                    }
                },
                {
                    let preset_choices = preset_choices.clone();
                    let highlighted = preset_highlighted;
                    move |cx| {
                        Binding::new(cx, preset_choices, move |cx| {
                            for (index, (id, name)) in preset_choices.get().into_iter().enumerate()
                            {
                                let event_id = id.clone();
                                Button::new(cx, move |cx| {
                                    Label::new(cx, name.clone())
                                        .alignment(Alignment::Left)
                                        .width(Stretch(1.0))
                                        .text_overflow(TextOverflow::Ellipsis)
                                })
                                .class("preset-item")
                                .toggle_class(
                                    "preset-item-highlight",
                                    highlighted.map(move |current| *current == Some(index)),
                                )
                                .on_hover(move |_| highlighted.set(Some(index)))
                                .on_press(move |cx| {
                                    cx.emit(PresetPanelAction::Select(event_id.clone()));
                                    cx.emit(PopupEvent::Close);
                                })
                                .width(Stretch(1.0))
                                .max_width(Pixels(layout.preset_width));
                            }
                        });
                    }
                },
            )
            .width(Pixels(layout.preset_width));
            attach_dropdown_keyboard_nav(
                preset_dropdown,
                preset_open,
                preset_highlighted,
                {
                    let preset_choices = preset_choices.clone();
                    let selected_name = selected_name.clone();
                    move || {
                        let choices = preset_choices.get();
                        let selected = selected_name.get();
                        initial_highlight(&choices, &selected)
                    }
                },
                {
                    let preset_choices = preset_choices.clone();
                    move || preset_choices.get().len()
                },
                {
                    let preset_choices = preset_choices.clone();
                    move |cx: &mut EventContext, index: usize| {
                        let choices = preset_choices.get();
                        if let Some((id, _)) = choices.get(index) {
                            cx.emit(PresetPanelAction::Select(id.clone()));
                        }
                    }
                },
            );
            Button::new(cx, |cx| Label::new(cx, ">"))
                .class("btn-cycle")
                .on_press(|cx| cx.emit(PresetPanelAction::Next))
                .width(Pixels(28.0));
            Button::new(cx, |cx| {
                Label::new(cx, translate(lang, "gui.browse", "Browse"))
            })
            .class("btn-action")
            .on_press(|cx| cx.emit(crate::PresetBrowserAction::Open))
            .width(Pixels(64.0));
            let available_languages: Vec<_> = Language::available().collect();
            let language_open = Signal::new(false);
            let language_highlighted: Signal<Option<usize>> = Signal::new(None);
            let content_languages = available_languages.clone();
            let language_dropdown = Dropdown::new(
                cx,
                move |cx| {
                    Button::new(cx, move |cx| {
                        Label::new(cx, language.map(|locale| locale.native_name().to_owned()))
                    })
                    .class("btn-action")
                    .on_press(|cx| cx.emit(PopupEvent::Switch))
                    .width(Pixels(90.0));
                },
                {
                    let highlighted = language_highlighted;
                    move |cx| {
                        for (index, locale) in content_languages.iter().copied().enumerate() {
                            let label = locale.native_name().to_owned();
                            Button::new(cx, move |cx| {
                                Label::new(cx, label.clone())
                                    .alignment(Alignment::Left)
                                    .width(Stretch(1.0))
                                    .text_overflow(TextOverflow::Ellipsis)
                            })
                            .class("preset-item")
                            .toggle_class(
                                "preset-item-highlight",
                                highlighted.map(move |current| *current == Some(index)),
                            )
                            .on_hover(move |_| highlighted.set(Some(index)))
                            .on_press(move |cx| {
                                cx.emit(PresetPanelAction::SelectLanguage(locale));
                                cx.emit(PopupEvent::Close);
                            })
                            .width(Pixels(90.0));
                        }
                    }
                },
            )
            .width(Pixels(90.0));
            attach_dropdown_keyboard_nav(
                language_dropdown,
                language_open,
                language_highlighted,
                {
                    let languages = available_languages.clone();
                    let language = language.clone();
                    move || {
                        if languages.is_empty() {
                            None
                        } else {
                            languages
                                .iter()
                                .position(|locale| *locale == language.get())
                                .or(Some(0))
                        }
                    }
                },
                {
                    let languages = available_languages.clone();
                    move || languages.len()
                },
                move |cx: &mut EventContext, index: usize| {
                    if let Some(locale) = available_languages.get(index) {
                        cx.emit(PresetPanelAction::SelectLanguage(*locale));
                    }
                },
            );

            right_accessory(cx);
        })
        .height(Pixels(38.0))
        .horizontal_gap(Pixels(8.0));
        Element::new(cx)
            .width(Stretch(1.0))
            .pointer_events(PointerEvents::None);
    })
    .height(Pixels(38.0))
    .width(Stretch(1.0));

    HStack::new(cx, move |cx| {
        let mut name_textbox = Textbox::new(cx, name_input.clone())
            .name(translate(lang, "shared.preset-name", "Preset name"))
            .placeholder(translate(lang, "shared.preset-name", "Preset name"))
            .on_edit(|cx, text| cx.emit(PresetPanelAction::SetName(text)));
        let name_textbox_entity = name_textbox.entity();
        name_textbox
            .context()
            .with_current(name_textbox_entity, |cx| {
                cx.add_listener(
                    move |_: &mut Textbox<Signal<String>, String>,
                          cx: &mut EventContext,
                          event: &mut Event| {
                        event.map(|window: &WindowEvent, _| {
                            if matches!(window, WindowEvent::MouseDown(MouseButton::Left))
                                && cx.hovered() != name_textbox_entity
                            {
                                cx.with_current(Entity::root(), |cx| {
                                    cx.focus_with_visibility(false)
                                });
                            }
                        });
                    },
                );
            });
        name_textbox
            .width(Pixels(layout.name_width))
            .height(Pixels(26.0));
        Button::new(cx, |cx| {
            Label::new(cx, translate(lang, "shared.save-as", "Save As"))
        })
        .class("btn-action")
        .on_press(|cx| cx.emit(PresetPanelAction::SaveAs))
        .width(Pixels(layout.save_as_width));
        Button::new(cx, |cx| {
            Label::new(cx, translate(lang, "shared.rename", "Rename"))
        })
        .class("btn-action")
        .on_press(|cx| cx.emit(PresetPanelAction::Rename))
        .disabled(is_user_preset.clone().map(|is_user| !is_user))
        .toggle_class(
            "btn-disabled",
            is_user_preset.clone().map(|is_user| !is_user),
        )
        .width(Pixels(layout.rename_width));
        Button::new(cx, |cx| {
            Label::new(cx, translate(lang, "shared.overwrite", "Overwrite"))
        })
        .class("btn-action")
        .on_press(|cx| cx.emit(PresetPanelAction::Overwrite))
        .disabled(is_user_preset.clone().map(|is_user| !is_user))
        .toggle_class(
            "btn-disabled",
            is_user_preset.clone().map(|is_user| !is_user),
        )
        .width(Pixels(layout.overwrite_width));
        Button::new(cx, |cx| {
            Label::new(cx, translate(lang, "shared.delete", "Delete"))
        })
        .class("btn-action")
        .on_press(|cx| cx.emit(PresetPanelAction::Delete))
        .disabled(is_user_preset.clone().map(|is_user| !is_user))
        .toggle_class(
            "btn-disabled",
            is_user_preset.clone().map(|is_user| !is_user),
        )
        .width(Pixels(layout.delete_width));
        Button::new(cx, |cx| {
            Label::new(cx, translate(lang, "shared.undo", "Undo"))
        })
        .class("btn-action")
        .on_press(|cx| cx.emit(PresetPanelAction::Undo))
        .disabled(can_undo.clone().map(|enabled| !enabled))
        .toggle_class("btn-disabled", can_undo.clone().map(|enabled| !enabled))
        .width(Pixels(48.0));
        Button::new(cx, |cx| {
            Label::new(cx, translate(lang, "shared.redo", "Redo"))
        })
        .class("btn-action")
        .on_press(|cx| cx.emit(PresetPanelAction::Redo))
        .disabled(can_redo.clone().map(|enabled| !enabled))
        .toggle_class("btn-disabled", can_redo.clone().map(|enabled| !enabled))
        .width(Pixels(48.0));
    })
    .height(Pixels(30.0))
    .horizontal_gap(Pixels(8.0));
}

#[cfg(test)]
mod tests {
    use super::{highlight_next, highlight_prev, initial_highlight};

    fn choices() -> Vec<(String, String)> {
        vec![
            ("a".to_owned(), "Alpha".to_owned()),
            ("b".to_owned(), "Beta".to_owned()),
            ("c".to_owned(), "Gamma".to_owned()),
        ]
    }

    #[test]
    fn initial_highlight_matches_the_current_selection() {
        assert_eq!(initial_highlight(&choices(), "Beta"), Some(1));
    }

    #[test]
    fn initial_highlight_falls_back_to_the_first_row() {
        assert_eq!(initial_highlight(&choices(), "Missing"), Some(0));
        assert_eq!(initial_highlight(&[], "Alpha"), None);
    }

    #[test]
    fn highlight_next_moves_and_wraps() {
        assert_eq!(highlight_next(Some(0), 3), Some(1));
        assert_eq!(highlight_next(Some(2), 3), Some(0));
        assert_eq!(highlight_next(None, 3), Some(0));
        assert_eq!(highlight_next(Some(9), 3), Some(0));
        assert_eq!(highlight_next(Some(0), 0), None);
    }

    #[test]
    fn highlight_prev_moves_and_wraps() {
        assert_eq!(highlight_prev(Some(1), 3), Some(0));
        assert_eq!(highlight_prev(Some(0), 3), Some(2));
        assert_eq!(highlight_prev(None, 3), Some(2));
        assert_eq!(highlight_prev(Some(9), 3), Some(2));
        assert_eq!(highlight_prev(Some(0), 0), None);
    }
}
