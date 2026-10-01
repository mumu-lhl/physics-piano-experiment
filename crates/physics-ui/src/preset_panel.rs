use vizia_plug::vizia::prelude::*;

use crate::Language;
use crate::ui_text;

#[derive(Debug, Clone)]
pub enum PresetPanelAction {
    Previous,
    Next,
    ToggleLanguage,
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
            Dropdown::new(
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
                    move |cx| {
                        Binding::new(cx, preset_choices, move |cx| {
                            for (id, name) in preset_choices.get() {
                                let event_id = id.clone();
                                Button::new(cx, move |cx| {
                                    Label::new(cx, name.clone())
                                        .alignment(Alignment::Left)
                                        .width(Stretch(1.0))
                                })
                                .class("preset-item")
                                .on_press(move |cx| {
                                    cx.emit(PresetPanelAction::Select(event_id.clone()));
                                    cx.emit(PopupEvent::Close);
                                })
                                .width(Stretch(1.0));
                            }
                        });
                    }
                },
            )
            .width(Pixels(layout.preset_width));
            Button::new(cx, |cx| Label::new(cx, ">"))
                .class("btn-cycle")
                .on_press(|cx| cx.emit(PresetPanelAction::Next))
                .width(Pixels(28.0));
            Button::new(cx, move |cx| {
                Label::new(
                    cx,
                    language.map(|lang| match lang {
                        Language::English => "中文".to_string(),
                        Language::SimplifiedChinese => "English".to_string(),
                    }),
                )
            })
            .class("btn-action")
            .on_press(|cx| cx.emit(PresetPanelAction::ToggleLanguage))
            .width(Pixels(60.0));

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
        Textbox::new(cx, name_input.clone())
            .name(ui_text(lang, "Preset name", "预设名称"))
            .placeholder(ui_text(lang, "Preset name", "预设名称"))
            .on_edit(|cx, text| cx.emit(PresetPanelAction::SetName(text)))
            .width(Pixels(layout.name_width))
            .height(Pixels(26.0));
        Button::new(cx, |cx| Label::new(cx, ui_text(lang, "Save As", "另存为")))
            .class("btn-action")
            .on_press(|cx| cx.emit(PresetPanelAction::SaveAs))
            .width(Pixels(layout.save_as_width));
        Button::new(cx, |cx| Label::new(cx, ui_text(lang, "Rename", "重命名")))
            .class("btn-action")
            .on_press(|cx| cx.emit(PresetPanelAction::Rename))
            .disabled(is_user_preset.clone().map(|is_user| !is_user))
            .width(Pixels(layout.rename_width));
        Button::new(cx, |cx| {
            Label::new(cx, ui_text(lang, "Overwrite", "覆盖保存"))
        })
        .class("btn-action")
        .on_press(|cx| cx.emit(PresetPanelAction::Overwrite))
        .disabled(is_user_preset.clone().map(|is_user| !is_user))
        .width(Pixels(layout.overwrite_width));
        Button::new(cx, |cx| Label::new(cx, ui_text(lang, "Delete", "删除")))
            .class("btn-action")
            .on_press(|cx| cx.emit(PresetPanelAction::Delete))
            .disabled(is_user_preset.clone().map(|is_user| !is_user))
            .width(Pixels(layout.delete_width));
        Button::new(cx, |cx| Label::new(cx, ui_text(lang, "Undo", "撤销")))
            .class("btn-action")
            .on_press(|cx| cx.emit(PresetPanelAction::Undo))
            .disabled(can_undo.clone().map(|enabled| !enabled))
            .width(Pixels(48.0));
        Button::new(cx, |cx| Label::new(cx, ui_text(lang, "Redo", "重做")))
            .class("btn-action")
            .on_press(|cx| cx.emit(PresetPanelAction::Redo))
            .disabled(can_redo.clone().map(|enabled| !enabled))
            .width(Pixels(48.0));
    })
    .height(Pixels(30.0))
    .horizontal_gap(Pixels(8.0));
}
