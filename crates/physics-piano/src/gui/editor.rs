//! Vizia Plug editor for the physical piano.

use atomic_float::AtomicF32;
use crossbeam_channel::Sender;
use nice_plug::prelude::{Editor, Param};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU64};
use vizia_plug::vizia::prelude::*;
use vizia_plug::widgets::util::ModifiersExt;
use vizia_plug::widgets::*;
use vizia_plug::{ViziaState, ViziaTheming, create_vizia_editor};

use crate::engine::EngineEvent;
use crate::gui::i18n::{I18n, Language, setup_vizia_fonts};
use crate::gui::keyboard::PianoKeyboardWidget;
use crate::gui::lid::PianoLidWidget;
use crate::gui::mics::MicStageWidget;
use crate::gui::scope::{LissajousScopeWidget, StereoVuMeterWidget};
use crate::nice_plugin::PhysicsPianoParams;
use physics_presets::{ParamTransition, Preset, PresetManager, UndoManager};
use physics_ui::{
    PresetPanelAction, PresetPanelLayout, PresetPanelSignals, parameter_slider, preset_choices,
    preset_panel, redraw_custom_view, set_param,
};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};

pub const EDITOR_WIDTH: u32 = 1080;
pub const EDITOR_HEIGHT: u32 = 620;

fn slider<P: Param + 'static>(cx: &mut Context, label: &'static str, param: &P) {
    parameter_slider(cx, label, param, 104.0);
}

struct PianoUiState {
    params: Arc<PhysicsPianoParams>,
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
    gui_tx: Sender<EngineEvent>,
    key_velocities: Arc<parking_lot::RwLock<[f32; 88]>>,
    octave_offset: i8,
    held_qwerty_keys: HashMap<Code, u8>,
    held_nav_keys: std::collections::HashSet<Code>,
}

#[derive(Debug)]
enum PianoUiEvent {
    PreviousPreset,
    NextPreset,
    ToggleLanguage,
    Undo,
    Redo,
    SelectPreset(String),
    SetName(String),
    SaveAs,
    Rename,
    Overwrite,
    Delete,
    RefreshDisplay,
}

fn piano_value(params: &PhysicsPianoParams, id: &str) -> Option<f32> {
    Some(match id {
        "inharm" => params.inharmonicity_scale.value(),
        "hardness" => params.hammer_hardness.value(),
        "detune" => params.unison_detuning.value(),
        "phantom" => params.phantom_gain.value(),
        "keynoise" => params.key_noise.value(),
        "dampernoise" => params.damper_noise.value(),
        "pedalnoise" => params.pedal_noise.value(),
        "mic_close" => params.mic_close.value(),
        "mic_player" => params.mic_player.value(),
        "mic_ambient" => params.mic_ambient.value(),
        "lid_angle" => params.lid_angle.value(),
        "velocity_curve" => params.velocity_curve.value(),
        "sustain" => params.sustain_pedal.value(),
        _ => return None,
    })
}

fn set_piano_value(cx: &mut EventContext, params: &PhysicsPianoParams, id: &str, value: f32) {
    match id {
        "inharm" => set_param(cx, &params.inharmonicity_scale, value),
        "hardness" => set_param(cx, &params.hammer_hardness, value),
        "detune" => set_param(cx, &params.unison_detuning, value),
        "phantom" => set_param(cx, &params.phantom_gain, value),
        "keynoise" => set_param(cx, &params.key_noise, value),
        "dampernoise" => set_param(cx, &params.damper_noise, value),
        "pedalnoise" => set_param(cx, &params.pedal_noise, value),
        "mic_close" => set_param(cx, &params.mic_close, value),
        "mic_player" => set_param(cx, &params.mic_player, value),
        "mic_ambient" => set_param(cx, &params.mic_ambient, value),
        "lid_angle" => set_param(cx, &params.lid_angle, value),
        "velocity_curve" => set_param(cx, &params.velocity_curve, value),
        "sustain" => set_param(cx, &params.sustain_pedal, value),
        _ => {}
    }
}

fn piano_snapshot(params: &PhysicsPianoParams) -> HashMap<String, f32> {
    [
        "inharm",
        "hardness",
        "detune",
        "phantom",
        "keynoise",
        "dampernoise",
        "pedalnoise",
        "mic_close",
        "mic_player",
        "mic_ambient",
        "lid_angle",
        "velocity_curve",
        "sustain",
    ]
    .into_iter()
    .filter_map(|id| piano_value(params, id).map(|value| (id.to_string(), value)))
    .collect()
}

impl PianoUiState {
    fn play_note(&self, key: u8, velocity: f64) {
        if (21..=108).contains(&key) {
            self.key_velocities.write()[(key - 21) as usize] = velocity as f32;
        }
        let _ = self.gui_tx.send(EngineEvent::NoteOn {
            time: 0,
            key,
            velocity,
        });
    }

    fn release_note(&self, key: u8) {
        if (21..=108).contains(&key) {
            // Suppress the stale audio-active bit until the debounced NoteOff reaches the engine.
            self.key_velocities.write()[(key - 21) as usize] = -1.0;
        }
        let _ = self.gui_tx.send(EngineEvent::NoteOff { time: 0, key });
    }

    fn set_undo_state(&self) {
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
                set_piano_value(cx, &self.params, &id, value);
            }
        }
        self.set_undo_state();
    }
}

impl Model for PianoUiState {
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|action: &PresetPanelAction, _| match action {
            PresetPanelAction::Previous => cx.emit(PianoUiEvent::PreviousPreset),
            PresetPanelAction::Next => cx.emit(PianoUiEvent::NextPreset),
            PresetPanelAction::ToggleLanguage => cx.emit(PianoUiEvent::ToggleLanguage),
            PresetPanelAction::Select(id) => cx.emit(PianoUiEvent::SelectPreset(id.clone())),
            PresetPanelAction::SetName(name) => cx.emit(PianoUiEvent::SetName(name.clone())),
            PresetPanelAction::SaveAs => cx.emit(PianoUiEvent::SaveAs),
            PresetPanelAction::Rename => cx.emit(PianoUiEvent::Rename),
            PresetPanelAction::Overwrite => cx.emit(PianoUiEvent::Overwrite),
            PresetPanelAction::Delete => cx.emit(PianoUiEvent::Delete),
            PresetPanelAction::Undo => cx.emit(PianoUiEvent::Undo),
            PresetPanelAction::Redo => cx.emit(PianoUiEvent::Redo),
        });
        event.map(|event, _| match event {
            PianoUiEvent::RefreshDisplay => {
                redraw_custom_view(cx, "piano-keyboard-widget");
                redraw_custom_view(cx, "lissajous-scope-widget");
                redraw_custom_view(cx, "stereo-vu-meter-widget");
            }
            PianoUiEvent::PreviousPreset | PianoUiEvent::NextPreset => {
                let next = matches!(event, PianoUiEvent::NextPreset);
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
                cx.emit(PianoUiEvent::SelectPreset(id));
            }
            PianoUiEvent::SelectPreset(id) => {
                let preset = self.manager.read().get_preset(id).cloned();
                if let Some(preset) = preset {
                    let mut changes = Vec::new();
                    for (pid, value) in &preset.params {
                        if let Some(old) = piano_value(&self.params, pid) {
                            if (old - value).abs() > 1e-5 {
                                changes.push(ParamTransition {
                                    param_id: pid.clone(),
                                    old_value: old,
                                    new_value: *value,
                                });
                                self.suppress_undo.fetch_add(1, Ordering::Relaxed);
                                set_piano_value(cx, &self.params, pid, *value);
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
                    self.set_undo_state();
                }
            }
            PianoUiEvent::ToggleLanguage => {
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
                if let Some(preset) = self.manager.read().get_preset(&self.selected_id.get()) {
                    self.selected_name.set(
                        preset
                            .display_name(lang == Language::SimplifiedChinese)
                            .to_string(),
                    );
                    self.name_input.set(
                        preset
                            .display_name(lang == Language::SimplifiedChinese)
                            .to_string(),
                    );
                }
                self.preset_choices
                    .set(preset_choices(&self.manager.read(), lang));
            }
            PianoUiEvent::Undo => self.apply_history(cx, false),
            PianoUiEvent::Redo => self.apply_history(cx, true),
            PianoUiEvent::SetName(name) => self.name_input.set(name.clone()),
            PianoUiEvent::SaveAs => {
                let name = self.name_input.get().trim().to_string();
                let name = if name.is_empty() {
                    "Custom Piano".to_string()
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
                    "piano",
                    piano_snapshot(&self.params),
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
            PianoUiEvent::Rename => {
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
            PianoUiEvent::Overwrite => {
                if self.is_user_preset.get() {
                    let _ = self.manager.write().overwrite_user_preset(
                        &self.selected_id.get(),
                        piano_snapshot(&self.params),
                    );
                }
            }
            PianoUiEvent::Delete => {
                let id = self.selected_id.get();
                if self
                    .manager
                    .write()
                    .delete_user_preset(&id)
                    .unwrap_or(false)
                {
                    self.preset_choices
                        .set(preset_choices(&self.manager.read(), self.language.get()));
                    cx.emit(PianoUiEvent::SelectPreset("steinway_concert_d".to_string()));
                }
            }
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
            let (id, value) = if ptr == params.inharmonicity_scale.as_ptr() {
                ("inharm", params.inharmonicity_scale.value())
            } else if ptr == params.hammer_hardness.as_ptr() {
                ("hardness", params.hammer_hardness.value())
            } else if ptr == params.unison_detuning.as_ptr() {
                ("detune", params.unison_detuning.value())
            } else if ptr == params.phantom_gain.as_ptr() {
                ("phantom", params.phantom_gain.value())
            } else if ptr == params.key_noise.as_ptr() {
                ("keynoise", params.key_noise.value())
            } else if ptr == params.damper_noise.as_ptr() {
                ("dampernoise", params.damper_noise.value())
            } else if ptr == params.pedal_noise.as_ptr() {
                ("pedalnoise", params.pedal_noise.value())
            } else if ptr == params.mic_close.as_ptr() {
                ("mic_close", params.mic_close.value())
            } else if ptr == params.mic_player.as_ptr() {
                ("mic_player", params.mic_player.value())
            } else if ptr == params.mic_ambient.as_ptr() {
                ("mic_ambient", params.mic_ambient.value())
            } else if ptr == params.lid_angle.as_ptr() {
                ("lid_angle", params.lid_angle.value())
            } else if ptr == params.velocity_curve.as_ptr() {
                ("velocity_curve", params.velocity_curve.value())
            } else if ptr == params.sustain_pedal.as_ptr() {
                ("sustain", params.sustain_pedal.value())
            } else {
                return;
            };
            if begin {
                self.undo.write().begin_gesture(id, value);
            } else if self.undo.write().end_gesture(id, value) {
                self.set_undo_state();
            }
        });

        event.map(|window: &WindowEvent, meta| match window {
            WindowEvent::KeyDown(code, _) => {
                if cx.modifiers().command() {
                    if *code == Code::KeyZ && !cx.modifiers().shift() {
                        cx.emit(PianoUiEvent::Undo);
                    } else if *code == Code::KeyY || (*code == Code::KeyZ && cx.modifiers().shift())
                    {
                        cx.emit(PianoUiEvent::Redo);
                    }
                    meta.consume();
                    return;
                }
                if *code == Code::KeyZ || *code == Code::KeyX {
                    if self.held_nav_keys.insert(*code) {
                        if *code == Code::KeyZ && self.octave_offset > -24 {
                            self.octave_offset -= 12;
                        }
                        if *code == Code::KeyX && self.octave_offset < 24 {
                            self.octave_offset += 12;
                        }
                    }
                    meta.consume();
                    return;
                }
                const KEY_MAP: &[(Code, u8)] = &[
                    (Code::KeyA, 60),
                    (Code::KeyW, 61),
                    (Code::KeyS, 62),
                    (Code::KeyE, 63),
                    (Code::KeyD, 64),
                    (Code::KeyF, 65),
                    (Code::KeyT, 66),
                    (Code::KeyG, 67),
                    (Code::KeyY, 68),
                    (Code::KeyH, 69),
                    (Code::KeyU, 70),
                    (Code::KeyJ, 71),
                    (Code::KeyK, 72),
                    (Code::KeyO, 73),
                    (Code::KeyL, 74),
                    (Code::KeyP, 75),
                    (Code::Semicolon, 76),
                    (Code::Quote, 77),
                ];
                if let Some((_, base)) = KEY_MAP.iter().find(|(key, _)| key == code) {
                    if !self.held_qwerty_keys.contains_key(code) {
                        let midi = (*base as i16 + self.octave_offset as i16).clamp(21, 108) as u8;
                        self.held_qwerty_keys.insert(*code, midi);
                        self.play_note(midi, 0.85);
                        meta.consume();
                    }
                }
            }
            WindowEvent::KeyUp(code, _) => {
                self.held_nav_keys.remove(code);
                if let Some(midi) = self.held_qwerty_keys.remove(code) {
                    self.release_note(midi);
                    meta.consume();
                }
            }
            WindowEvent::FocusOut => {
                self.held_nav_keys.clear();
                let released: Vec<u8> = self
                    .held_qwerty_keys
                    .drain()
                    .map(|(_, midi)| midi)
                    .collect();
                for midi in released {
                    self.release_note(midi);
                }
            }
            _ => {}
        });
    }
}

pub fn default_vizia_state() -> Arc<ViziaState> {
    ViziaState::new(|| (EDITOR_WIDTH, EDITOR_HEIGHT))
}

#[allow(clippy::too_many_arguments)]
pub fn create_vizia_piano_editor(
    params: Arc<PhysicsPianoParams>,
    peak_l: Arc<AtomicF32>,
    peak_r: Arc<AtomicF32>,
    active_keys_low: Arc<AtomicU64>,
    active_keys_high: Arc<AtomicU64>,
    key_velocities: Arc<parking_lot::RwLock<[f32; 88]>>,
    recent_orbit_t: Arc<parking_lot::RwLock<Vec<f32>>>,
    recent_orbit_p: Arc<parking_lot::RwLock<Vec<f32>>>,
    language_atom: Arc<AtomicU8>,
    gui_tx: Sender<EngineEvent>,
    preset_manager: Arc<parking_lot::RwLock<PresetManager>>,
    undo_manager: Arc<parking_lot::RwLock<UndoManager>>,
    editor_state: Arc<ViziaState>,
) -> Option<Box<dyn Editor>> {
    let lang = if language_atom.load(std::sync::atomic::Ordering::Relaxed) == 1 {
        Language::SimplifiedChinese
    } else {
        Language::English
    };
    let selected_name = preset_manager
        .read()
        .get_preset("steinway_concert_d")
        .map(|preset| {
            preset
                .display_name(lang == Language::SimplifiedChinese)
                .to_string()
        })
        .unwrap_or_else(|| "Steinway Concert D".to_string());
    let manager = preset_manager;
    let language_initial = lang;
    let initial_id = "steinway_concert_d".to_string();

    create_vizia_editor(editor_state, ViziaTheming::Custom, move |cx, _| {
        // Signals need to belong to this Vizia tree's reactive scope.
        let preset_name = Signal::new(selected_name.clone());
        let preset_id = Signal::new(initial_id.clone());
        let preset_choices = Signal::new(preset_choices(&manager.read(), language_initial));
        let language = Signal::new(language_initial);
        let language_view = language.clone();
        let language_ui = language.clone();
        let name_input = Signal::new(selected_name.clone());
        let is_user_preset = Signal::new(false);
        let can_undo = Signal::new(undo_manager.read().can_undo());
        let can_redo = Signal::new(undo_manager.read().can_redo());
        let is_user_preset_ui = is_user_preset.clone();
        let can_undo_ui = can_undo.clone();
        let can_redo_ui = can_redo.clone();
        let suppress_undo = Arc::new(AtomicU32::new(0));
        setup_vizia_fonts(cx);
        if let Err(err) = cx.add_stylesheet(include_style!("src/gui/theme.css")) {
            eprintln!("Failed to load Vizia stylesheet: {err:?}");
        }
        PianoUiState {
            params: params.clone(),
            manager: manager.clone(),
            undo: undo_manager.clone(),
            selected_name: preset_name.clone(),
            selected_id: preset_id,
            preset_choices: preset_choices.clone(),
            language,
            language_atom: language_atom.clone(),
            name_input: name_input.clone(),
            is_user_preset,
            can_undo: can_undo.clone(),
            can_redo: can_redo.clone(),
            suppress_undo,
            gui_tx: gui_tx.clone(),
            key_velocities: key_velocities.clone(),
            octave_offset: 0,
            held_qwerty_keys: HashMap::new(),
            held_nav_keys: std::collections::HashSet::new(),
        }
        .build(cx);
        let display_timer = cx.add_timer(std::time::Duration::from_millis(33), None, |cx, _| {
            cx.emit(PianoUiEvent::RefreshDisplay)
        });
        cx.start_timer(display_timer);

        let params_ui = params.clone();
        let peak_l_ui = peak_l.clone();
        let peak_r_ui = peak_r.clone();
        let active_keys_low_ui = active_keys_low.clone();
        let active_keys_high_ui = active_keys_high.clone();
        let key_velocities_ui = key_velocities.clone();
        let recent_orbit_t_ui = recent_orbit_t.clone();
        let recent_orbit_p_ui = recent_orbit_p.clone();
        let gui_tx_ui = gui_tx.clone();
        let name_input_ui = name_input.clone();
        let preset_name_ui = preset_name.clone();
        let preset_choices_ui = preset_choices.clone();
        let language_view_ui = language_view.clone();
        let is_user_preset_controls = is_user_preset_ui.clone();
        let can_undo_controls = can_undo_ui.clone();
        let can_redo_controls = can_redo_ui.clone();

        Binding::new(cx, language_ui, move |cx| {
            let lang = language_ui.get();
            let params = params_ui.clone();
            let peak_l = peak_l_ui.clone();
            let peak_r = peak_r_ui.clone();
            let active_keys_low = active_keys_low_ui.clone();
            let active_keys_high = active_keys_high_ui.clone();
            let key_velocities = key_velocities_ui.clone();
            let recent_orbit_t = recent_orbit_t_ui.clone();
            let recent_orbit_p = recent_orbit_p_ui.clone();
            let gui_tx = gui_tx_ui.clone();
            let initial_preset_name = preset_name_ui.clone();
            let preset_choices = preset_choices_ui.clone();
            let name_input = name_input_ui.clone();
            let language_view = language_view_ui.clone();
            let is_user_preset = is_user_preset_controls.clone();
            let can_undo = can_undo_controls.clone();
            let can_redo = can_redo_controls.clone();
            VStack::new(cx, move |cx| {
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
                        preset_label: None,
                        preset_width: 190.0,
                        name_width: 120.0,
                        save_as_width: 60.0,
                        rename_width: 54.0,
                        overwrite_width: 64.0,
                        delete_width: 44.0,
                    },
                    move |cx| {
                        HStack::new(cx, |cx| {
                            LissajousScopeWidget::new(
                                cx,
                                recent_orbit_t.clone(),
                                recent_orbit_p.clone(),
                            )
                            .size(Pixels(32.0));
                            StereoVuMeterWidget::new(cx, peak_l.clone(), peak_r.clone())
                                .width(Pixels(70.0))
                                .height(Pixels(24.0));
                        })
                        .horizontal_gap(Pixels(8.0));
                    },
                );

                HStack::new(cx, |cx| {
                    PianoLidWidget::new(cx, params.clone(), lang)
                        .width(Stretch(1.0))
                        .height(Pixels(150.0))
                        .overflow(Overflow::Hidden);
                    MicStageWidget::new(cx, params.clone(), lang)
                        .width(Stretch(1.0))
                        .height(Pixels(150.0));
                })
                .height(Pixels(154.0))
                .horizontal_gap(Pixels(8.0));

                HStack::new(cx, |cx| {
                    VStack::new(cx, |cx| {
                        Label::new(cx, I18n::rack_pedals(lang)).class("rack-title");
                        slider(cx, I18n::sustain(lang), &params.sustain_pedal);
                        slider(cx, I18n::key_action(lang), &params.key_noise);
                        slider(cx, I18n::damper_noise(lang), &params.damper_noise);
                        slider(cx, I18n::pedal_shock(lang), &params.pedal_noise);
                    })
                    .class("rack-box")
                    .width(Stretch(1.0));
                    VStack::new(cx, |cx| {
                        Label::new(cx, I18n::rack_string(lang)).class("rack-title");
                        slider(cx, I18n::inharmonicity(lang), &params.inharmonicity_scale);
                        slider(cx, I18n::hammer_hardness(lang), &params.hammer_hardness);
                        slider(cx, I18n::unison_detune(lang), &params.unison_detuning);
                        slider(cx, I18n::phantom_partials(lang), &params.phantom_gain);
                    })
                    .class("rack-box")
                    .width(Stretch(1.0));
                    VStack::new(cx, |cx| {
                        Label::new(cx, I18n::rack_spatial(lang)).class("rack-title");
                        slider(cx, I18n::mic_close(lang), &params.mic_close);
                        slider(cx, I18n::mic_player(lang), &params.mic_player);
                        slider(cx, I18n::mic_ambient(lang), &params.mic_ambient);
                        slider(cx, I18n::lid_angle(lang), &params.lid_angle);
                    })
                    .class("rack-box")
                    .width(Stretch(1.0));
                    VStack::new(cx, |cx| {
                        Label::new(cx, I18n::rack_output(lang)).class("rack-title");
                        slider(cx, I18n::master_gain(lang), &params.master_gain);
                        slider(cx, I18n::velocity_curve(lang), &params.velocity_curve);
                        ParamButton::new(cx, &params.una_corda).with_label(I18n::una_corda(lang));
                    })
                    .class("rack-box")
                    .width(Stretch(1.0));
                })
                .height(Pixels(170.0))
                .horizontal_gap(Pixels(8.0));

                PianoKeyboardWidget::new(
                    cx,
                    gui_tx.clone(),
                    active_keys_low.clone(),
                    active_keys_high.clone(),
                    key_velocities.clone(),
                )
                .focusable(true)
                .height(Pixels(130.0))
                .width(Stretch(1.0));
                Label::new(cx, I18n::keyboard_hint(lang)).class("hint-text");
            });
        });
    })
}
