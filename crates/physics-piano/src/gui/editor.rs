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
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
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
    pub preset_manager: Arc<parking_lot::RwLock<PresetManager>>,
    pub undo_manager: Arc<parking_lot::RwLock<UndoManager>>,
    pub can_undo: bool,
    pub can_redo: bool,
}

impl PianoViziaData {
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
                let mgr = self.preset_manager.read();
                if let Some(preset) = mgr.get_preset(preset_id) {
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
                            set_piano_param_value(cx, &self.params, pid, new_val);
                        }
                    }
                    if !transitions.is_empty() {
                        self.undo_manager.write().record_batch(&preset.name, transitions);
                    }
                    self.selected_preset_id = Some(preset_id.clone());
                    self.can_undo = self.undo_manager.read().can_undo();
                    self.can_redo = self.undo_manager.read().can_redo();
                }
            }
            PianoUiEvent::PromptSavePreset => {
                let preset_count = self.preset_manager.read().presets().len();
                let new_id = format!("user_piano_preset_{}", preset_count + 1);
                let new_name = format!("User Piano {}", preset_count + 1);
                let params_map = snapshot_piano_params(&self.params);
                let preset = Preset::new(new_id.clone(), new_name, "piano", params_map);
                let mut mgr = self.preset_manager.write();
                if let Ok(()) = mgr.save_user_preset(preset) {
                    self.selected_preset_id = Some(new_id);
                }
            }
            PianoUiEvent::SavePreset(name) => {
                let safe_id = name.to_lowercase().replace(' ', "_");
                let params_map = snapshot_piano_params(&self.params);
                let preset = Preset::new(safe_id.clone(), name.clone(), "piano", params_map);
                let mut mgr = self.preset_manager.write();
                if let Ok(()) = mgr.save_user_preset(preset) {
                    self.selected_preset_id = Some(safe_id);
                }
            }
            PianoUiEvent::DeletePreset(preset_id) => {
                let mut mgr = self.preset_manager.write();
                let _ = mgr.delete_user_preset(preset_id);
                if self.selected_preset_id.as_deref() == Some(preset_id) {
                    self.selected_preset_id = None;
                }
            }
            PianoUiEvent::Undo => {
                if let Some(restores) = self.undo_manager.write().undo() {
                    for (pid, val) in restores {
                        set_piano_param_value(cx, &self.params, &pid, val);
                    }
                    self.can_undo = self.undo_manager.read().can_undo();
                    self.can_redo = self.undo_manager.read().can_redo();
                }
            }
            PianoUiEvent::Redo => {
                if let Some(restores) = self.undo_manager.write().redo() {
                    for (pid, val) in restores {
                        set_piano_param_value(cx, &self.params, &pid, val);
                    }
                    self.can_undo = self.undo_manager.read().can_undo();
                    self.can_redo = self.undo_manager.read().can_redo();
                }
            }
        });

        // Intercept parameter slider gestures for single-parameter undo/redo
        event.map(|raw_param: &RawParamEvent, _| match raw_param {
            RawParamEvent::BeginSetParameter(ptr) => {
                let (pid, val) = self.resolve_param_ptr(*ptr);
                if !pid.is_empty() {
                    self.undo_manager.write().begin_gesture(pid, val);
                }
            }
            RawParamEvent::EndSetParameter(ptr) => {
                let (pid, val) = self.resolve_param_ptr(*ptr);
                if !pid.is_empty() {
                    if self.undo_manager.write().end_gesture(pid, val) {
                        self.can_undo = self.undo_manager.read().can_undo();
                        self.can_redo = self.undo_manager.read().can_redo();
                    }
                }
            }
            _ => {}
        });

        // Keyboard shortcuts: Ctrl+Z (Undo), Ctrl+Y / Ctrl+Shift+Z (Redo)
        event.map(|window_event: &WindowEvent, _| {
            if let WindowEvent::KeyDown(code, _) = window_event {
                if cx.modifiers().command() {
                    if *code == Code::KeyZ && !cx.modifiers().shift() {
                        cx.emit(PianoUiEvent::Undo);
                    } else if *code == Code::KeyY || (*code == Code::KeyZ && cx.modifiers().shift()) {
                        cx.emit(PianoUiEvent::Redo);
                    }
                }
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
        preset_manager,
        undo_manager,
        can_undo: false,
        can_redo: false,
    };

    create_vizia_editor(editor_state, ViziaTheming::Custom, move |cx, _gui_cx| {
        setup_vizia_fonts(cx);

        if let Err(err) = cx.add_stylesheet(include_style!("src/gui/theme.css")) {
            nih_plug::nih_error!("Failed to load Vizia stylesheet: {err:?}");
        }

        data.clone().build(cx);

        let lang = data.language;

        VStack::new(cx, |cx| {
            // 1. Header Bar: Title, Language, Undo/Redo, Presets, Save Preset, Meter
            HStack::new(cx, |cx| {
                VStack::new(cx, |cx| {
                    Label::new(cx, I18n::title(lang)).class("title");
                    Label::new(cx, I18n::subtitle(lang)).class("subtitle");
                })
                .width(Stretch(1.0));

                // Undo & Redo Buttons
                HStack::new(cx, |cx| {
                    Button::new(
                        cx,
                        |cx| cx.emit(PianoUiEvent::Undo),
                        |cx| Label::new(cx, "↶ Undo"),
                    )
                    .class("btn-icon")
                    .width(Pixels(56.0));

                    Button::new(
                        cx,
                        |cx| cx.emit(PianoUiEvent::Redo),
                        |cx| Label::new(cx, "↷ Redo"),
                    )
                    .class("btn-icon")
                    .width(Pixels(56.0));
                })
                .col_between(Pixels(4.0));

                // Save Custom Preset Button
                Button::new(
                    cx,
                    |cx| cx.emit(PianoUiEvent::PromptSavePreset),
                    |cx| {
                        Label::new(
                            cx,
                            match lang {
                                Language::English => "💾 Save",
                                Language::SimplifiedChinese => "💾 保存",
                            },
                        )
                    },
                )
                .width(Pixels(62.0));

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
                .width(Pixels(74.0));

                // Visualizers (Lissajous & Stereo VU Meter)
                HStack::new(cx, |cx| {
                    LissajousScopeWidget::new(cx, PianoViziaData::recent_orbit_t, PianoViziaData::recent_orbit_p)
                        .size(Pixels(36.0));
                    StereoVuMeterWidget::new(cx, PianoViziaData::peak_l, PianoViziaData::peak_r)
                        .width(Pixels(70.0))
                        .height(Pixels(28.0));
                })
                .col_between(Pixels(10.0));
            })
            .height(Pixels(38.0))
            .child_top(Stretch(1.0))
            .child_bottom(Stretch(1.0))
            .col_between(Pixels(10.0));

            // 1b. Preset Selection Bar
            HStack::new(cx, |cx| {
                let mgr = data.preset_manager.read();
                let presets = mgr.presets().to_vec();
                for preset in presets {
                    let p_id = preset.id.clone();
                    let p_name = preset.display_name(lang == Language::SimplifiedChinese).to_string();
                    let is_factory = preset.is_factory;

                    HStack::new(cx, |cx| {
                        let id_for_select = p_id.clone();
                        Button::new(
                            cx,
                            move |cx| {
                                cx.emit(PianoUiEvent::SelectPreset(id_for_select.clone()));
                            },
                            move |cx| Label::new(cx, &p_name),
                        )
                        .height(Pixels(22.0));

                        // If user preset, show delete button
                        if !is_factory {
                            let id_for_del = p_id.clone();
                            Button::new(
                                cx,
                                move |cx| {
                                    cx.emit(PianoUiEvent::DeletePreset(id_for_del.clone()));
                                },
                                |cx| Label::new(cx, "✕"),
                            )
                            .width(Pixels(18.0))
                            .height(Pixels(22.0));
                        }
                    })
                    .col_between(Pixels(1.0));
                }
            })
            .height(Pixels(24.0))
            .col_between(Pixels(4.0));

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

            // 3. Four Parameter Control Racks
            HStack::new(cx, |cx| {
                // Rack 1: Pedals & Mechanics
                VStack::new(cx, |cx| {
                    Label::new(cx, I18n::rack_pedals(lang)).class("rack-title");
                    HStack::new(cx, |cx| {
                        Label::new(cx, I18n::sustain(lang)).class("param-label");
                        ParamSlider::new(cx, PianoViziaData::params, |p| &p.sustain_pedal);
                    }).size(Auto);
                    HStack::new(cx, |cx| {
                        Label::new(cx, I18n::key_action(lang)).class("param-label");
                        ParamSlider::new(cx, PianoViziaData::params, |p| &p.key_noise);
                    }).size(Auto);
                    HStack::new(cx, |cx| {
                        Label::new(cx, I18n::damper_noise(lang)).class("param-label");
                        ParamSlider::new(cx, PianoViziaData::params, |p| &p.damper_noise);
                    }).size(Auto);
                    HStack::new(cx, |cx| {
                        Label::new(cx, I18n::pedal_shock(lang)).class("param-label");
                        ParamSlider::new(cx, PianoViziaData::params, |p| &p.pedal_noise);
                    }).size(Auto);
                })
                .class("rack-box")
                .width(Stretch(1.0))
                .height(Pixels(175.0));

                // Rack 2: String & Hammer Physics
                VStack::new(cx, |cx| {
                    Label::new(cx, I18n::rack_string(lang)).class("rack-title");
                    HStack::new(cx, |cx| {
                        Label::new(cx, I18n::inharmonicity(lang)).class("param-label");
                        ParamSlider::new(cx, PianoViziaData::params, |p| &p.inharmonicity_scale);
                    }).size(Auto);
                    HStack::new(cx, |cx| {
                        Label::new(cx, I18n::hammer_hardness(lang)).class("param-label");
                        ParamSlider::new(cx, PianoViziaData::params, |p| &p.hammer_hardness);
                    }).size(Auto);
                    HStack::new(cx, |cx| {
                        Label::new(cx, I18n::unison_detune(lang)).class("param-label");
                        ParamSlider::new(cx, PianoViziaData::params, |p| &p.unison_detuning);
                    }).size(Auto);
                    HStack::new(cx, |cx| {
                        Label::new(cx, I18n::phantom_partials(lang)).class("param-label");
                        ParamSlider::new(cx, PianoViziaData::params, |p| &p.phantom_gain);
                    }).size(Auto);
                })
                .class("rack-box")
                .width(Stretch(1.0))
                .height(Pixels(175.0));

                // Rack 3: Spatial Mics & Lid
                VStack::new(cx, |cx| {
                    Label::new(cx, I18n::rack_spatial(lang)).class("rack-title");
                    HStack::new(cx, |cx| {
                        Label::new(cx, I18n::mic_close(lang)).class("param-label");
                        ParamSlider::new(cx, PianoViziaData::params, |p| &p.mic_close);
                    }).size(Auto);
                    HStack::new(cx, |cx| {
                        Label::new(cx, I18n::mic_player(lang)).class("param-label");
                        ParamSlider::new(cx, PianoViziaData::params, |p| &p.mic_player);
                    }).size(Auto);
                    HStack::new(cx, |cx| {
                        Label::new(cx, I18n::mic_ambient(lang)).class("param-label");
                        ParamSlider::new(cx, PianoViziaData::params, |p| &p.mic_ambient);
                    }).size(Auto);
                    HStack::new(cx, |cx| {
                        Label::new(cx, I18n::lid_angle(lang)).class("param-label");
                        ParamSlider::new(cx, PianoViziaData::params, |p| &p.lid_angle);
                    }).size(Auto);
                })
                .class("rack-box")
                .width(Stretch(1.0))
                .height(Pixels(175.0));

                // Rack 4: Output & Touch Response
                VStack::new(cx, |cx| {
                    Label::new(cx, I18n::rack_output(lang)).class("rack-title");
                    HStack::new(cx, |cx| {
                        Label::new(cx, I18n::master_gain(lang)).class("param-label");
                        ParamSlider::new(cx, PianoViziaData::params, |p| &p.master_gain);
                    }).size(Auto);
                    HStack::new(cx, |cx| {
                        Label::new(cx, I18n::velocity_curve(lang)).class("param-label");
                        ParamSlider::new(cx, PianoViziaData::params, |p| &p.velocity_curve);
                    }).size(Auto);
                })
                .class("rack-box")
                .width(Stretch(1.0))
                .height(Pixels(175.0));
            })
            .col_between(Pixels(8.0))
            .height(Pixels(180.0));

            // 4. Interactive 88-Key Keyboard with dynamic velocity travel sink
            PianoKeyboardWidget::new(
                cx,
                gui_tx.clone(),
                PianoViziaData::active_keys_low,
                PianoViziaData::active_keys_high,
                PianoViziaData::key_velocities,
            )
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
