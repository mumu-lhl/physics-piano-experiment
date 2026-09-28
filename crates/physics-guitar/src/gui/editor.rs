//! Complete Vizia GUI Editor for Physics Guitar.
//!
//! Replaces the old egui immediate-mode layout with a high-performance,
//! retained-mode vector UI adhering to professional virtual instrument standards.
//! Integrates shared JSON preset persistence and granular Undo/Redo gesture tracking.

use crossbeam_channel::Sender;
use nih_plug::prelude::{Editor, Param, ParamPtr};
use nih_plug_vizia::vizia::prelude::*;
use nih_plug_vizia::widgets::util::ModifiersExt;
use nih_plug_vizia::widgets::*;
use nih_plug_vizia::{create_vizia_editor, ViziaState, ViziaTheming};
use physics_presets::{ParamTransition, Preset, PresetManager, UndoManager};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};
use std::sync::Arc;

use crate::gui::fretboard_view::GuitarFretboardWidget;
use crate::gui::i18n::{setup_vizia_fonts, I18n, Language};
use crate::nih_plugin::{GuiGuitarEvent, PhysicsGuitarParams};

pub const EDITOR_WIDTH: u32 = 1100;
pub const EDITOR_HEIGHT: u32 = 590;

fn apply_param<P: Param>(cx: &mut EventContext, param: &P, val: P::Plain) {
    cx.emit(ParamEvent::BeginSetParameter(param).upcast());
    cx.emit(ParamEvent::SetParameter(param, val).upcast());
    cx.emit(ParamEvent::EndSetParameter(param).upcast());
}

fn get_guitar_param_value(params: &PhysicsGuitarParams, param_id: &str) -> Option<f32> {
    match param_id {
        "mode" => Some(params.mode.value() as f32),
        "pluck_style" => Some(params.pluck_style.value() as f32),
        "pickup_pos" => Some(params.pickup_pos.value() as f32),
        "pickup_type" => Some(params.pickup_type.value() as f32),
        "tone" => Some(params.tone.value()),
        "palmmute" => Some(params.palm_mute.value()),
        "pluckpos" => Some(params.pluck_pos.value()),
        "amp_drive" => Some(params.amp_drive.value()),
        "cab_enabled" => Some(if params.cab_enabled.value() { 1.0 } else { 0.0 }),
        "strum_speed" => Some(params.strum_speed.value()),
        "fret_buzz" => Some(params.fret_buzz.value()),
        "finger_squeak" => Some(params.finger_squeak.value()),
        "groove_pattern" => Some(params.groove_pattern.value() as f32),
        "groove_bpm" => Some(params.groove_bpm.value()),
        "gain" => Some(params.master_gain.value()),
        _ => None,
    }
}

fn set_guitar_param_value(cx: &mut EventContext, params: &PhysicsGuitarParams, param_id: &str, val: f32) {
    match param_id {
        "mode" => apply_param(cx, &params.mode, val.round() as i32),
        "pluck_style" => apply_param(cx, &params.pluck_style, val.round() as i32),
        "pickup_pos" => apply_param(cx, &params.pickup_pos, val.round() as i32),
        "pickup_type" => apply_param(cx, &params.pickup_type, val.round() as i32),
        "tone" => apply_param(cx, &params.tone, val),
        "palmmute" => apply_param(cx, &params.palm_mute, val),
        "pluckpos" => apply_param(cx, &params.pluck_pos, val),
        "amp_drive" => apply_param(cx, &params.amp_drive, val),
        "cab_enabled" => apply_param(cx, &params.cab_enabled, val >= 0.5),
        "strum_speed" => apply_param(cx, &params.strum_speed, val),
        "fret_buzz" => apply_param(cx, &params.fret_buzz, val),
        "finger_squeak" => apply_param(cx, &params.finger_squeak, val),
        "groove_pattern" => apply_param(cx, &params.groove_pattern, val.round() as i32),
        "groove_bpm" => apply_param(cx, &params.groove_bpm, val),
        "gain" => apply_param(cx, &params.master_gain, val),
        _ => {}
    }
}

pub fn snapshot_guitar_params(params: &PhysicsGuitarParams) -> HashMap<String, f32> {
    let mut map = HashMap::new();
    map.insert("mode".to_string(), params.mode.value() as f32);
    map.insert("pluck_style".to_string(), params.pluck_style.value() as f32);
    map.insert("pickup_pos".to_string(), params.pickup_pos.value() as f32);
    map.insert("pickup_type".to_string(), params.pickup_type.value() as f32);
    map.insert("tone".to_string(), params.tone.value());
    map.insert("palmmute".to_string(), params.palm_mute.value());
    map.insert("pluckpos".to_string(), params.pluck_pos.value());
    map.insert("amp_drive".to_string(), params.amp_drive.value());
    map.insert("cab_enabled".to_string(), if params.cab_enabled.value() { 1.0 } else { 0.0 });
    map.insert("strum_speed".to_string(), params.strum_speed.value());
    map.insert("fret_buzz".to_string(), params.fret_buzz.value());
    map.insert("finger_squeak".to_string(), params.finger_squeak.value());
    map.insert("groove_pattern".to_string(), params.groove_pattern.value() as f32);
    map.insert("groove_bpm".to_string(), params.groove_bpm.value());
    map.insert("gain".to_string(), params.master_gain.value());
    map
}

#[derive(Lens, Clone)]
pub struct GuitarViziaData {
    pub params: Arc<PhysicsGuitarParams>,
    pub active_frets_shared: Arc<[AtomicU8; 6]>,
    pub string_energies_shared: Arc<[AtomicU32; 6]>,
    pub language_atom: Arc<AtomicU8>,
    pub language: Language,
    pub selected_preset_id: Option<String>,
    pub selected_preset_name: String,
    pub preset_manager: Arc<parking_lot::RwLock<PresetManager>>,
    pub undo_manager: Arc<parking_lot::RwLock<UndoManager>>,
    pub can_undo: bool,
    pub can_redo: bool,
    pub suppress_undo_gestures: Arc<AtomicU32>,
}

impl GuitarViziaData {
    fn resolve_param_ptr(&self, ptr: ParamPtr) -> (&'static str, f32) {
        let pr = &self.params;
        if ptr == pr.mode.as_ptr() {
            ("mode", pr.mode.value() as f32)
        } else if ptr == pr.pluck_style.as_ptr() {
            ("pluck_style", pr.pluck_style.value() as f32)
        } else if ptr == pr.pickup_pos.as_ptr() {
            ("pickup_pos", pr.pickup_pos.value() as f32)
        } else if ptr == pr.pickup_type.as_ptr() {
            ("pickup_type", pr.pickup_type.value() as f32)
        } else if ptr == pr.tone.as_ptr() {
            ("tone", pr.tone.value())
        } else if ptr == pr.palm_mute.as_ptr() {
            ("palmmute", pr.palm_mute.value())
        } else if ptr == pr.pluck_pos.as_ptr() {
            ("pluckpos", pr.pluck_pos.value())
        } else if ptr == pr.amp_drive.as_ptr() {
            ("amp_drive", pr.amp_drive.value())
        } else if ptr == pr.cab_enabled.as_ptr() {
            ("cab_enabled", if pr.cab_enabled.value() { 1.0 } else { 0.0 })
        } else if ptr == pr.strum_speed.as_ptr() {
            ("strum_speed", pr.strum_speed.value())
        } else if ptr == pr.fret_buzz.as_ptr() {
            ("fret_buzz", pr.fret_buzz.value())
        } else if ptr == pr.finger_squeak.as_ptr() {
            ("finger_squeak", pr.finger_squeak.value())
        } else if ptr == pr.groove_pattern.as_ptr() {
            ("groove_pattern", pr.groove_pattern.value() as f32)
        } else if ptr == pr.groove_bpm.as_ptr() {
            ("groove_bpm", pr.groove_bpm.value())
        } else if ptr == pr.master_gain.as_ptr() {
            ("gain", pr.master_gain.value())
        } else {
            ("", 0.0)
        }
    }
}

#[derive(Debug, Clone)]
pub enum GuitarUiEvent {
    ToggleLanguage,
    SelectPreset(String),
    CyclePreset(bool),
    PromptSavePreset,
    SavePreset(String),
    DeletePreset(String),
    Undo,
    Redo,
}

impl Model for GuitarViziaData {
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|app_event, _| match app_event {
            GuitarUiEvent::ToggleLanguage => {
                self.language = match self.language {
                    Language::English => Language::SimplifiedChinese,
                    Language::SimplifiedChinese => Language::English,
                };
                self.language_atom.store(
                    if self.language == Language::SimplifiedChinese { 1 } else { 0 },
                    Ordering::Relaxed,
                );
            }
            GuitarUiEvent::SelectPreset(preset_id) => {
                let preset_opt = {
                    let mgr = self.preset_manager.read();
                    mgr.get_preset(&preset_id).cloned()
                };
                if let Some(preset) = preset_opt {
                    let mut transitions = Vec::new();
                    for (pid, &new_val) in &preset.params {
                        if let Some(old_val) = get_guitar_param_value(&self.params, pid) {
                            if (new_val - old_val).abs() > 1e-4 {
                                transitions.push(ParamTransition {
                                    param_id: pid.clone(),
                                    old_value: old_val,
                                    new_value: new_val,
                                });
                            }
                            self.suppress_undo_gestures.fetch_add(1, Ordering::Relaxed);
                            set_guitar_param_value(cx, &self.params, pid, new_val);
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
            GuitarUiEvent::CyclePreset(next) => {
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
                    cx.emit(GuitarUiEvent::SelectPreset(next_id));
                }
            }
            GuitarUiEvent::PromptSavePreset => {
                let preset_count = self.preset_manager.read().presets().len();
                let new_id = format!("user_guitar_preset_{}", preset_count + 1);
                let new_name = format!("User Guitar {}", preset_count + 1);
                let params_map = snapshot_guitar_params(&self.params);
                let preset = Preset::new(new_id.clone(), new_name.clone(), "guitar", params_map);
                let mut mgr = self.preset_manager.write();
                if let Ok(()) = mgr.save_user_preset(preset) {
                    self.selected_preset_id = Some(new_id);
                    self.selected_preset_name = new_name;
                }
            }
            GuitarUiEvent::SavePreset(name) => {
                let safe_id = name.to_lowercase().replace(' ', "_");
                let params_map = snapshot_guitar_params(&self.params);
                let preset = Preset::new(safe_id.clone(), name.clone(), "guitar", params_map);
                let mut mgr = self.preset_manager.write();
                if let Ok(()) = mgr.save_user_preset(preset) {
                    self.selected_preset_id = Some(safe_id);
                    self.selected_preset_name = name.clone();
                }
            }
            GuitarUiEvent::DeletePreset(preset_id) => {
                let mut mgr = self.preset_manager.write();
                let _ = mgr.delete_user_preset(&preset_id);
                if self.selected_preset_id.as_deref() == Some(&preset_id) {
                    self.selected_preset_id = None;
                    self.selected_preset_name = "Strat Clean Chime".to_string();
                }
            }
            GuitarUiEvent::Undo => {
                let restores = {
                    let mut mgr = self.undo_manager.write();
                    mgr.undo()
                };
                if let Some(restores) = restores {
                    for (pid, val) in restores {
                        self.suppress_undo_gestures.fetch_add(1, Ordering::Relaxed);
                        set_guitar_param_value(cx, &self.params, &pid, val);
                    }
                    let (u, r) = {
                        let mgr = self.undo_manager.read();
                        (mgr.can_undo(), mgr.can_redo())
                    };
                    self.can_undo = u;
                    self.can_redo = r;
                }
            }
            GuitarUiEvent::Redo => {
                let restores = {
                    let mut mgr = self.undo_manager.write();
                    mgr.redo()
                };
                if let Some(restores) = restores {
                    for (pid, val) in restores {
                        self.suppress_undo_gestures.fetch_add(1, Ordering::Relaxed);
                        set_guitar_param_value(cx, &self.params, &pid, val);
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

        // Keyboard shortcuts: Ctrl+Z (Undo), Ctrl+Y / Ctrl+Shift+Z (Redo)
        event.map(|window_event: &WindowEvent, _| {
            if let WindowEvent::KeyDown(code, _) = window_event {
                if cx.modifiers().command() {
                    if *code == Code::KeyZ && !cx.modifiers().shift() {
                        cx.emit(GuitarUiEvent::Undo);
                    } else if *code == Code::KeyY || (*code == Code::KeyZ && cx.modifiers().shift()) {
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
    let initial_lang = if language_atom.load(Ordering::Relaxed) == 1 {
        Language::SimplifiedChinese
    } else {
        Language::English
    };

    let initial_preset_name = {
        let mgr = preset_manager.read();
        mgr.get_preset("strat_clean_chime")
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "Strat Clean Chime".to_string())
    };

    let data = GuitarViziaData {
        params,
        active_frets_shared,
        string_energies_shared,
        language_atom,
        language: initial_lang,
        selected_preset_id: Some("strat_clean_chime".to_string()),
        selected_preset_name: initial_preset_name,
        preset_manager,
        undo_manager,
        can_undo: false,
        can_redo: false,
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
            // 1. Header Bar: Title, Compact Preset Selector, Undo/Redo, Language
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
                        |cx| cx.emit(GuitarUiEvent::CyclePreset(false)),
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
                                GuitarViziaData::selected_preset_name.map(|name| format!("{} ▾", name)),
                            )
                            .class("preset-dropdown-label")
                            .top(Stretch(1.0))
                            .bottom(Stretch(1.0))
                        },
                        |cx| {
                            Binding::new(cx, GuitarViziaData::preset_manager, |cx, mgr_lens| {
                                let mgr_arc = mgr_lens.get(cx);
                                let mgr = mgr_arc.read();
                                for preset in mgr.presets() {
                                    let pid = preset.id.clone();
                                    let pname = preset.name.clone();
                                    Label::new(cx, &pname)
                                        .class("preset-item")
                                        .on_press(move |cx| {
                                            cx.emit(GuitarUiEvent::SelectPreset(pid.clone()));
                                            cx.emit(PopupEvent::Close);
                                        });
                                }
                            });
                        },
                    )
                    .class("preset-dropdown")
                    .height(Pixels(24.0))
                    .width(Pixels(185.0));

                    Button::new(
                        cx,
                        |cx| cx.emit(GuitarUiEvent::CyclePreset(true)),
                        |cx| Label::new(cx, "▶"),
                    )
                    .class("btn-cycle")
                    .width(Pixels(26.0))
                    .height(Pixels(24.0));

                    Button::new(
                        cx,
                        |cx| cx.emit(GuitarUiEvent::PromptSavePreset),
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
                        |cx| cx.emit(GuitarUiEvent::Undo),
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
                        |cx| cx.emit(GuitarUiEvent::Redo),
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
                    |cx| cx.emit(GuitarUiEvent::ToggleLanguage),
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
            })
            .height(Pixels(38.0))
            .child_top(Stretch(1.0))
            .child_bottom(Stretch(1.0))
            .col_between(Pixels(10.0));

            // 2. Four Parameter Control Racks with aligned labels and sliders
            HStack::new(cx, |cx| {
                // Rack 1: Instrument & Pickup Circuit
                VStack::new(cx, |cx| {
                    Label::new(cx, I18n::rack_instrument(lang)).class("rack-title");
                    HStack::new(cx, |cx| {
                        Label::new(cx, "Mode:").class("param-label").width(Pixels(86.0)).top(Stretch(1.0)).bottom(Stretch(1.0));
                        ParamSlider::new(cx, GuitarViziaData::params, |p| &p.mode).height(Pixels(20.0)).width(Stretch(1.0)).top(Stretch(1.0)).bottom(Stretch(1.0));
                    }).height(Pixels(22.0)).col_between(Pixels(6.0));
                    HStack::new(cx, |cx| {
                        Label::new(cx, "Pickup:").class("param-label").width(Pixels(86.0)).top(Stretch(1.0)).bottom(Stretch(1.0));
                        ParamSlider::new(cx, GuitarViziaData::params, |p| &p.pickup_pos).height(Pixels(20.0)).width(Stretch(1.0)).top(Stretch(1.0)).bottom(Stretch(1.0));
                    }).height(Pixels(22.0)).col_between(Pixels(6.0));
                    HStack::new(cx, |cx| {
                        Label::new(cx, "Coil:").class("param-label").width(Pixels(86.0)).top(Stretch(1.0)).bottom(Stretch(1.0));
                        ParamSlider::new(cx, GuitarViziaData::params, |p| &p.pickup_type).height(Pixels(20.0)).width(Stretch(1.0)).top(Stretch(1.0)).bottom(Stretch(1.0));
                    }).height(Pixels(22.0)).col_between(Pixels(6.0));
                    HStack::new(cx, |cx| {
                        Label::new(cx, "Tone:").class("param-label").width(Pixels(86.0)).top(Stretch(1.0)).bottom(Stretch(1.0));
                        ParamSlider::new(cx, GuitarViziaData::params, |p| &p.tone).height(Pixels(20.0)).width(Stretch(1.0)).top(Stretch(1.0)).bottom(Stretch(1.0));
                    }).height(Pixels(22.0)).col_between(Pixels(6.0));
                })
                .class("rack-box")
                .width(Stretch(1.0))
                .height(Pixels(165.0));

                // Rack 2: Tube Preamp & Tone Shaping
                VStack::new(cx, |cx| {
                    Label::new(cx, I18n::rack_amp(lang)).class("rack-title");
                    HStack::new(cx, |cx| {
                        Label::new(cx, "Overdrive:").class("param-label").width(Pixels(86.0)).top(Stretch(1.0)).bottom(Stretch(1.0));
                        ParamSlider::new(cx, GuitarViziaData::params, |p| &p.amp_drive).height(Pixels(20.0)).width(Stretch(1.0)).top(Stretch(1.0)).bottom(Stretch(1.0));
                    }).height(Pixels(22.0)).col_between(Pixels(6.0));
                    HStack::new(cx, |cx| {
                        Label::new(cx, "PalmMute:").class("param-label").width(Pixels(86.0)).top(Stretch(1.0)).bottom(Stretch(1.0));
                        ParamSlider::new(cx, GuitarViziaData::params, |p| &p.palm_mute).height(Pixels(20.0)).width(Stretch(1.0)).top(Stretch(1.0)).bottom(Stretch(1.0));
                    }).height(Pixels(22.0)).col_between(Pixels(6.0));
                    HStack::new(cx, |cx| {
                        Label::new(cx, "PluckPos:").class("param-label").width(Pixels(86.0)).top(Stretch(1.0)).bottom(Stretch(1.0));
                        ParamSlider::new(cx, GuitarViziaData::params, |p| &p.pluck_pos).height(Pixels(20.0)).width(Stretch(1.0)).top(Stretch(1.0)).bottom(Stretch(1.0));
                    }).height(Pixels(22.0)).col_between(Pixels(6.0));
                })
                .class("rack-box")
                .width(Stretch(1.0))
                .height(Pixels(165.0));

                // Rack 3: Mechanics & Organic Noises
                VStack::new(cx, |cx| {
                    Label::new(cx, I18n::rack_mechanics(lang)).class("rack-title");
                    HStack::new(cx, |cx| {
                        Label::new(cx, "Style:").class("param-label").width(Pixels(86.0)).top(Stretch(1.0)).bottom(Stretch(1.0));
                        ParamSlider::new(cx, GuitarViziaData::params, |p| &p.pluck_style).height(Pixels(20.0)).width(Stretch(1.0)).top(Stretch(1.0)).bottom(Stretch(1.0));
                    }).height(Pixels(22.0)).col_between(Pixels(6.0));
                    HStack::new(cx, |cx| {
                        Label::new(cx, "Speed:").class("param-label").width(Pixels(86.0)).top(Stretch(1.0)).bottom(Stretch(1.0));
                        ParamSlider::new(cx, GuitarViziaData::params, |p| &p.strum_speed).height(Pixels(20.0)).width(Stretch(1.0)).top(Stretch(1.0)).bottom(Stretch(1.0));
                    }).height(Pixels(22.0)).col_between(Pixels(6.0));
                    HStack::new(cx, |cx| {
                        Label::new(cx, "Buzz:").class("param-label").width(Pixels(86.0)).top(Stretch(1.0)).bottom(Stretch(1.0));
                        ParamSlider::new(cx, GuitarViziaData::params, |p| &p.fret_buzz).height(Pixels(20.0)).width(Stretch(1.0)).top(Stretch(1.0)).bottom(Stretch(1.0));
                    }).height(Pixels(22.0)).col_between(Pixels(6.0));
                    HStack::new(cx, |cx| {
                        Label::new(cx, "Squeak:").class("param-label").width(Pixels(86.0)).top(Stretch(1.0)).bottom(Stretch(1.0));
                        ParamSlider::new(cx, GuitarViziaData::params, |p| &p.finger_squeak).height(Pixels(20.0)).width(Stretch(1.0)).top(Stretch(1.0)).bottom(Stretch(1.0));
                    }).height(Pixels(22.0)).col_between(Pixels(6.0));
                })
                .class("rack-box")
                .width(Stretch(1.0))
                .height(Pixels(165.0));

                // Rack 4: Groove & Master Output
                VStack::new(cx, |cx| {
                    Label::new(cx, I18n::rack_master(lang)).class("rack-title");
                    HStack::new(cx, |cx| {
                        Label::new(cx, "Pattern:").class("param-label").width(Pixels(86.0)).top(Stretch(1.0)).bottom(Stretch(1.0));
                        ParamSlider::new(cx, GuitarViziaData::params, |p| &p.groove_pattern).height(Pixels(20.0)).width(Stretch(1.0)).top(Stretch(1.0)).bottom(Stretch(1.0));
                    }).height(Pixels(22.0)).col_between(Pixels(6.0));
                    HStack::new(cx, |cx| {
                        Label::new(cx, "BPM:").class("param-label").width(Pixels(86.0)).top(Stretch(1.0)).bottom(Stretch(1.0));
                        ParamSlider::new(cx, GuitarViziaData::params, |p| &p.groove_bpm).height(Pixels(20.0)).width(Stretch(1.0)).top(Stretch(1.0)).bottom(Stretch(1.0));
                    }).height(Pixels(22.0)).col_between(Pixels(6.0));
                    HStack::new(cx, |cx| {
                        Label::new(cx, "Volume:").class("param-label").width(Pixels(86.0)).top(Stretch(1.0)).bottom(Stretch(1.0));
                        ParamSlider::new(cx, GuitarViziaData::params, |p| &p.master_gain).height(Pixels(20.0)).width(Stretch(1.0)).top(Stretch(1.0)).bottom(Stretch(1.0));
                    }).height(Pixels(22.0)).col_between(Pixels(6.0));
                })
                .class("rack-box")
                .width(Stretch(1.0))
                .height(Pixels(165.0));
            })
            .col_between(Pixels(8.0))
            .height(Pixels(170.0));

            // 3. Interactive 24-Fret 6-String Fretboard
            GuitarFretboardWidget::new(
                cx,
                gui_tx.clone(),
                GuitarViziaData::active_frets_shared,
                GuitarViziaData::string_energies_shared,
            )
            .height(Pixels(150.0))
            .width(Stretch(1.0));

            // Footer hint
            Label::new(cx, I18n::fretboard_hint(lang))
                .class("hint-text")
                .height(Pixels(16.0));
        })
        .child_space(Pixels(10.0))
        .row_between(Pixels(8.0));

        ResizeHandle::new(cx);
    })
}
