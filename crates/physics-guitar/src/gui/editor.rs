//! Vizia Plug editor for the physical guitar.

use crossbeam_channel::Sender;
use nice_plug::prelude::{Editor, Param};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU32, Ordering};
use vizia_plug::vizia::prelude::*;
use vizia_plug::widgets::util::ModifiersExt;
use vizia_plug::widgets::*;
use vizia_plug::{ViziaState, ViziaTheming, create_vizia_editor};

use crate::gui::fretboard_view::GuitarFretboardWidget;
use crate::gui::i18n::{I18n, Language, setup_vizia_fonts};
use crate::nice_plugin::{GuiGuitarEvent, PhysicsGuitarParams};
use physics_presets::{ParamTransition, Preset, PresetManager, UndoManager};
use physics_ui::{
    CancelParamGestureEvent, PresetPanelAction, PresetPanelLayout, PresetPanelSignals,
    add_base_theme, discrete_selector as shared_discrete_selector, map_param_history_event,
    parameter_slider, preset_choices, preset_panel, redraw_custom_view, set_param, ui_text,
};
use std::collections::HashMap;

#[derive(Clone)]
struct DiscreteSelections {
    mode: Signal<i32>,
    pluck_style: Signal<i32>,
    pickup_pos: Signal<i32>,
    pickup_type: Signal<i32>,
    groove_pattern: Signal<i32>,
}

#[derive(Clone, Copy)]
enum DiscreteParam {
    Mode,
    PluckStyle,
    PickupPosition,
    PickupType,
    GroovePattern,
}

pub const EDITOR_WIDTH: u32 = 1080;
pub const EDITOR_HEIGHT: u32 = 560;

fn slider<P: Param + 'static>(cx: &mut Context, label: &'static str, param: &P) {
    parameter_slider(cx, label, param, 92.0);
}

fn discrete_selector(
    cx: &mut Context,
    label: &'static str,
    selected: Signal<i32>,
    options: Vec<(i32, String)>,
    params: Arc<PhysicsGuitarParams>,
    param: DiscreteParam,
) {
    shared_discrete_selector(cx, label, selected, options, 92.0, move |cx, value| {
        set_discrete_param(cx, &params, param, value);
    });
}

fn set_discrete_param(
    cx: &mut EventContext,
    params: &PhysicsGuitarParams,
    param: DiscreteParam,
    value: i32,
) {
    match param {
        DiscreteParam::Mode => set_param(cx, &params.mode, value),
        DiscreteParam::PluckStyle => set_param(cx, &params.pluck_style, value),
        DiscreteParam::PickupPosition => set_param(cx, &params.pickup_pos, value),
        DiscreteParam::PickupType => set_param(cx, &params.pickup_type, value),
        DiscreteParam::GroovePattern => set_param(cx, &params.groove_pattern, value),
    }
}

struct GuitarUiState {
    params: Arc<PhysicsGuitarParams>,
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

#[derive(Debug)]
enum GuitarUiEvent {
    PreviousPreset,
    NextPreset,
    ToggleLanguage,
    SetName(String),
    SelectPreset(String),
    SaveAs,
    Rename,
    Overwrite,
    Delete,
    Undo,
    Redo,
    RefreshDisplay,
}

fn guitar_value(params: &PhysicsGuitarParams, id: &str) -> Option<f32> {
    Some(match id {
        "mode" => params.mode.value() as f32,
        "pluck_style" => params.pluck_style.value() as f32,
        "pickup_pos" => params.pickup_pos.value() as f32,
        "pickup_type" => params.pickup_type.value() as f32,
        "tone" => params.tone.value(),
        "palmmute" => params.palm_mute.value(),
        "pluckpos" => params.pluck_pos.value(),
        "amp_drive" => params.amp_drive.value(),
        "cab_enabled" => {
            if params.cab_enabled.value() {
                1.0
            } else {
                0.0
            }
        }
        "strum_speed" => params.strum_speed.value(),
        "fret_buzz" => params.fret_buzz.value(),
        "finger_squeak" => params.finger_squeak.value(),
        "groove_pattern" => params.groove_pattern.value() as f32,
        "groove_bpm" => params.groove_bpm.value(),
        "gain" => params.master_gain.value(),
        _ => return None,
    })
}

fn set_guitar_value(cx: &mut EventContext, params: &PhysicsGuitarParams, id: &str, value: f32) {
    match id {
        "mode" => set_param(cx, &params.mode, value.round() as i32),
        "pluck_style" => set_param(cx, &params.pluck_style, value.round() as i32),
        "pickup_pos" => set_param(cx, &params.pickup_pos, value.round() as i32),
        "pickup_type" => set_param(cx, &params.pickup_type, value.round() as i32),
        "tone" => set_param(cx, &params.tone, value),
        "palmmute" => set_param(cx, &params.palm_mute, value),
        "pluckpos" => set_param(cx, &params.pluck_pos, value),
        "amp_drive" => set_param(cx, &params.amp_drive, value),
        "cab_enabled" => set_param(cx, &params.cab_enabled, value >= 0.5),
        "strum_speed" => set_param(cx, &params.strum_speed, value),
        "fret_buzz" => set_param(cx, &params.fret_buzz, value),
        "finger_squeak" => set_param(cx, &params.finger_squeak, value),
        "groove_pattern" => set_param(cx, &params.groove_pattern, value.round() as i32),
        "groove_bpm" => set_param(cx, &params.groove_bpm, value),
        "gain" => set_param(cx, &params.master_gain, value),
        _ => {}
    }
}

fn guitar_snapshot(params: &PhysicsGuitarParams) -> HashMap<String, f32> {
    [
        "mode",
        "pluck_style",
        "pickup_pos",
        "pickup_type",
        "tone",
        "palmmute",
        "pluckpos",
        "amp_drive",
        "cab_enabled",
        "strum_speed",
        "fret_buzz",
        "finger_squeak",
        "groove_pattern",
        "groove_bpm",
        "gain",
    ]
    .into_iter()
    .filter_map(|id| guitar_value(params, id).map(|value| (id.to_string(), value)))
    .collect()
}

impl GuitarUiState {
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
                set_guitar_value(cx, &self.params, &id, value);
            }
        }
        self.update_history_state();
    }
}

impl Model for GuitarUiState {
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|action: &PresetPanelAction, _| match action {
            PresetPanelAction::Previous => cx.emit(GuitarUiEvent::PreviousPreset),
            PresetPanelAction::Next => cx.emit(GuitarUiEvent::NextPreset),
            PresetPanelAction::ToggleLanguage => cx.emit(GuitarUiEvent::ToggleLanguage),
            PresetPanelAction::Select(id) => cx.emit(GuitarUiEvent::SelectPreset(id.clone())),
            PresetPanelAction::SetName(name) => cx.emit(GuitarUiEvent::SetName(name.clone())),
            PresetPanelAction::SaveAs => cx.emit(GuitarUiEvent::SaveAs),
            PresetPanelAction::Rename => cx.emit(GuitarUiEvent::Rename),
            PresetPanelAction::Overwrite => cx.emit(GuitarUiEvent::Overwrite),
            PresetPanelAction::Delete => cx.emit(GuitarUiEvent::Delete),
            PresetPanelAction::Undo => cx.emit(GuitarUiEvent::Undo),
            PresetPanelAction::Redo => cx.emit(GuitarUiEvent::Redo),
        });
        event.map(|event, _| match event {
            GuitarUiEvent::RefreshDisplay => {
                self.discrete.mode.set(self.params.mode.value());
                self.discrete
                    .pluck_style
                    .set(self.params.pluck_style.value());
                self.discrete.pickup_pos.set(self.params.pickup_pos.value());
                self.discrete
                    .pickup_type
                    .set(self.params.pickup_type.value());
                self.discrete
                    .groove_pattern
                    .set(self.params.groove_pattern.value());
                redraw_custom_view(cx, "guitar-fretboard-widget");
            }
            GuitarUiEvent::PreviousPreset | GuitarUiEvent::NextPreset => {
                let next = matches!(event, GuitarUiEvent::NextPreset);
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
                cx.emit(GuitarUiEvent::SelectPreset(id));
            }
            GuitarUiEvent::SelectPreset(id) => {
                let preset = self.manager.read().get_preset(id).cloned();
                if let Some(preset) = preset {
                    let mut changes = Vec::new();
                    for (pid, value) in &preset.params {
                        if let Some(old) = guitar_value(&self.params, pid) {
                            if (old - value).abs() > 1e-5 {
                                changes.push(ParamTransition {
                                    param_id: pid.clone(),
                                    old_value: old,
                                    new_value: *value,
                                });
                                self.suppress_undo.fetch_add(1, Ordering::Relaxed);
                                set_guitar_value(cx, &self.params, pid, *value);
                            }
                        }
                    }
                    if !changes.is_empty() {
                        self.undo.write().record_batch(&preset.name, changes);
                    }
                    self.selected_id.set(preset.id.clone());
                    self.selected_name.set(
                        preset
                            .display_name(self.language.get() == Language::SimplifiedChinese)
                            .to_string(),
                    );
                    self.name_input.set(
                        preset
                            .display_name(self.language.get() == Language::SimplifiedChinese)
                            .to_string(),
                    );
                    self.is_user_preset
                        .set(self.manager.read().is_user_preset(id));
                    self.preset_choices
                        .set(preset_choices(&self.manager.read(), self.language.get()));
                    self.update_history_state();
                }
            }
            GuitarUiEvent::ToggleLanguage => {
                let lang = if self.language.get() == Language::English {
                    Language::SimplifiedChinese
                } else {
                    Language::English
                };
                self.language.set(lang);
                self.language_atom.store(
                    u8::from(lang == Language::SimplifiedChinese),
                    Ordering::Relaxed,
                );
                self.preset_choices
                    .set(preset_choices(&self.manager.read(), lang));
                if let Some(preset) = self.manager.read().get_preset(&self.selected_id.get()) {
                    let display = preset
                        .display_name(lang == Language::SimplifiedChinese)
                        .to_string();
                    self.selected_name.set(display.clone());
                    self.name_input.set(display);
                }
            }
            GuitarUiEvent::SetName(name) => self.name_input.set(name.clone()),
            GuitarUiEvent::SaveAs => {
                let name = self.name_input.get().trim().to_string();
                let name = if name.is_empty() {
                    "Custom Guitar".to_string()
                } else {
                    name
                };
                let id = format!(
                    "user_{}",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_nanos())
                        .unwrap_or_default()
                );
                let preset = Preset::new(
                    id.clone(),
                    name.clone(),
                    "guitar",
                    guitar_snapshot(&self.params),
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
            GuitarUiEvent::Rename => {
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
            GuitarUiEvent::Overwrite => {
                if self.is_user_preset.get() {
                    let _ = self.manager.write().overwrite_user_preset(
                        &self.selected_id.get(),
                        guitar_snapshot(&self.params),
                    );
                }
            }
            GuitarUiEvent::Delete => {
                let id = self.selected_id.get();
                if self
                    .manager
                    .write()
                    .delete_user_preset(&id)
                    .unwrap_or(false)
                {
                    self.preset_choices
                        .set(preset_choices(&self.manager.read(), self.language.get()));
                    cx.emit(GuitarUiEvent::SelectPreset("strat_clean_chime".to_string()));
                }
            }
            GuitarUiEvent::Undo => self.apply_history(cx, false),
            GuitarUiEvent::Redo => self.apply_history(cx, true),
        });

        event.map(|cancel: &CancelParamGestureEvent, _| {
            let ptr = cancel.param;
            let params = &self.params;
            let id = if ptr == params.mode.as_ptr() {
                "mode"
            } else if ptr == params.pluck_style.as_ptr() {
                "pluck_style"
            } else if ptr == params.pickup_pos.as_ptr() {
                "pickup_pos"
            } else if ptr == params.pickup_type.as_ptr() {
                "pickup_type"
            } else if ptr == params.tone.as_ptr() {
                "tone"
            } else if ptr == params.palm_mute.as_ptr() {
                "palmmute"
            } else if ptr == params.pluck_pos.as_ptr() {
                "pluckpos"
            } else if ptr == params.amp_drive.as_ptr() {
                "amp_drive"
            } else if ptr == params.cab_enabled.as_ptr() {
                "cab_enabled"
            } else if ptr == params.strum_speed.as_ptr() {
                "strum_speed"
            } else if ptr == params.fret_buzz.as_ptr() {
                "fret_buzz"
            } else if ptr == params.finger_squeak.as_ptr() {
                "finger_squeak"
            } else if ptr == params.groove_pattern.as_ptr() {
                "groove_pattern"
            } else if ptr == params.groove_bpm.as_ptr() {
                "groove_bpm"
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
            let (id, value) = if ptr == params.mode.as_ptr() {
                ("mode", params.mode.value() as f32)
            } else if ptr == params.pluck_style.as_ptr() {
                ("pluck_style", params.pluck_style.value() as f32)
            } else if ptr == params.pickup_pos.as_ptr() {
                ("pickup_pos", params.pickup_pos.value() as f32)
            } else if ptr == params.pickup_type.as_ptr() {
                ("pickup_type", params.pickup_type.value() as f32)
            } else if ptr == params.tone.as_ptr() {
                ("tone", params.tone.value())
            } else if ptr == params.palm_mute.as_ptr() {
                ("palmmute", params.palm_mute.value())
            } else if ptr == params.pluck_pos.as_ptr() {
                ("pluckpos", params.pluck_pos.value())
            } else if ptr == params.amp_drive.as_ptr() {
                ("amp_drive", params.amp_drive.value())
            } else if ptr == params.cab_enabled.as_ptr() {
                (
                    "cab_enabled",
                    if params.cab_enabled.value() { 1.0 } else { 0.0 },
                )
            } else if ptr == params.strum_speed.as_ptr() {
                ("strum_speed", params.strum_speed.value())
            } else if ptr == params.fret_buzz.as_ptr() {
                ("fret_buzz", params.fret_buzz.value())
            } else if ptr == params.finger_squeak.as_ptr() {
                ("finger_squeak", params.finger_squeak.value())
            } else if ptr == params.groove_pattern.as_ptr() {
                ("groove_pattern", params.groove_pattern.value() as f32)
            } else if ptr == params.groove_bpm.as_ptr() {
                ("groove_bpm", params.groove_bpm.value())
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

        event.map(|window: &WindowEvent, _| {
            if let WindowEvent::KeyDown(code, _) = window {
                if cx.modifiers().command() {
                    if *code == Code::KeyZ && !cx.modifiers().shift() {
                        cx.emit(GuitarUiEvent::Undo);
                    } else if *code == Code::KeyY || (*code == Code::KeyZ && cx.modifiers().shift())
                    {
                        cx.emit(GuitarUiEvent::Redo);
                    }
                }
            }
        });
    }
}

pub fn default_vizia_state() -> Arc<ViziaState> {
    ViziaState::new(|| (EDITOR_WIDTH, EDITOR_HEIGHT))
}

#[allow(clippy::too_many_arguments)]
pub fn create_vizia_guitar_editor(
    params: Arc<PhysicsGuitarParams>,
    active_frets_shared: Arc<[AtomicU8; 6]>,
    string_energies_shared: Arc<[AtomicU32; 6]>,
    language_atom: Arc<AtomicU8>,
    gui_tx: Sender<GuiGuitarEvent>,
    preset_manager: Arc<parking_lot::RwLock<PresetManager>>,
    undo_manager: Arc<parking_lot::RwLock<UndoManager>>,
    editor_state: Arc<ViziaState>,
) -> Option<Box<dyn Editor>> {
    let lang = if language_atom.load(Ordering::Relaxed) == 1 {
        Language::SimplifiedChinese
    } else {
        Language::English
    };
    let selected_name = preset_manager
        .read()
        .get_preset("strat_clean_chime")
        .map(|preset| {
            preset
                .display_name(lang == Language::SimplifiedChinese)
                .to_string()
        })
        .unwrap_or_else(|| "Strat Clean Chime".to_string());
    create_vizia_editor(editor_state, ViziaTheming::Custom, move |cx, _| {
        // Signals must be created while Vizia is building this editor so their
        // scope remains alive for the lifetime of the view tree.
        let preset_name = Signal::new(selected_name.clone());
        let preset_id = Signal::new("strat_clean_chime".to_string());
        let preset_choices = Signal::new(preset_choices(&preset_manager.read(), lang));
        let language = Signal::new(lang);
        let language_view = language.clone();
        let language_ui = language.clone();
        let name_input = Signal::new(selected_name.clone());
        let is_user_preset = Signal::new(false);
        let can_undo = Signal::new(undo_manager.read().can_undo());
        let can_redo = Signal::new(undo_manager.read().can_redo());
        let discrete = DiscreteSelections {
            mode: Signal::new(params.mode.value()),
            pluck_style: Signal::new(params.pluck_style.value()),
            pickup_pos: Signal::new(params.pickup_pos.value()),
            pickup_type: Signal::new(params.pickup_type.value()),
            groove_pattern: Signal::new(params.groove_pattern.value()),
        };
        let is_user_preset_ui = is_user_preset.clone();
        let can_undo_ui = can_undo.clone();
        let can_redo_ui = can_redo.clone();
        let suppress_undo = Arc::new(AtomicU32::new(0));
        setup_vizia_fonts(cx);
        if let Err(err) = add_base_theme(cx) {
            eprintln!("Failed to load shared Vizia theme: {err:?}");
        }
        GuitarUiState {
            params: params.clone(),
            manager: preset_manager.clone(),
            undo: undo_manager.clone(),
            selected_name: preset_name.clone(),
            selected_id: preset_id,
            preset_choices: preset_choices.clone(),
            language,
            language_atom: language_atom.clone(),
            name_input: name_input.clone(),
            is_user_preset,
            can_undo,
            can_redo,
            suppress_undo,
            discrete: discrete.clone(),
        }
        .build(cx);
        let display_timer = cx.add_timer(std::time::Duration::from_millis(33), None, |cx, _| {
            cx.emit(GuitarUiEvent::RefreshDisplay)
        });
        cx.start_timer(display_timer);

        let params_ui = params.clone();
        let gui_tx_ui = gui_tx.clone();
        let active_frets_ui = active_frets_shared.clone();
        let string_energies_ui = string_energies_shared.clone();
        let name_input_ui = name_input.clone();
        let preset_name_ui = preset_name.clone();
        let preset_choices_ui = preset_choices.clone();
        let language_view_ui = language_view.clone();
        let is_user_preset_controls = is_user_preset_ui.clone();
        let can_undo_controls = can_undo_ui.clone();
        let can_redo_controls = can_redo_ui.clone();
        let discrete_controls = discrete.clone();

        Binding::new(cx, language_ui, move |cx| {
            let lang = language_ui.get();
            let params = params_ui.clone();
            let gui_tx = gui_tx_ui.clone();
            let active_frets_shared = active_frets_ui.clone();
            let string_energies_shared = string_energies_ui.clone();
            let initial_preset_name = preset_name_ui.clone();
            let preset_choices = preset_choices_ui.clone();
            let name_input = name_input_ui.clone();
            let language_view = language_view_ui.clone();
            let is_user_preset = is_user_preset_controls.clone();
            let can_undo = can_undo_controls.clone();
            let can_redo = can_redo_controls.clone();
            let discrete = discrete_controls.clone();
            VStack::new(cx, move |cx| {
                let mode_options = vec![
                    (0, I18n::mode_electric(lang).to_string()),
                    (1, I18n::mode_acoustic(lang).to_string()),
                ];
                let pluck_options = vec![
                    (0, I18n::pluck_plectrum(lang).to_string()),
                    (1, I18n::pluck_finger(lang).to_string()),
                ];
                let pickup_position_options = vec![
                    (0, I18n::pickup_bridge(lang).to_string()),
                    (1, I18n::pickup_mid(lang).to_string()),
                    (2, I18n::pickup_neck(lang).to_string()),
                    (3, ui_text(lang, "Bridge + Neck", "琴桥 + 琴颈").to_string()),
                    (4, ui_text(lang, "Bridge + Mid", "琴桥 + 中间").to_string()),
                ];
                let pickup_type_options = vec![
                    (0, I18n::pickup_single(lang).to_string()),
                    (1, I18n::pickup_humbucker(lang).to_string()),
                ];
                let groove_options = (0..=4)
                    .map(|index| (index, I18n::groove_name(index, lang).to_string()))
                    .collect();

                preset_panel(
                    cx,
                    lang,
                    PresetPanelSignals {
                        selected_name: initial_preset_name,
                        preset_choices,
                        language: language_view,
                        name_input,
                        is_user_preset,
                        can_undo,
                        can_redo,
                    },
                    PresetPanelLayout {
                        preset_label: Some(I18n::preset(lang)),
                        preset_width: 180.0,
                        name_width: 130.0,
                        save_as_width: 64.0,
                        rename_width: 58.0,
                        overwrite_width: 68.0,
                        delete_width: 48.0,
                    },
                    |_| {},
                );

                HStack::new(cx, |cx| {
                    VStack::new(cx, |cx| {
                        Label::new(cx, I18n::rack_instrument(lang)).class("rack-title");
                        discrete_selector(
                            cx,
                            I18n::mode_param_label(lang),
                            discrete.mode.clone(),
                            mode_options.clone(),
                            params.clone(),
                            DiscreteParam::Mode,
                        );
                        discrete_selector(
                            cx,
                            I18n::pluck_style_label(lang),
                            discrete.pluck_style.clone(),
                            pluck_options.clone(),
                            params.clone(),
                            DiscreteParam::PluckStyle,
                        );
                        discrete_selector(
                            cx,
                            I18n::pickup_pos_label(lang),
                            discrete.pickup_pos.clone(),
                            pickup_position_options.clone(),
                            params.clone(),
                            DiscreteParam::PickupPosition,
                        );
                        discrete_selector(
                            cx,
                            I18n::pickup_type_label(lang),
                            discrete.pickup_type.clone(),
                            pickup_type_options.clone(),
                            params.clone(),
                            DiscreteParam::PickupType,
                        );
                    })
                    .class("rack-box")
                    .width(Stretch(1.0));

                    VStack::new(cx, |cx| {
                        Label::new(cx, I18n::rack_pickup(lang)).class("rack-title");
                        slider(cx, I18n::tone_label(lang), &params.tone);
                        slider(cx, I18n::palm_mute_label(lang), &params.palm_mute);
                        slider(cx, I18n::pluck_pos_label(lang), &params.pluck_pos);
                        ParamButton::new(cx, &params.cab_enabled)
                            .with_label(I18n::cab_enabled(lang));
                    })
                    .class("rack-box")
                    .width(Stretch(1.0));

                    VStack::new(cx, |cx| {
                        Label::new(cx, I18n::rack_amp(lang)).class("rack-title");
                        slider(cx, I18n::amp_drive_label(lang), &params.amp_drive);
                        slider(cx, I18n::strum_speed_label(lang), &params.strum_speed);
                        slider(cx, I18n::fret_buzz_label(lang), &params.fret_buzz);
                        slider(cx, I18n::finger_squeak_label(lang), &params.finger_squeak);
                    })
                    .class("rack-box")
                    .width(Stretch(1.0));

                    VStack::new(cx, |cx| {
                        Label::new(cx, I18n::rack_master(lang)).class("rack-title");
                        discrete_selector(
                            cx,
                            I18n::groove_pattern_label(lang),
                            discrete.groove_pattern.clone(),
                            groove_options,
                            params.clone(),
                            DiscreteParam::GroovePattern,
                        );
                        slider(cx, I18n::bpm_label(lang), &params.groove_bpm);
                        slider(cx, I18n::master_gain_label(lang), &params.master_gain);
                    })
                    .class("rack-box")
                    .width(Stretch(1.0));
                })
                .height(Pixels(150.0))
                .horizontal_gap(Pixels(8.0));

                GuitarFretboardWidget::new(
                    cx,
                    gui_tx.clone(),
                    active_frets_shared.clone(),
                    string_energies_shared.clone(),
                )
                .height(Pixels(220.0))
                .width(Stretch(1.0));
                Label::new(cx, I18n::fretboard_hint(lang)).class("hint-text");
            });
        });
    })
}
