//! Complete Vizia GUI Editor for Physics Piano.
//!
//! Replaces the old egui immediate-mode layout with a high-performance,
//! retained-mode vector UI adhering to professional virtual instrument standards.
//! Integrates shared JSON preset persistence and granular Undo/Redo gesture tracking.

use atomic_float::AtomicF32;
use crossbeam_channel::Sender;
use nih_plug::prelude::{Editor, Param, ParamPtr};
use nih_plug_vizia::vizia::prelude::*;
use nih_plug_vizia::widgets::util::ModifiersExt;
use nih_plug_vizia::widgets::*;
use nih_plug_vizia::{create_vizia_editor, ViziaState, ViziaTheming};
use physics_presets::{ParamTransition, Preset, PresetManager, UndoManager};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;

use crate::engine::EngineEvent;
use crate::gui::i18n::{setup_vizia_fonts, I18n, Language};
use crate::gui::keyboard::PianoKeyboardWidget;
use crate::gui::lid::PianoLidWidget;
use crate::gui::mics::MicStageWidget;
use crate::gui::scope::{LissajousScopeWidget, StereoVuMeterWidget};
use crate::nih_plugin::PhysicsPianoParams;

pub const EDITOR_WIDTH: u32 = 1080;
pub const EDITOR_HEIGHT: u32 = 620;

fn apply_param<P: Param>(cx: &mut EventContext, param: &P, val: P::Plain) {
    cx.emit(ParamEvent::BeginSetParameter(param).upcast());
    cx.emit(ParamEvent::SetParameter(param, val).upcast());
    cx.emit(ParamEvent::EndSetParameter(param).upcast());
}

fn get_piano_param_value(params: &PhysicsPianoParams, param_id: &str) -> Option<f32> {
    match param_id {
        "inharm" => Some(params.inharmonicity_scale.value()),
        "hardness" => Some(params.hammer_hardness.value()),
        "detune" => Some(params.unison_detuning.value()),
        "phantom" => Some(params.phantom_gain.value()),
        "keynoise" => Some(params.key_noise.value()),
        "dampernoise" => Some(params.damper_noise.value()),
        "pedalnoise" => Some(params.pedal_noise.value()),
        "mic_close" => Some(params.mic_close.value()),
        "mic_player" => Some(params.mic_player.value()),
        "mic_ambient" => Some(params.mic_ambient.value()),
        "lid_angle" => Some(params.lid_angle.value()),
        "velocity_curve" => Some(params.velocity_curve.value()),
        "sustain" => Some(params.sustain_pedal.value()),
        _ => None,
    }
}

fn set_piano_param_value(cx: &mut EventContext, params: &PhysicsPianoParams, param_id: &str, val: f32) {
    match param_id {
        "inharm" => apply_param(cx, &params.inharmonicity_scale, val),
        "hardness" => apply_param(cx, &params.hammer_hardness, val),
        "detune" => apply_param(cx, &params.unison_detuning, val),
        "phantom" => apply_param(cx, &params.phantom_gain, val),
        "keynoise" => apply_param(cx, &params.key_noise, val),
        "dampernoise" => apply_param(cx, &params.damper_noise, val),
        "pedalnoise" => apply_param(cx, &params.pedal_noise, val),
        "mic_close" => apply_param(cx, &params.mic_close, val),
        "mic_player" => apply_param(cx, &params.mic_player, val),
        "mic_ambient" => apply_param(cx, &params.mic_ambient, val),
        "lid_angle" => apply_param(cx, &params.lid_angle, val),
        "velocity_curve" => apply_param(cx, &params.velocity_curve, val),
        "sustain" => apply_param(cx, &params.sustain_pedal, val),
        _ => {}
    }
}

pub fn snapshot_piano_params(params: &PhysicsPianoParams) -> HashMap<String, f32> {
    let mut map = HashMap::new();
    map.insert("inharm".to_string(), params.inharmonicity_scale.value());
    map.insert("hardness".to_string(), params.hammer_hardness.value());
    map.insert("detune".to_string(), params.unison_detuning.value());
    map.insert("phantom".to_string(), params.phantom_gain.value());
    map.insert("keynoise".to_string(), params.key_noise.value());
    map.insert("dampernoise".to_string(), params.damper_noise.value());
    map.insert("pedalnoise".to_string(), params.pedal_noise.value());
    map.insert("mic_close".to_string(), params.mic_close.value());
    map.insert("mic_player".to_string(), params.mic_player.value());
    map.insert("mic_ambient".to_string(), params.mic_ambient.value());
    map.insert("lid_angle".to_string(), params.lid_angle.value());
    map.insert("velocity_curve".to_string(), params.velocity_curve.value());
    map
}

#[derive(Lens, Clone)]
pub struct PianoViziaData {
    pub params: Arc<PhysicsPianoParams>,
    pub peak_l: Arc<AtomicF32>,
    pub peak_r: Arc<AtomicF32>,
    pub active_keys_low: Arc<AtomicU64>,
    pub active_keys_high: Arc<AtomicU64>,
    pub key_velocities: Arc<parking_lot::RwLock<[f32; 88]>>,
    pub recent_orbit_t: Arc<parking_lot::RwLock<Vec<f32>>>,
    pub recent_orbit_p: Arc<parking_lot::RwLock<Vec<f32>>>,
    pub language_atom: Arc<AtomicU8>,
    pub language: Language,
    pub selected_preset_id: Option<String>,
    pub selected_preset_name: String,
    pub preset_manager: Arc<parking_lot::RwLock<PresetManager>>,
    pub undo_manager: Arc<parking_lot::RwLock<UndoManager>>,
    pub can_undo: bool,
    pub can_redo: bool,
    pub gui_tx: Sender<EngineEvent>,
    pub octave_offset: i8,
    pub held_qwerty_keys: HashMap<Code, u8>,
    pub suppress_undo_gestures: Arc<AtomicU32>,
}

impl PianoViziaData {
    pub fn play_note(&mut self, key: u8, velocity: f32) {
        if (21..=108).contains(&key) {
            let idx = (key - 21) as usize;
            if let Some(mut vels) = self.key_velocities.try_write() {
                vels[idx] = velocity;
            }
        }
        let _ = self.gui_tx.send(EngineEvent::NoteOn {
            time: 0,
            key,
            velocity: velocity as f64,
        });
    }

    pub fn release_note(&mut self, key: u8) {
        let _ = self.gui_tx.send(EngineEvent::NoteOff {
            time: 0,
            key,
        });
    }

    fn resolve_param_ptr(&self, ptr: ParamPtr) -> (&'static str, f32) {
        let pr = &self.params;
        if ptr == pr.inharmonicity_scale.as_ptr() {
            ("inharm", pr.inharmonicity_scale.value())
        } else if ptr == pr.hammer_hardness.as_ptr() {
            ("hardness", pr.hammer_hardness.value())
        } else if ptr == pr.unison_detuning.as_ptr() {
            ("detune", pr.unison_detuning.value())
        } else if ptr == pr.phantom_gain.as_ptr() {
            ("phantom", pr.phantom_gain.value())
        } else if ptr == pr.key_noise.as_ptr() {
            ("keynoise", pr.key_noise.value())
        } else if ptr == pr.damper_noise.as_ptr() {
            ("dampernoise", pr.damper_noise.value())
        } else if ptr == pr.pedal_noise.as_ptr() {
            ("pedalnoise", pr.pedal_noise.value())
        } else if ptr == pr.mic_close.as_ptr() {
            ("mic_close", pr.mic_close.value())
        } else if ptr == pr.mic_player.as_ptr() {
            ("mic_player", pr.mic_player.value())
        } else if ptr == pr.mic_ambient.as_ptr() {
            ("mic_ambient", pr.mic_ambient.value())
        } else if ptr == pr.lid_angle.as_ptr() {
            ("lid_angle", pr.lid_angle.value())
        } else if ptr == pr.velocity_curve.as_ptr() {
            ("velocity_curve", pr.velocity_curve.value())
        } else if ptr == pr.sustain_pedal.as_ptr() {
            ("sustain", pr.sustain_pedal.value())
        } else {
            ("", 0.0)
        }
    }
}

#[derive(Debug, Clone)]
pub enum PianoUiEvent {
    ToggleLanguage,
    SelectPreset(String),
    CyclePreset(bool),
    PromptSavePreset,
    SavePreset(String),
    DeletePreset(String),
    Undo,
    Redo,
}

impl Model for PianoViziaData {
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|app_event, _| match app_event {
            PianoUiEvent::ToggleLanguage => {
                self.language = match self.language {
                    Language::English => Language::SimplifiedChinese,
                    Language::SimplifiedChinese => Language::English,
                };
                self.language_atom.store(
                    if self.language == Language::SimplifiedChinese { 1 } else { 0 },
                    Ordering::Relaxed,
                );
            }
            PianoUiEvent::SelectPreset(preset_id) => {
                let preset_opt = {
                    let mgr = self.preset_manager.read();
                    mgr.get_preset(&preset_id).cloned()
                };
                if let Some(preset) = preset_opt {
                    let mut transitions = Vec::new();
                    for (pid, &new_val) in &preset.params {
                        if let Some(old_val) = get_piano_param_value(&self.params, pid) {
                            if (new_val - old_val).abs() > 1e-4 {
                                transitions.push(ParamTransition {
                                    param_id: pid.clone(),
                                    old_value: old_val,
                                    new_value: new_val,
                                });
                            }
                            self.suppress_undo_gestures.fetch_add(1, Ordering::Relaxed);
                            set_piano_param_value(cx, &self.params, pid, new_val);
                        }
                    }
                    if !transitions.is_empty() {
                        self.undo_manager.write().record_batch(&preset.name, transitions);
                    }
                    self.selected_preset_id = Some(preset_id.clone());
                    self.selected_preset_name = preset.name.clone();
                    let (u, r) = {
                        let mgr = self.undo_manager.read();
                        (mgr.can_undo(), mgr.can_redo())
                    };
                    self.can_undo = u;
                    self.can_redo = r;
                }
            }
            PianoUiEvent::CyclePreset(next) => {
                let next_id_opt = {
                    let mgr = self.preset_manager.read();
                    let presets = mgr.presets();
                    if presets.is_empty() {
                        None
                    } else {
                        let curr_idx = self
                            .selected_preset_id
                            .as_ref()
                            .and_then(|id| presets.iter().position(|p| &p.id == id))
                            .unwrap_or(0);
                        let new_idx = if *next {
                            (curr_idx + 1) % presets.len()
                        } else {
                            (curr_idx + presets.len() - 1) % presets.len()
                        };
                        Some(presets[new_idx].id.clone())
                    }
                };
                if let Some(next_id) = next_id_opt {
                    cx.emit(PianoUiEvent::SelectPreset(next_id));
                }
            }
            PianoUiEvent::PromptSavePreset => {
                let preset_count = self.preset_manager.read().presets().len();
                let new_id = format!("user_piano_preset_{}", preset_count + 1);
                let new_name = format!("User Piano {}", preset_count + 1);
                let params_map = snapshot_piano_params(&self.params);
                let preset = Preset::new(new_id.clone(), new_name.clone(), "piano", params_map);
                let mut mgr = self.preset_manager.write();
                if let Ok(()) = mgr.save_user_preset(preset) {
                    self.selected_preset_id = Some(new_id);
                    self.selected_preset_name = new_name;
                }
            }
            PianoUiEvent::SavePreset(name) => {
                let safe_id = name.to_lowercase().replace(' ', "_");
                let params_map = snapshot_piano_params(&self.params);
                let preset = Preset::new(safe_id.clone(), name.clone(), "piano", params_map);
                let mut mgr = self.preset_manager.write();
                if let Ok(()) = mgr.save_user_preset(preset) {
                    self.selected_preset_id = Some(safe_id);
                    self.selected_preset_name = name.clone();
                }
            }
            PianoUiEvent::DeletePreset(preset_id) => {
                let mut mgr = self.preset_manager.write();
                let _ = mgr.delete_user_preset(&preset_id);
                if self.selected_preset_id.as_deref() == Some(&preset_id) {
                    self.selected_preset_id = None;
                    self.selected_preset_name = "Steinway Concert D".to_string();
                }
            }
            PianoUiEvent::Undo => {
                let restores = {
                    let mut mgr = self.undo_manager.write();
                    mgr.undo()
                };
                if let Some(restores) = restores {
                    for (pid, val) in restores {
                        self.suppress_undo_gestures.fetch_add(1, Ordering::Relaxed);
                        set_piano_param_value(cx, &self.params, &pid, val);
                    }
                    let (u, r) = {
                        let mgr = self.undo_manager.read();
                        (mgr.can_undo(), mgr.can_redo())
                    };
                    self.can_undo = u;
                    self.can_redo = r;
                }
            }
            PianoUiEvent::Redo => {
                let restores = {
                    let mut mgr = self.undo_manager.write();
                    mgr.redo()
                };
                if let Some(restores) = restores {
                    for (pid, val) in restores {
                        self.suppress_undo_gestures.fetch_add(1, Ordering::Relaxed);
                        set_piano_param_value(cx, &self.params, &pid, val);
                    }
                    let (u, r) = {
                        let mgr = self.undo_manager.read();
                        (mgr.can_undo(), mgr.can_redo())
                    };
                    self.can_undo = u;
                    self.can_redo = r;
                }
            }
        });

        // Intercept parameter slider gestures for single-parameter undo/redo
        event.map(|raw_param: &RawParamEvent, _| {
            match raw_param {
                RawParamEvent::BeginSetParameter(ptr) => {
                    if self.suppress_undo_gestures.load(Ordering::Relaxed) > 0 {
                        self.suppress_undo_gestures.fetch_sub(1, Ordering::Relaxed);
                        return;
                    }
                    let (pid, val) = self.resolve_param_ptr(*ptr);
                    if !pid.is_empty() {
                        self.undo_manager.write().begin_gesture(pid, val);
                    }
                }
                RawParamEvent::EndSetParameter(ptr) => {
                    let (pid, val) = self.resolve_param_ptr(*ptr);
                    if !pid.is_empty() {
                        if self.undo_manager.write().end_gesture(pid, val) {
                            let (u, r) = {
                                let mgr = self.undo_manager.read();
                                (mgr.can_undo(), mgr.can_redo())
                            };
                            self.can_undo = u;
                            self.can_redo = r;
                        }
                    }
                }
                _ => {}
            }
        });

        // Keyboard shortcuts & QWERTY piano playing
        event.map(|window_event: &WindowEvent, meta| {
            match window_event {
                WindowEvent::KeyDown(code, _) => {
                    if cx.modifiers().command() {
                        if *code == Code::KeyZ && !cx.modifiers().shift() {
                            cx.emit(PianoUiEvent::Undo);
                            meta.consume();
                        } else if *code == Code::KeyY || (*code == Code::KeyZ && cx.modifiers().shift()) {
                            cx.emit(PianoUiEvent::Redo);
                            meta.consume();
                        }
                        return;
                    }

                    // Octave Shift: Z = Octave Down (-12), X = Octave Up (+12)
                    if *code == Code::KeyZ {
                        if self.octave_offset > -24 {
                            self.octave_offset -= 12;
                        }
                        meta.consume();
                        return;
                    }
                    if *code == Code::KeyX {
                        if self.octave_offset < 24 {
                            self.octave_offset += 12;
                        }
                        meta.consume();
                        return;
                    }

                    const QWERTY_MAP: &[(Code, u8)] = &[
                        (Code::KeyA, 60),      // C4
                        (Code::KeyW, 61),      // C#4
                        (Code::KeyS, 62),      // D4
                        (Code::KeyE, 63),      // D#4
                        (Code::KeyD, 64),      // E4
                        (Code::KeyF, 65),      // F4
                        (Code::KeyT, 66),      // F#4
                        (Code::KeyG, 67),      // G4
                        (Code::KeyY, 68),      // G#4
                        (Code::KeyH, 69),      // A4
                        (Code::KeyU, 70),      // A#4
                        (Code::KeyJ, 71),      // B4
                        (Code::KeyK, 72),      // C5
                        (Code::KeyO, 73),      // C#5
                        (Code::KeyL, 74),      // D5
                        (Code::KeyP, 75),      // D#5
                        (Code::Semicolon, 76), // E5
                        (Code::Quote, 77),     // F5
                    ];

                    for &(c, base_midi) in QWERTY_MAP {
                        if *code == c && !self.held_qwerty_keys.contains_key(code) {
                            let midi = (base_midi as i16 + self.octave_offset as i16).clamp(21, 108) as u8;
                            self.held_qwerty_keys.insert(*code, midi);
                            self.play_note(midi, 0.85);
                            cx.needs_redraw();
                            meta.consume();
                            break;
                        }
                    }
                }
                WindowEvent::KeyUp(code, _) => {
                    if let Some(midi) = self.held_qwerty_keys.remove(code) {
                        self.release_note(midi);
                        cx.needs_redraw();
                        meta.consume();
                    }
                }
                WindowEvent::FocusOut => {
                    let keys: Vec<u8> = self.held_qwerty_keys.drain().map(|(_, k)| k).collect();
                    for k in keys {
                        self.release_note(k);
                    }
                    cx.needs_redraw();
                }
                _ => {}
            }
        });
    }
}

pub fn default_vizia_state() -> Arc<ViziaState> {
    ViziaState::new(|| (EDITOR_WIDTH, EDITOR_HEIGHT))
}

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
    let initial_lang = if language_atom.load(Ordering::Relaxed) == 1 {
        Language::SimplifiedChinese
    } else {
        Language::English
    };

    let initial_preset_name = {
        let mgr = preset_manager.read();
        mgr.get_preset("steinway_concert_d")
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "Steinway Concert D".to_string())
    };

    let data = PianoViziaData {
        params,
        peak_l,
        peak_r,
        active_keys_low,
        active_keys_high,
        key_velocities,
        recent_orbit_t,
        recent_orbit_p,
        language_atom,
        language: initial_lang,
        selected_preset_id: Some("steinway_concert_d".to_string()),
        selected_preset_name: initial_preset_name,
        preset_manager,
        undo_manager,
        can_undo: false,
        can_redo: false,
        gui_tx,
        octave_offset: 0,
        held_qwerty_keys: HashMap::new(),
        suppress_undo_gestures: Arc::new(AtomicU32::new(0)),
    };

    create_vizia_editor(editor_state, ViziaTheming::Custom, move |cx, _gui_cx| {
        setup_vizia_fonts(cx);

        if let Err(err) = cx.add_stylesheet(include_style!("src/gui/theme.css")) {
            nih_plug::nih_error!("Failed to load Vizia stylesheet: {err:?}");
        }

        data.clone().build(cx);

        let lang = data.language;

        VStack::new(cx, |cx| {
            // 1. Header Bar: Title, Compact Preset Selector, Undo/Redo, Language, Meter
            HStack::new(cx, |cx| {
                VStack::new(cx, |cx| {
                    Label::new(cx, I18n::title(lang)).class("title");
                    Label::new(cx, I18n::subtitle(lang)).class("subtitle");
                })
                .width(Pixels(170.0));

                // Compact Preset Navigator: ◀ | Preset ▾ | ▶ | Save
                HStack::new(cx, |cx| {
                    Label::new(cx, I18n::preset(lang))
                        .class("param-label")
                        .top(Stretch(1.0))
                        .bottom(Stretch(1.0));

                    Button::new(
                        cx,
                        |cx| cx.emit(PianoUiEvent::CyclePreset(false)),
                        |cx| Label::new(cx, "◀"),
                    )
                    .class("btn-cycle")
                    .width(Pixels(26.0))
                    .height(Pixels(24.0));

                    Dropdown::new(
                        cx,
                        |cx| {
                            Label::new(
                                cx,
                                PianoViziaData::selected_preset_name.map(|name| format!("{} ▾", name)),
                            )
                            .class("preset-dropdown-label")
                            .top(Stretch(1.0))
                            .bottom(Stretch(1.0))
                        },
                        |cx| {
                            Binding::new(cx, PianoViziaData::preset_manager, |cx, mgr_lens| {
                                let mgr_arc = mgr_lens.get(cx);
                                let mgr = mgr_arc.read();
                                for preset in mgr.presets() {
                                    let pid = preset.id.clone();
                                    let pname = preset.name.clone();
                                    Label::new(cx, &pname)
                                        .class("preset-item")
                                        .on_press(move |cx| {
                                            cx.emit(PianoUiEvent::SelectPreset(pid.clone()));
                                            cx.emit(PopupEvent::Close);
                                        });
                                }
                            });
                        },
                    )
                    .class("preset-dropdown")
                    .height(Pixels(24.0))
                    .width(Pixels(180.0));

                    Button::new(
                        cx,
                        |cx| cx.emit(PianoUiEvent::CyclePreset(true)),
                        |cx| Label::new(cx, "▶"),
                    )
                    .class("btn-cycle")
                    .width(Pixels(26.0))
                    .height(Pixels(24.0));

                    Button::new(
                        cx,
                        |cx| cx.emit(PianoUiEvent::PromptSavePreset),
                        |cx| {
                            Label::new(
                                cx,
                                match lang {
                                    Language::English => "Save",
                                    Language::SimplifiedChinese => "保存",
                                },
                            )
                        },
                    )
                    .height(Pixels(24.0))
                    .width(Pixels(52.0));
                })
                .col_between(Pixels(4.0))
                .top(Stretch(1.0))
                .bottom(Stretch(1.0));

                Element::new(cx).width(Stretch(1.0));

                // Undo & Redo Buttons (clean text without broken unicode arrows)
                HStack::new(cx, |cx| {
                    Button::new(
                        cx,
                        |cx| cx.emit(PianoUiEvent::Undo),
                        |cx| {
                            Label::new(
                                cx,
                                match lang {
                                    Language::English => "Undo",
                                    Language::SimplifiedChinese => "撤销",
                                },
                            )
                        },
                    )
                    .class("btn-action")
                    .height(Pixels(24.0))
                    .width(Pixels(52.0));

                    Button::new(
                        cx,
                        |cx| cx.emit(PianoUiEvent::Redo),
                        |cx| {
                            Label::new(
                                cx,
                                match lang {
                                    Language::English => "Redo",
                                    Language::SimplifiedChinese => "重做",
                                },
                            )
                        },
                    )
                    .class("btn-action")
                    .height(Pixels(24.0))
                    .width(Pixels(52.0));
                })
                .col_between(Pixels(4.0))
                .top(Stretch(1.0))
                .bottom(Stretch(1.0));

                // Language Switcher Button
                Button::new(
                    cx,
                    |cx| cx.emit(PianoUiEvent::ToggleLanguage),
                    |cx| {
                        Label::new(
                            cx,
                            match lang {
                                Language::English => "🌐 中文",
                                Language::SimplifiedChinese => "🌐 English",
                            },
                        )
                    },
                )
                .height(Pixels(24.0))
                .width(Pixels(74.0))
                .top(Stretch(1.0))
                .bottom(Stretch(1.0));

                // Visualizers (Lissajous & Stereo VU Meter)
                HStack::new(cx, |cx| {
                    LissajousScopeWidget::new(cx, PianoViziaData::recent_orbit_t, PianoViziaData::recent_orbit_p)
                        .size(Pixels(32.0));
                    StereoVuMeterWidget::new(cx, PianoViziaData::peak_l, PianoViziaData::peak_r)
                        .width(Pixels(70.0))
                        .height(Pixels(24.0));
                })
                .col_between(Pixels(8.0))
                .top(Stretch(1.0))
                .bottom(Stretch(1.0));
            })
            .height(Pixels(38.0))
            .child_top(Stretch(1.0))
            .child_bottom(Stretch(1.0))
            .col_between(Pixels(10.0));

            // 2. Center Visual Physical Metaphors Panel (Lid Geometry & Mic Soundstage)
            HStack::new(cx, |cx| {
                PianoLidWidget::new(cx, PianoViziaData::params, lang)
                    .width(Stretch(1.0))
                    .height(Pixels(150.0));

                MicStageWidget::new(cx, PianoViziaData::params, lang)
                    .width(Stretch(1.0))
                    .height(Pixels(150.0));
            })
            .height(Pixels(154.0))
            .col_between(Pixels(10.0));

            // 3. Four Parameter Control Racks with aligned labels and sliders
            HStack::new(cx, |cx| {
                // Rack 1: Pedals & Mechanics
                VStack::new(cx, |cx| {
                    Label::new(cx, I18n::rack_pedals(lang)).class("rack-title");
                    HStack::new(cx, |cx| {
                        Label::new(cx, I18n::sustain(lang)).class("param-label").width(Pixels(86.0)).top(Stretch(1.0)).bottom(Stretch(1.0));
                        ParamSlider::new(cx, PianoViziaData::params, |p| &p.sustain_pedal).height(Pixels(20.0)).width(Stretch(1.0)).top(Stretch(1.0)).bottom(Stretch(1.0));
                    }).height(Pixels(22.0)).col_between(Pixels(6.0));
                    HStack::new(cx, |cx| {
                        Label::new(cx, I18n::key_action(lang)).class("param-label").width(Pixels(86.0)).top(Stretch(1.0)).bottom(Stretch(1.0));
                        ParamSlider::new(cx, PianoViziaData::params, |p| &p.key_noise).height(Pixels(20.0)).width(Stretch(1.0)).top(Stretch(1.0)).bottom(Stretch(1.0));
                    }).height(Pixels(22.0)).col_between(Pixels(6.0));
                    HStack::new(cx, |cx| {
                        Label::new(cx, I18n::damper_noise(lang)).class("param-label").width(Pixels(86.0)).top(Stretch(1.0)).bottom(Stretch(1.0));
                        ParamSlider::new(cx, PianoViziaData::params, |p| &p.damper_noise).height(Pixels(20.0)).width(Stretch(1.0)).top(Stretch(1.0)).bottom(Stretch(1.0));
                    }).height(Pixels(22.0)).col_between(Pixels(6.0));
                    HStack::new(cx, |cx| {
                        Label::new(cx, I18n::pedal_shock(lang)).class("param-label").width(Pixels(86.0)).top(Stretch(1.0)).bottom(Stretch(1.0));
                        ParamSlider::new(cx, PianoViziaData::params, |p| &p.pedal_noise).height(Pixels(20.0)).width(Stretch(1.0)).top(Stretch(1.0)).bottom(Stretch(1.0));
                    }).height(Pixels(22.0)).col_between(Pixels(6.0));
                })
                .class("rack-box")
                .width(Stretch(1.0))
                .height(Pixels(165.0));

                // Rack 2: String & Hammer Physics
                VStack::new(cx, |cx| {
                    Label::new(cx, I18n::rack_string(lang)).class("rack-title");
                    HStack::new(cx, |cx| {
                        Label::new(cx, I18n::inharmonicity(lang)).class("param-label").width(Pixels(86.0)).top(Stretch(1.0)).bottom(Stretch(1.0));
                        ParamSlider::new(cx, PianoViziaData::params, |p| &p.inharmonicity_scale).height(Pixels(20.0)).width(Stretch(1.0)).top(Stretch(1.0)).bottom(Stretch(1.0));
                    }).height(Pixels(22.0)).col_between(Pixels(6.0));
                    HStack::new(cx, |cx| {
                        Label::new(cx, I18n::hammer_hardness(lang)).class("param-label").width(Pixels(86.0)).top(Stretch(1.0)).bottom(Stretch(1.0));
                        ParamSlider::new(cx, PianoViziaData::params, |p| &p.hammer_hardness).height(Pixels(20.0)).width(Stretch(1.0)).top(Stretch(1.0)).bottom(Stretch(1.0));
                    }).height(Pixels(22.0)).col_between(Pixels(6.0));
                    HStack::new(cx, |cx| {
                        Label::new(cx, I18n::unison_detune(lang)).class("param-label").width(Pixels(86.0)).top(Stretch(1.0)).bottom(Stretch(1.0));
                        ParamSlider::new(cx, PianoViziaData::params, |p| &p.unison_detuning).height(Pixels(20.0)).width(Stretch(1.0)).top(Stretch(1.0)).bottom(Stretch(1.0));
                    }).height(Pixels(22.0)).col_between(Pixels(6.0));
                    HStack::new(cx, |cx| {
                        Label::new(cx, I18n::phantom_partials(lang)).class("param-label").width(Pixels(86.0)).top(Stretch(1.0)).bottom(Stretch(1.0));
                        ParamSlider::new(cx, PianoViziaData::params, |p| &p.phantom_gain).height(Pixels(20.0)).width(Stretch(1.0)).top(Stretch(1.0)).bottom(Stretch(1.0));
                    }).height(Pixels(22.0)).col_between(Pixels(6.0));
                })
                .class("rack-box")
                .width(Stretch(1.0))
                .height(Pixels(165.0));

                // Rack 3: Spatial Mics & Lid
                VStack::new(cx, |cx| {
                    Label::new(cx, I18n::rack_spatial(lang)).class("rack-title");
                    HStack::new(cx, |cx| {
                        Label::new(cx, I18n::mic_close(lang)).class("param-label").width(Pixels(86.0)).top(Stretch(1.0)).bottom(Stretch(1.0));
                        ParamSlider::new(cx, PianoViziaData::params, |p| &p.mic_close).height(Pixels(20.0)).width(Stretch(1.0)).top(Stretch(1.0)).bottom(Stretch(1.0));
                    }).height(Pixels(22.0)).col_between(Pixels(6.0));
                    HStack::new(cx, |cx| {
                        Label::new(cx, I18n::mic_player(lang)).class("param-label").width(Pixels(86.0)).top(Stretch(1.0)).bottom(Stretch(1.0));
                        ParamSlider::new(cx, PianoViziaData::params, |p| &p.mic_player).height(Pixels(20.0)).width(Stretch(1.0)).top(Stretch(1.0)).bottom(Stretch(1.0));
                    }).height(Pixels(22.0)).col_between(Pixels(6.0));
                    HStack::new(cx, |cx| {
                        Label::new(cx, I18n::mic_ambient(lang)).class("param-label").width(Pixels(86.0)).top(Stretch(1.0)).bottom(Stretch(1.0));
                        ParamSlider::new(cx, PianoViziaData::params, |p| &p.mic_ambient).height(Pixels(20.0)).width(Stretch(1.0)).top(Stretch(1.0)).bottom(Stretch(1.0));
                    }).height(Pixels(22.0)).col_between(Pixels(6.0));
                    HStack::new(cx, |cx| {
                        Label::new(cx, I18n::lid_angle(lang)).class("param-label").width(Pixels(86.0)).top(Stretch(1.0)).bottom(Stretch(1.0));
                        ParamSlider::new(cx, PianoViziaData::params, |p| &p.lid_angle).height(Pixels(20.0)).width(Stretch(1.0)).top(Stretch(1.0)).bottom(Stretch(1.0));
                    }).height(Pixels(22.0)).col_between(Pixels(6.0));
                })
                .class("rack-box")
                .width(Stretch(1.0))
                .height(Pixels(165.0));

                // Rack 4: Output & Touch Response
                VStack::new(cx, |cx| {
                    Label::new(cx, I18n::rack_output(lang)).class("rack-title");
                    HStack::new(cx, |cx| {
                        Label::new(cx, I18n::master_gain(lang)).class("param-label").width(Pixels(86.0)).top(Stretch(1.0)).bottom(Stretch(1.0));
                        ParamSlider::new(cx, PianoViziaData::params, |p| &p.master_gain).height(Pixels(20.0)).width(Stretch(1.0)).top(Stretch(1.0)).bottom(Stretch(1.0));
                    }).height(Pixels(22.0)).col_between(Pixels(6.0));
                    HStack::new(cx, |cx| {
                        Label::new(cx, I18n::velocity_curve(lang)).class("param-label").width(Pixels(86.0)).top(Stretch(1.0)).bottom(Stretch(1.0));
                        ParamSlider::new(cx, PianoViziaData::params, |p| &p.velocity_curve).height(Pixels(20.0)).width(Stretch(1.0)).top(Stretch(1.0)).bottom(Stretch(1.0));
                    }).height(Pixels(22.0)).col_between(Pixels(6.0));
                })
                .class("rack-box")
                .width(Stretch(1.0))
                .height(Pixels(165.0));
            })
            .col_between(Pixels(8.0))
            .height(Pixels(170.0));

            // 4. Interactive 88-Key Keyboard with dynamic velocity travel sink
            PianoKeyboardWidget::new(
                cx,
                data.gui_tx.clone(),
                PianoViziaData::active_keys_low,
                PianoViziaData::active_keys_high,
                PianoViziaData::key_velocities,
            )
            .focusable(true)
            .height(Pixels(130.0))
            .width(Stretch(1.0));

            // Footer hint
            Label::new(cx, I18n::keyboard_hint(lang))
                .class("hint-text")
                .height(Pixels(16.0));
        })
        .child_space(Pixels(10.0))
        .row_between(Pixels(8.0));

        ResizeHandle::new(cx);
    })
}
