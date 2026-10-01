//! Vizia editor for the physical drum kit.
//!
//! This mirrors the piano/guitar editor structure: factory/user presets and
//! language controls are shared, parameter racks are grouped, and the playable
//! custom pad view is kept as a real instrument surface rather than a row of
//! generic buttons.

mod i18n;
mod pad_view;

use crate::nice_plugin::{GuiDrumEvent, PhysicsDrumParams};
use i18n::{I18n, Language};
use nice_plug::prelude::{Editor, Param};
use pad_view::DrumPadWidget;
use physics_presets::{ParamTransition, Preset, PresetManager, UndoManager};
use physics_ui::{
    CancelParamGestureEvent, PresetPanelAction, PresetPanelLayout, PresetPanelSignals,
    add_base_theme, map_param_history_event, parameter_slider, preset_choices, preset_panel,
    redraw_custom_view, set_param, setup_vizia_fonts,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU32, Ordering};
use vizia_plug::vizia::prelude::*;
use vizia_plug::widgets::RawParamEvent;
use vizia_plug::{ViziaTheming, create_vizia_editor};

pub const EDITOR_WIDTH: u32 = 900;
pub const EDITOR_HEIGHT: u32 = 640;

#[derive(Debug)]
enum DrumUiEvent {
    PreviousPreset,
    NextPreset,
    ToggleLanguage,
    SelectPreset(String),
    SetName(String),
    SaveAs,
    Rename,
    Overwrite,
    Delete,
    Undo,
    Redo,
    RefreshDisplay,
}

fn drum_value(params: &PhysicsDrumParams, id: &str) -> Option<f32> {
    Some(match id {
        "snare_tightness" => params.snare_tightness.value(),
        "snare_decay" => params.snare_decay.value(),
        "hihat_open" => params.hihat_open.value(),
        "cymbal_decay" => params.cymbal_decay.value(),
        "gain" => params.master_gain.value(),
        _ => return None,
    })
}

fn set_drum_value(cx: &mut EventContext, params: &PhysicsDrumParams, id: &str, value: f32) {
    match id {
        "snare_tightness" => set_param(cx, &params.snare_tightness, value),
        "snare_decay" => set_param(cx, &params.snare_decay, value),
        "hihat_open" => set_param(cx, &params.hihat_open, value),
        "cymbal_decay" => set_param(cx, &params.cymbal_decay, value),
        "gain" => set_param(cx, &params.master_gain, value),
        _ => {}
    }
}

fn drum_snapshot(params: &PhysicsDrumParams) -> HashMap<String, f32> {
    [
        "snare_tightness",
        "snare_decay",
        "hihat_open",
        "cymbal_decay",
        "gain",
    ]
    .into_iter()
    .filter_map(|id| drum_value(params, id).map(|value| (id.to_string(), value)))
    .collect()
}

struct DrumUiState {
    params: Arc<PhysicsDrumParams>,
    manager: Arc<parking_lot::RwLock<PresetManager>>,
    undo: Arc<parking_lot::RwLock<UndoManager>>,
    selected_name: Signal<String>,
    selected_id: Signal<String>,
    preset_choices: Signal<Vec<(String, String)>>,
    language: Signal<Language>,
    language_atom: Arc<AtomicU8>,
    name_input: Signal<String>,
    is_user_preset: Signal<bool>,
    can_undo: Signal<bool>,
    can_redo: Signal<bool>,
    suppress_undo: Arc<AtomicU32>,
}

impl DrumUiState {
    fn update_history_state(&self) {
        let undo = self.undo.read();
        self.can_undo.set(undo.can_undo());
        self.can_redo.set(undo.can_redo());
    }

    fn apply_history(&mut self, cx: &mut EventContext, redo: bool) {
        let changes = if redo {
            self.undo.write().redo()
        } else {
            self.undo.write().undo()
        };
        if let Some(changes) = changes {
            for (id, value) in changes {
                self.suppress_undo.fetch_add(1, Ordering::Relaxed);
                set_drum_value(cx, &self.params, &id, value);
            }
        }
        self.update_history_state();
    }
}

impl Model for DrumUiState {
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|action: &PresetPanelAction, _| match action {
            PresetPanelAction::Previous => cx.emit(DrumUiEvent::PreviousPreset),
            PresetPanelAction::Next => cx.emit(DrumUiEvent::NextPreset),
            PresetPanelAction::ToggleLanguage => cx.emit(DrumUiEvent::ToggleLanguage),
            PresetPanelAction::Select(id) => cx.emit(DrumUiEvent::SelectPreset(id.clone())),
            PresetPanelAction::SetName(name) => cx.emit(DrumUiEvent::SetName(name.clone())),
            PresetPanelAction::SaveAs => cx.emit(DrumUiEvent::SaveAs),
            PresetPanelAction::Rename => cx.emit(DrumUiEvent::Rename),
            PresetPanelAction::Overwrite => cx.emit(DrumUiEvent::Overwrite),
            PresetPanelAction::Delete => cx.emit(DrumUiEvent::Delete),
            PresetPanelAction::Undo => cx.emit(DrumUiEvent::Undo),
            PresetPanelAction::Redo => cx.emit(DrumUiEvent::Redo),
        });

        event.map(|action: &DrumUiEvent, _| match action {
            DrumUiEvent::RefreshDisplay => redraw_custom_view(cx, "drum-pad-widget"),
            DrumUiEvent::PreviousPreset | DrumUiEvent::NextPreset => {
                let next = matches!(action, DrumUiEvent::NextPreset);
                let id = {
                    let manager = self.manager.read();
                    let presets = manager.presets();
                    if presets.is_empty() {
                        return;
                    }
                    let index = presets
                        .iter()
                        .position(|preset| preset.id == self.selected_id.get())
                        .unwrap_or(0);
                    let index = if next {
                        (index + 1) % presets.len()
                    } else {
                        (index + presets.len() - 1) % presets.len()
                    };
                    presets[index].id.clone()
                };
                cx.emit(DrumUiEvent::SelectPreset(id));
            }
            DrumUiEvent::SelectPreset(id) => {
                let preset = self.manager.read().get_preset(id).cloned();
                if let Some(preset) = preset {
                    let mut changes = Vec::new();
                    for (param_id, value) in &preset.params {
                        if let Some(old_value) = drum_value(&self.params, param_id) {
                            if (old_value - value).abs() > 1e-5 {
                                changes.push(ParamTransition {
                                    param_id: param_id.clone(),
                                    old_value,
                                    new_value: *value,
                                });
                                self.suppress_undo.fetch_add(1, Ordering::Relaxed);
                                set_drum_value(cx, &self.params, param_id, *value);
                            }
                        }
                    }
                    if !changes.is_empty() {
                        self.undo.write().record_batch(&preset.name, changes);
                    }
                    self.selected_id.set(preset.id.clone());
                    let display_name = preset
                        .display_name(self.language.get() == Language::SimplifiedChinese)
                        .to_string();
                    self.selected_name.set(display_name.clone());
                    self.name_input.set(display_name);
                    self.is_user_preset
                        .set(self.manager.read().is_user_preset(id));
                    self.preset_choices
                        .set(preset_choices(&self.manager.read(), self.language.get()));
                    self.update_history_state();
                }
            }
            DrumUiEvent::ToggleLanguage => {
                let language = if self.language.get() == Language::English {
                    Language::SimplifiedChinese
                } else {
                    Language::English
                };
                self.language.set(language);
                self.language_atom.store(
                    u8::from(language == Language::SimplifiedChinese),
                    Ordering::Relaxed,
                );
                if let Some(preset) = self.manager.read().get_preset(&self.selected_id.get()) {
                    let display_name = preset
                        .display_name(language == Language::SimplifiedChinese)
                        .to_string();
                    self.selected_name.set(display_name.clone());
                    self.name_input.set(display_name);
                }
                self.preset_choices
                    .set(preset_choices(&self.manager.read(), language));
                redraw_custom_view(cx, "drum-pad-widget");
            }
            DrumUiEvent::SetName(name) => self.name_input.set(name.clone()),
            DrumUiEvent::SaveAs => {
                let name = self.name_input.get().trim().to_string();
                let name = if name.is_empty() {
                    "Custom Drum Kit".to_string()
                } else {
                    name
                };
                let id = format!(
                    "user_{}",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|duration| duration.as_nanos())
                        .unwrap_or_default()
                );
                let preset = Preset::new(
                    id.clone(),
                    name.clone(),
                    "drum",
                    drum_snapshot(&self.params),
                );
                if self.manager.write().save_user_preset(preset).is_ok() {
                    self.selected_id.set(id);
                    self.selected_name.set(name.clone());
                    self.name_input.set(name);
                    self.is_user_preset.set(true);
                    self.preset_choices
                        .set(preset_choices(&self.manager.read(), self.language.get()));
                }
            }
            DrumUiEvent::Rename => {
                if self.is_user_preset.get() {
                    let name = self.name_input.get().trim().to_string();
                    if !name.is_empty()
                        && self
                            .manager
                            .write()
                            .rename_user_preset(&self.selected_id.get(), &name)
                            .unwrap_or(false)
                    {
                        self.selected_name.set(name);
                        self.preset_choices
                            .set(preset_choices(&self.manager.read(), self.language.get()));
                    }
                }
            }
            DrumUiEvent::Overwrite => {
                if self.is_user_preset.get() {
                    let _ = self.manager.write().overwrite_user_preset(
                        &self.selected_id.get(),
                        drum_snapshot(&self.params),
                    );
                }
            }
            DrumUiEvent::Delete => {
                let id = self.selected_id.get();
                if self
                    .manager
                    .write()
                    .delete_user_preset(&id)
                    .unwrap_or(false)
                {
                    self.preset_choices
                        .set(preset_choices(&self.manager.read(), self.language.get()));
                    cx.emit(DrumUiEvent::SelectPreset("drum_studio_kit".to_string()));
                }
            }
            DrumUiEvent::Undo => self.apply_history(cx, false),
            DrumUiEvent::Redo => self.apply_history(cx, true),
        });

        event.map(|cancel: &CancelParamGestureEvent, _| {
            let ptr = cancel.param;
            let params = &self.params;
            let id = if ptr == params.snare_tightness.as_ptr() {
                "snare_tightness"
            } else if ptr == params.snare_decay.as_ptr() {
                "snare_decay"
            } else if ptr == params.hihat_open.as_ptr() {
                "hihat_open"
            } else if ptr == params.cymbal_decay.as_ptr() {
                "cymbal_decay"
            } else if ptr == params.master_gain.as_ptr() {
                "gain"
            } else {
                return;
            };
            self.suppress_undo.fetch_add(1, Ordering::Relaxed);
            let restored = unsafe { ptr.preview_plain(cancel.restore_normalized) };
            if self.undo.write().cancel_last_single_param(id, restored) {
                self.update_history_state();
            }
        });

        map_param_history_event(event, |raw, wheel| {
            let (ptr, begin) = match raw {
                RawParamEvent::BeginSetParameter(ptr) => (*ptr, true),
                RawParamEvent::EndSetParameter(ptr) => (*ptr, false),
                RawParamEvent::SetParameterNormalized(ptr, _) => (*ptr, false),
                _ => return,
            };
            if begin && self.suppress_undo.load(Ordering::Relaxed) > 0 {
                self.suppress_undo.fetch_sub(1, Ordering::Relaxed);
                return;
            }
            let params = &self.params;
            let (id, value) = if ptr == params.snare_tightness.as_ptr() {
                ("snare_tightness", params.snare_tightness.value())
            } else if ptr == params.snare_decay.as_ptr() {
                ("snare_decay", params.snare_decay.value())
            } else if ptr == params.hihat_open.as_ptr() {
                ("hihat_open", params.hihat_open.value())
            } else if ptr == params.cymbal_decay.as_ptr() {
                ("cymbal_decay", params.cymbal_decay.value())
            } else if ptr == params.master_gain.as_ptr() {
                ("gain", params.master_gain.value())
            } else {
                return;
            };
            if let RawParamEvent::SetParameterNormalized(_, normalized) = raw {
                self.undo
                    .write()
                    .update_gesture_value(id, unsafe { ptr.preview_plain(*normalized) });
                return;
            }
            if begin {
                self.undo.write().begin_gesture(id, value);
            } else {
                let changed = if wheel {
                    self.undo.write().end_wheel_gesture(id, value)
                } else {
                    self.undo.write().end_gesture(id, value)
                };
                if changed {
                    self.update_history_state();
                }
            }
        });
    }
}

pub fn create_vizia_drum_editor(
    params: Arc<PhysicsDrumParams>,
    voice_energies: Arc<[std::sync::atomic::AtomicU32; 10]>,
    gui_tx: crossbeam_channel::Sender<GuiDrumEvent>,
    language_atom: Arc<AtomicU8>,
    preset_manager: Arc<parking_lot::RwLock<PresetManager>>,
    undo_manager: Arc<parking_lot::RwLock<UndoManager>>,
) -> Option<Box<dyn Editor>> {
    let language_initial = if language_atom.load(Ordering::Relaxed) == 1 {
        Language::SimplifiedChinese
    } else {
        Language::English
    };
    let selected_id = "drum_studio_kit".to_string();
    let selected_name = preset_manager
        .read()
        .get_preset(&selected_id)
        .map(|preset| {
            preset
                .display_name(language_initial == Language::SimplifiedChinese)
                .to_string()
        })
        .unwrap_or_else(|| "Studio Kit".to_string());

    let editor_state = params.editor_state.clone();
    create_vizia_editor(editor_state, ViziaTheming::Custom, move |cx, _| {
        setup_vizia_fonts(cx);
        if let Err(error) = add_base_theme(cx) {
            eprintln!("Failed to load shared Vizia theme: {error:?}");
        }
        let selected_name_signal = Signal::new(selected_name.clone());
        let selected_id_signal = Signal::new(selected_id.clone());
        let preset_choices_signal =
            Signal::new(preset_choices(&preset_manager.read(), language_initial));
        let language = Signal::new(language_initial);
        let name_input = Signal::new(selected_name.clone());
        let is_user_preset = Signal::new(false);
        let can_undo = Signal::new(undo_manager.read().can_undo());
        let can_redo = Signal::new(undo_manager.read().can_redo());

        DrumUiState {
            params: params.clone(),
            manager: preset_manager.clone(),
            undo: undo_manager.clone(),
            selected_name: selected_name_signal.clone(),
            selected_id: selected_id_signal,
            preset_choices: preset_choices_signal.clone(),
            language: language.clone(),
            language_atom: language_atom.clone(),
            name_input: name_input.clone(),
            is_user_preset: is_user_preset.clone(),
            can_undo: can_undo.clone(),
            can_redo: can_redo.clone(),
            suppress_undo: Arc::new(AtomicU32::new(0)),
        }
        .build(cx);

        let display_timer = cx.add_timer(std::time::Duration::from_millis(33), None, |cx, _| {
            cx.emit(DrumUiEvent::RefreshDisplay)
        });
        cx.start_timer(display_timer);

        let params_ui = params.clone();
        let language_ui = language.clone();
        let language_view = language.clone();
        let voice_energies_ui = voice_energies.clone();
        let gui_tx_ui = gui_tx.clone();
        let selected_name_ui = selected_name_signal;
        let preset_choices_ui = preset_choices_signal;
        let name_input_ui = name_input;
        let is_user_preset_ui = is_user_preset;
        let can_undo_ui = can_undo;
        let can_redo_ui = can_redo;
        let language_atom_ui = language_atom.clone();

        Binding::new(cx, language_ui, move |cx| {
            let lang = language_ui.get();
            let params = params_ui.clone();
            let language_view = language_view.clone();
            let voice_energies = voice_energies_ui.clone();
            let gui_tx = gui_tx_ui.clone();
            let language_atom = language_atom_ui.clone();
            VStack::new(cx, move |cx| {
                preset_panel(
                    cx,
                    lang,
                    PresetPanelSignals {
                        selected_name: selected_name_ui.clone(),
                        preset_choices: preset_choices_ui.clone(),
                        language: language_view,
                        name_input: name_input_ui.clone(),
                        is_user_preset: is_user_preset_ui.clone(),
                        can_undo: can_undo_ui.clone(),
                        can_redo: can_redo_ui.clone(),
                    },
                    PresetPanelLayout {
                        preset_label: Some(I18n::preset(lang)),
                        preset_width: 180.0,
                        name_width: 120.0,
                        save_as_width: 62.0,
                        rename_width: 58.0,
                        overwrite_width: 68.0,
                        delete_width: 48.0,
                    },
                    |_| {},
                );

                HStack::new(cx, |cx| {
                    VStack::new(cx, |cx| {
                        Label::new(cx, I18n::title(lang)).class("title");
                        Label::new(cx, I18n::subtitle(lang)).class("subtitle");
                    })
                    .width(Stretch(1.0));
                })
                .class("header")
                .width(Stretch(1.0));

                HStack::new(cx, |cx| {
                    VStack::new(cx, |cx| {
                        Label::new(cx, I18n::snare_membrane(lang)).class("rack-title");
                        parameter_slider(
                            cx,
                            I18n::snare_tightness(lang),
                            &params.snare_tightness,
                            112.0,
                        );
                        parameter_slider(cx, I18n::snare_decay(lang), &params.snare_decay, 112.0);
                        Label::new(cx, I18n::snare_note(lang)).class("small-note");
                    })
                    .class("rack-box")
                    .width(Stretch(1.0));
                    VStack::new(cx, |cx| {
                        Label::new(cx, I18n::cymbal_output(lang)).class("rack-title");
                        parameter_slider(cx, I18n::hihat_open(lang), &params.hihat_open, 112.0);
                        parameter_slider(cx, I18n::cymbal_decay(lang), &params.cymbal_decay, 112.0);
                        parameter_slider(cx, I18n::master_gain(lang), &params.master_gain, 112.0);
                    })
                    .class("rack-box")
                    .width(Stretch(1.0));
                })
                .class("rack-row")
                .width(Stretch(1.0))
                .height(Pixels(100.0))
                .horizontal_gap(Pixels(8.0));

                DrumPadWidget::new(cx, voice_energies, gui_tx, language_atom)
                    .class("instrument-view")
                    .width(Stretch(1.0))
                    .height(Pixels(350.0))
                    .overflow(Overflow::Hidden);
                Label::new(cx, I18n::hint(lang)).class("hint-text");
            })
            .class("root")
            .width(Stretch(1.0))
            .height(Stretch(1.0));
        });
    })
}
