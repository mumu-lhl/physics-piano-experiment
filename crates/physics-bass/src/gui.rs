//! Vizia editor for the physical bass.
//!
//! The layout follows the piano/guitar editors: preset and language controls at
//! the top, parameter racks in the middle, and the playable instrument view at
//! the bottom.  The fretboard itself is a custom Skia view, not a collection of
//! tiny buttons, so it remains usable at plugin-sized window dimensions.

mod fretboard_view;
mod i18n;

use crate::nice_plugin::{GuiBassEvent, PhysicsBassParams};
use fretboard_view::BassFretboardWidget;
use i18n::{I18n, Language};
use nice_plug::prelude::{Editor, Param};
use physics_presets::{ParamTransition, Preset, PresetManager, UndoManager};
use physics_ui::{
    PresetPanelAction, PresetPanelLayout, PresetPanelSignals, discrete_selector, parameter_slider,
    preset_choices, preset_panel, redraw_custom_view, set_param, setup_vizia_fonts,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU32, Ordering};
use vizia_plug::vizia::prelude::*;
use vizia_plug::widgets::{ParamButton, ParamButtonExt, RawParamEvent};
use vizia_plug::{ViziaTheming, create_vizia_editor};

pub const EDITOR_WIDTH: u32 = 1120;
pub const EDITOR_HEIGHT: u32 = 650;

#[derive(Clone)]
struct DiscreteSelections {
    mode: Signal<i32>,
    pluck_style: Signal<i32>,
}

#[derive(Debug)]
enum BassUiEvent {
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

fn bass_value(params: &PhysicsBassParams, id: &str) -> Option<f32> {
    Some(match id {
        "mode" => params.mode.value() as f32,
        "pluck_style" => params.pluck_style.value() as f32,
        "five_string" => u8::from(params.five_string.value()) as f32,
        "pickup_position" => params.pickup_position.value(),
        "pluck_position" => params.pluck_position.value(),
        "tone" => params.tone.value(),
        "fret_buzz" => params.fret_buzz.value(),
        "body_mix" => params.body_mix.value(),
        "gain" => params.master_gain.value(),
        _ => return None,
    })
}

fn set_bass_value(cx: &mut EventContext, params: &PhysicsBassParams, id: &str, value: f32) {
    match id {
        "mode" => set_param(cx, &params.mode, value.round() as i32),
        "pluck_style" => set_param(cx, &params.pluck_style, value.round() as i32),
        "five_string" => set_param(cx, &params.five_string, value >= 0.5),
        "pickup_position" => set_param(cx, &params.pickup_position, value),
        "pluck_position" => set_param(cx, &params.pluck_position, value),
        "tone" => set_param(cx, &params.tone, value),
        "fret_buzz" => set_param(cx, &params.fret_buzz, value),
        "body_mix" => set_param(cx, &params.body_mix, value),
        "gain" => set_param(cx, &params.master_gain, value),
        _ => {}
    }
}

fn bass_snapshot(params: &PhysicsBassParams) -> HashMap<String, f32> {
    [
        "mode",
        "pluck_style",
        "five_string",
        "pickup_position",
        "pluck_position",
        "tone",
        "fret_buzz",
        "body_mix",
        "gain",
    ]
    .into_iter()
    .filter_map(|id| bass_value(params, id).map(|value| (id.to_string(), value)))
    .collect()
}

struct BassUiState {
    params: Arc<PhysicsBassParams>,
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
    discrete: DiscreteSelections,
}

impl BassUiState {
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
                set_bass_value(cx, &self.params, &id, value);
            }
        }
        self.update_history_state();
    }
}

impl Model for BassUiState {
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|action: &PresetPanelAction, _| match action {
            PresetPanelAction::Previous => cx.emit(BassUiEvent::PreviousPreset),
            PresetPanelAction::Next => cx.emit(BassUiEvent::NextPreset),
            PresetPanelAction::ToggleLanguage => cx.emit(BassUiEvent::ToggleLanguage),
            PresetPanelAction::Select(id) => cx.emit(BassUiEvent::SelectPreset(id.clone())),
            PresetPanelAction::SetName(name) => cx.emit(BassUiEvent::SetName(name.clone())),
            PresetPanelAction::SaveAs => cx.emit(BassUiEvent::SaveAs),
            PresetPanelAction::Rename => cx.emit(BassUiEvent::Rename),
            PresetPanelAction::Overwrite => cx.emit(BassUiEvent::Overwrite),
            PresetPanelAction::Delete => cx.emit(BassUiEvent::Delete),
            PresetPanelAction::Undo => cx.emit(BassUiEvent::Undo),
            PresetPanelAction::Redo => cx.emit(BassUiEvent::Redo),
        });

        event.map(|action: &BassUiEvent, _| match action {
            BassUiEvent::RefreshDisplay => {
                self.discrete.mode.set(self.params.mode.value());
                self.discrete
                    .pluck_style
                    .set(self.params.pluck_style.value());
                redraw_custom_view(cx, "bass-fretboard-widget");
            }
            BassUiEvent::PreviousPreset | BassUiEvent::NextPreset => {
                let next = matches!(action, BassUiEvent::NextPreset);
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
                cx.emit(BassUiEvent::SelectPreset(id));
            }
            BassUiEvent::SelectPreset(id) => {
                let preset = self.manager.read().get_preset(id).cloned();
                if let Some(preset) = preset {
                    let mut changes = Vec::new();
                    for (param_id, value) in &preset.params {
                        if let Some(old_value) = bass_value(&self.params, param_id) {
                            if (old_value - value).abs() > 1e-5 {
                                changes.push(ParamTransition {
                                    param_id: param_id.clone(),
                                    old_value,
                                    new_value: *value,
                                });
                                self.suppress_undo.fetch_add(1, Ordering::Relaxed);
                                set_bass_value(cx, &self.params, param_id, *value);
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
            BassUiEvent::ToggleLanguage => {
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
                redraw_custom_view(cx, "bass-fretboard-widget");
            }
            BassUiEvent::SetName(name) => self.name_input.set(name.clone()),
            BassUiEvent::SaveAs => {
                let name = self.name_input.get().trim().to_string();
                let name = if name.is_empty() {
                    "Custom Bass".to_string()
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
                    "bass",
                    bass_snapshot(&self.params),
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
            BassUiEvent::Rename => {
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
            BassUiEvent::Overwrite => {
                if self.is_user_preset.get() {
                    let _ = self.manager.write().overwrite_user_preset(
                        &self.selected_id.get(),
                        bass_snapshot(&self.params),
                    );
                }
            }
            BassUiEvent::Delete => {
                let id = self.selected_id.get();
                if self
                    .manager
                    .write()
                    .delete_user_preset(&id)
                    .unwrap_or(false)
                {
                    self.preset_choices
                        .set(preset_choices(&self.manager.read(), self.language.get()));
                    cx.emit(BassUiEvent::SelectPreset("bass_finger_punch".to_string()));
                }
            }
            BassUiEvent::Undo => self.apply_history(cx, false),
            BassUiEvent::Redo => self.apply_history(cx, true),
        });

        event.map(|raw: &RawParamEvent, _| {
            let (ptr, begin) = match raw {
                RawParamEvent::BeginSetParameter(ptr) => (*ptr, true),
                RawParamEvent::EndSetParameter(ptr) => (*ptr, false),
                _ => return,
            };
            if begin && self.suppress_undo.load(Ordering::Relaxed) > 0 {
                self.suppress_undo.fetch_sub(1, Ordering::Relaxed);
                return;
            }
            let params = &self.params;
            let (id, value) = if ptr == params.mode.as_ptr() {
                ("mode", params.mode.value() as f32)
            } else if ptr == params.pluck_style.as_ptr() {
                ("pluck_style", params.pluck_style.value() as f32)
            } else if ptr == params.five_string.as_ptr() {
                ("five_string", u8::from(params.five_string.value()) as f32)
            } else if ptr == params.pickup_position.as_ptr() {
                ("pickup_position", params.pickup_position.value())
            } else if ptr == params.pluck_position.as_ptr() {
                ("pluck_position", params.pluck_position.value())
            } else if ptr == params.tone.as_ptr() {
                ("tone", params.tone.value())
            } else if ptr == params.fret_buzz.as_ptr() {
                ("fret_buzz", params.fret_buzz.value())
            } else if ptr == params.body_mix.as_ptr() {
                ("body_mix", params.body_mix.value())
            } else if ptr == params.master_gain.as_ptr() {
                ("gain", params.master_gain.value())
            } else {
                return;
            };
            if begin {
                self.undo.write().begin_gesture(id, value);
            } else if self.undo.write().end_gesture(id, value) {
                self.update_history_state();
            }
        });
    }
}

pub fn create_vizia_bass_editor(
    params: Arc<PhysicsBassParams>,
    active_frets: Arc<[std::sync::atomic::AtomicU8; 5]>,
    string_energies: Arc<[std::sync::atomic::AtomicU32; 5]>,
    gui_tx: crossbeam_channel::Sender<GuiBassEvent>,
    language_atom: Arc<AtomicU8>,
    preset_manager: Arc<parking_lot::RwLock<PresetManager>>,
    undo_manager: Arc<parking_lot::RwLock<UndoManager>>,
) -> Option<Box<dyn Editor>> {
    let language_initial = if language_atom.load(Ordering::Relaxed) == 1 {
        Language::SimplifiedChinese
    } else {
        Language::English
    };
    let selected_id = "bass_finger_punch".to_string();
    let selected_name = preset_manager
        .read()
        .get_preset(&selected_id)
        .map(|preset| {
            preset
                .display_name(language_initial == Language::SimplifiedChinese)
                .to_string()
        })
        .unwrap_or_else(|| "Finger Punch".to_string());

    let editor_state = params.editor_state.clone();
    create_vizia_editor(editor_state, ViziaTheming::Custom, move |cx, _| {
        setup_vizia_fonts(cx);
        if let Err(error) = cx.add_stylesheet(include_style!("src/gui/theme.css")) {
            eprintln!("Failed to load bass Vizia stylesheet: {error:?}");
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
        let discrete = DiscreteSelections {
            mode: Signal::new(params.mode.value()),
            pluck_style: Signal::new(params.pluck_style.value()),
        };

        BassUiState {
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
            discrete: discrete.clone(),
        }
        .build(cx);

        let display_timer = cx.add_timer(std::time::Duration::from_millis(33), None, |cx, _| {
            cx.emit(BassUiEvent::RefreshDisplay)
        });
        cx.start_timer(display_timer);

        let params_ui = params.clone();
        let language_ui = language.clone();
        let language_view = language.clone();
        let active_frets_ui = active_frets.clone();
        let string_energies_ui = string_energies.clone();
        let gui_tx_ui = gui_tx.clone();
        let selected_name_ui = selected_name_signal;
        let preset_choices_ui = preset_choices_signal;
        let name_input_ui = name_input;
        let is_user_preset_ui = is_user_preset;
        let can_undo_ui = can_undo;
        let can_redo_ui = can_redo;
        let discrete_ui = discrete;
        let language_atom_ui = language_atom.clone();

        Binding::new(cx, language_ui, move |cx| {
            let lang = language_ui.get();
            let params = params_ui.clone();
            let language_view = language_view.clone();
            let discrete = discrete_ui.clone();
            let active_frets = active_frets_ui.clone();
            let string_energies = string_energies_ui.clone();
            let gui_tx = gui_tx_ui.clone();
            let language_atom = language_atom_ui.clone();
            VStack::new(cx, move |cx| {
                HStack::new(cx, |cx| {
                    VStack::new(cx, |cx| {
                        Label::new(cx, I18n::title(lang)).class("title");
                        Label::new(cx, I18n::subtitle(lang)).class("subtitle");
                    })
                    .width(Stretch(1.0));
                    Label::new(cx, I18n::audition(lang)).class("status");
                })
                .class("header")
                .width(Stretch(1.0));

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
                        preset_width: 190.0,
                        name_width: 130.0,
                        save_as_width: 64.0,
                        rename_width: 58.0,
                        overwrite_width: 68.0,
                        delete_width: 48.0,
                    },
                    move |cx| {
                        Label::new(cx, I18n::audition(lang)).class("status");
                    },
                );

                HStack::new(cx, |cx| {
                    VStack::new(cx, |cx| {
                        Label::new(cx, I18n::instrument(lang)).class("rack-title");
                        discrete_selector(
                            cx,
                            I18n::model(lang),
                            discrete.mode.clone(),
                            vec![
                                (0, I18n::electric(lang).to_string()),
                                (1, I18n::acoustic(lang).to_string()),
                            ],
                            94.0,
                            {
                                let params = params.clone();
                                move |cx, value| set_param(cx, &params.mode, value)
                            },
                        );
                        discrete_selector(
                            cx,
                            I18n::exciter(lang),
                            discrete.pluck_style.clone(),
                            vec![
                                (0, I18n::finger(lang).to_string()),
                                (1, I18n::pick(lang).to_string()),
                                (2, I18n::slap(lang).to_string()),
                            ],
                            94.0,
                            {
                                let params = params.clone();
                                move |cx, value| set_param(cx, &params.pluck_style, value)
                            },
                        );
                        HStack::new(cx, |cx| {
                            Label::new(cx, I18n::tuning(lang)).class("param-label");
                            ParamButton::new(cx, &params.five_string)
                                .with_label(I18n::five_string(lang));
                        })
                        .horizontal_gap(Pixels(6.0));
                    })
                    .class("rack-box")
                    .width(Stretch(1.0));

                    VStack::new(cx, |cx| {
                        Label::new(cx, I18n::string_pickup(lang)).class("rack-title");
                        parameter_slider(
                            cx,
                            I18n::pluck_position(lang),
                            &params.pluck_position,
                            108.0,
                        );
                        parameter_slider(
                            cx,
                            I18n::pickup_position(lang),
                            &params.pickup_position,
                            108.0,
                        );
                        parameter_slider(cx, I18n::tone(lang), &params.tone, 108.0);
                    })
                    .class("rack-box")
                    .width(Stretch(1.0));

                    VStack::new(cx, |cx| {
                        Label::new(cx, I18n::contact_body(lang)).class("rack-title");
                        parameter_slider(cx, I18n::fret_buzz(lang), &params.fret_buzz, 108.0);
                        parameter_slider(cx, I18n::body_mix(lang), &params.body_mix, 108.0);
                        Label::new(cx, I18n::slap_note(lang)).class("small-note");
                    })
                    .class("rack-box")
                    .width(Stretch(1.0));

                    VStack::new(cx, |cx| {
                        Label::new(cx, I18n::output(lang)).class("rack-title");
                        parameter_slider(cx, I18n::master_gain(lang), &params.master_gain, 108.0);
                        Label::new(cx, I18n::output_note(lang)).class("small-note");
                    })
                    .class("rack-box")
                    .width(Stretch(1.0));
                })
                .class("rack-row")
                .width(Stretch(1.0))
                .height(Pixels(145.0))
                .horizontal_gap(Pixels(8.0));

                Label::new(cx, I18n::fretboard_hint(lang)).class("hint-text");
                BassFretboardWidget::new(
                    cx,
                    params,
                    active_frets,
                    string_energies,
                    gui_tx,
                    language_atom,
                )
                .class("instrument-view")
                .width(Stretch(1.0))
                .height(Pixels(300.0))
                .overflow(Overflow::Hidden);
            })
            .class("root")
            .width(Stretch(1.0))
            .height(Stretch(1.0));
        });
    })
}
