//! Vizia Plug editor for the physical piano.

use atomic_float::AtomicF32;
use crossbeam_channel::Sender;
use nice_plug::prelude::Param;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU64};
use vizia_plug::vizia::prelude::*;
use vizia_plug::widgets::util::ModifiersExt;
use vizia_plug::widgets::*;
use vizia_plug::{ViziaEditor, ViziaState, ViziaTheming, create_vizia_editor};

use crate::engine::EngineEvent;
use crate::gui::keyboard::PianoKeyboardWidget;
use crate::gui::lid::PianoLidWidget;
use crate::gui::mics::MicStageWidget;
use crate::gui::scope::LissajousScopeWidget;
use crate::nice_plugin::{PhysicsPianoParams, migrate_velocity_curve_params};
use physics_presets::{ParamTransition, Preset, PresetManager, UndoManager};
use physics_ui::Language;
use physics_ui::{
    CancelParamGestureEvent, LastCc, LearnArm, PresetBrowserAction, PresetBrowserItem,
    PresetBrowserSignals, PresetPanelAction, PresetPanelLayout, PresetPanelSignals,
    SpectrumAnalyzerWidget, StereoVuMeterWidget, VelocityCurveEditorWidget, WaveformWidget,
    add_base_theme, device_settings_row, theme_toggle_button,
    build_ptr_to_id_map, filter_browser_items, map_param_history_event, parameter_knob,
    parse_tags_input, preset_browser, preset_browser_categories, preset_browser_items,
    preset_browser_tags, preset_choices, preset_display_name, preset_panel, redraw_custom_view,
    set_param, setup_vizia_fonts, translate, translate_format,
};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};

pub const EDITOR_WIDTH: u32 = 1080;
pub const EDITOR_HEIGHT: u32 = 825;

fn knob<P: Param + 'static>(
    cx: &mut Context,
    label: &'static str,
    param: &P,
    tooltip: &'static str,
    learn: &LearnArm,
) {
    parameter_knob(cx, label, param, 120.0, tooltip, Some(learn.clone()));
}

struct PianoUiState {
    params: Arc<PhysicsPianoParams>,
    manager: Arc<parking_lot::RwLock<PresetManager>>,
    undo: Arc<parking_lot::RwLock<UndoManager>>,
    selected_name: Signal<String>,
    selected_id: Signal<String>,
    preset_choices: Signal<Vec<(String, String)>>,
    browser_open: Signal<bool>,
    browser_search: Signal<String>,
    browser_category: Signal<Option<String>>,
    browser_favorites_only: Signal<bool>,
    browser_categories: Signal<Vec<String>>,
    browser_filtered: Signal<Vec<PresetBrowserItem>>,
    browser_tags: Signal<Vec<String>>,
    browser_selected_tags: Signal<Vec<String>>,
    browser_tags_input: Signal<String>,
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
    // MIDI Learn state: the arm (mode + pending param id) lives in the
    // shared LearnArm; the poll bookkeeping below is handler-local.
    learn_arm: LearnArm,
    learn_status: Signal<String>,
    last_cc: Arc<LastCc>,
    last_seen_seq: u32,
    last_seen_pending: Option<String>,
}

#[derive(Debug)]
enum PianoUiEvent {
    PreviousPreset,
    NextPreset,
    SetLanguage(Language),
    Undo,
    Redo,
    SelectPreset(String),
    SetName(String),
    SaveAs,
    Rename,
    Overwrite,
    Delete,
    ToggleLearn,
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
        "velocity_point_25" => params.velocity_point_25.value(),
        "velocity_point_50" => params.velocity_point_50.value(),
        "velocity_point_75" => params.velocity_point_75.value(),
        "fx_eq_low" => params.fx_eq_low.value(),
        "fx_eq_mid" => params.fx_eq_mid.value(),
        "fx_eq_high" => params.fx_eq_high.value(),
        "fx_reverb_mix" => params.fx_reverb_mix.value(),
        "fx_reverb_decay" => params.fx_reverb_decay.value(),
        "sustain" => params.sustain_pedal.value(),
        "gain" => params.master_gain.value(),
        "unacorda" => f32::from(params.una_corda.value()),
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
        "velocity_point_25" => set_param(cx, &params.velocity_point_25, value),
        "velocity_point_50" => set_param(cx, &params.velocity_point_50, value),
        "velocity_point_75" => set_param(cx, &params.velocity_point_75, value),
        "fx_eq_low" => set_param(cx, &params.fx_eq_low, value),
        "fx_eq_mid" => set_param(cx, &params.fx_eq_mid, value),
        "fx_eq_high" => set_param(cx, &params.fx_eq_high, value),
        "fx_reverb_mix" => set_param(cx, &params.fx_reverb_mix, value),
        "fx_reverb_decay" => set_param(cx, &params.fx_reverb_decay, value),
        "sustain" => set_param(cx, &params.sustain_pedal, value),
        "gain" => set_param(cx, &params.master_gain, value),
        "unacorda" => set_param(cx, &params.una_corda, value >= 0.5),
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
        "velocity_point_25",
        "velocity_point_50",
        "velocity_point_75",
        "fx_eq_low",
        "fx_eq_mid",
        "fx_eq_high",
        "fx_reverb_mix",
        "fx_reverb_decay",
        "sustain",
        "gain",
        "unacorda",
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

    /// Refresh the preset dropdown rows and the browser rows/categories
    /// after any preset list or filter change.
    fn refresh_preset_signals(&self) {
        let manager = self.manager.read();
        let lang = self.language.get();
        self.preset_choices.set(preset_choices(&manager, lang));
        let items = preset_browser_items(&manager, lang);
        self.browser_categories
            .set(preset_browser_categories(&items));
        self.browser_tags.set(preset_browser_tags(&items));
        self.browser_filtered.set(filter_browser_items(
            &items,
            &self.browser_search.get(),
            self.browser_category.get().as_deref(),
            self.browser_favorites_only.get(),
            &self.browser_selected_tags.get(),
        ));
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
            PresetPanelAction::SelectLanguage(language) => {
                cx.emit(PianoUiEvent::SetLanguage(*language))
            }
            PresetPanelAction::Select(id) => cx.emit(PianoUiEvent::SelectPreset(id.clone())),
            PresetPanelAction::SetName(name) => cx.emit(PianoUiEvent::SetName(name.clone())),
            PresetPanelAction::SaveAs => cx.emit(PianoUiEvent::SaveAs),
            PresetPanelAction::Rename => cx.emit(PianoUiEvent::Rename),
            PresetPanelAction::Overwrite => cx.emit(PianoUiEvent::Overwrite),
            PresetPanelAction::Delete => cx.emit(PianoUiEvent::Delete),
            PresetPanelAction::Undo => cx.emit(PianoUiEvent::Undo),
            PresetPanelAction::Redo => cx.emit(PianoUiEvent::Redo),
        });
        event.map(|action: &PresetBrowserAction, _| match action {
            PresetBrowserAction::Open => self.browser_open.set(true),
            PresetBrowserAction::Close => self.browser_open.set(false),
            PresetBrowserAction::SetSearch(text) => {
                self.browser_search.set(text.clone());
                self.refresh_preset_signals();
            }
            PresetBrowserAction::SetCategory(category) => {
                self.browser_category.set(category.clone());
                self.refresh_preset_signals();
            }
            PresetBrowserAction::ToggleFavoritesOnly => {
                let next = !self.browser_favorites_only.get();
                self.browser_favorites_only.set(next);
                self.refresh_preset_signals();
            }
            PresetBrowserAction::ToggleFavorite(id) => {
                let _ = self.manager.write().toggle_favorite(id);
                self.refresh_preset_signals();
            }
            PresetBrowserAction::ToggleTag(tag) => {
                let mut selected = self.browser_selected_tags.get();
                if let Some(pos) = selected.iter().position(|t| t == tag) {
                    selected.remove(pos);
                } else {
                    selected.push(tag.clone());
                }
                self.browser_selected_tags.set(selected);
                self.refresh_preset_signals();
            }
            PresetBrowserAction::SetTagsInput(text) => {
                self.browser_tags_input.set(text.clone());
            }
            PresetBrowserAction::SetTags(text) => {
                let tags = parse_tags_input(text);
                let id = self.selected_id.get();
                let _ = self.manager.write().set_tags(&id, tags);
                self.browser_tags_input
                    .set(self.manager.read().tags_for(&id).join(", "));
                self.refresh_preset_signals();
            }
            PresetBrowserAction::Select(id) => {
                cx.emit(PianoUiEvent::SelectPreset(id.clone()));
                self.browser_open.set(false);
            }
        });
        event.map(|event, _| match event {
            PianoUiEvent::ToggleLearn => {
                let lang = self.language.get();
                let on = !self.learn_arm.mode.get();
                self.learn_arm.mode.set(on);
                if on {
                    self.learn_status
                        .set(translate(lang, "learn.on", "Learn on: click a knob").to_string());
                } else {
                    self.learn_arm.pending.set(None);
                    self.last_seen_pending = None;
                    self.learn_status
                        .set(translate(lang, "learn.off", "Learn off").to_string());
                }
            }
            PianoUiEvent::RefreshDisplay => {
                // MIDI Learn: while armed, poll for a newly moved
                // controller and bind it. Learn mode stays on after a
                // successful bind so several knobs can be bound in a row.
                if self.learn_arm.mode.get() {
                    let seq = self.last_cc.seq();
                    if seq != self.last_seen_seq {
                        self.last_seen_seq = seq;
                        if let Some(id) = self.learn_arm.pending.get() {
                            let cc = self.last_cc.cc();
                            let lang = self.language.get();
                            match self.manager.write().bind_midi_cc(cc, &id) {
                                Ok(true) => {
                                    self.learn_arm.pending.set(None);
                                    self.last_seen_pending = None;
                                    self.learn_status.set(translate_format(
                                        lang,
                                        "learn.bound",
                                        "Bound CC{cc} to {param}",
                                        &[("cc", &cc.to_string()), ("param", &id)],
                                    ));
                                }
                                Ok(false) => {
                                    self.learn_status.set(translate_format(
                                        lang,
                                        "learn.reserved",
                                        "CC{cc} is reserved and can't be learned",
                                        &[("cc", &cc.to_string()), ("param", &id)],
                                    ));
                                }
                                Err(err) => {
                                    eprintln!("MIDI Learn: failed to save binding: {err}");
                                }
                            }
                        }
                    }
                    // A newly armed knob (pending changed to Some) prompts
                    // the user to move a controller.
                    let pending = self.learn_arm.pending.get();
                    if pending != self.last_seen_pending {
                        self.last_seen_pending = pending.clone();
                        if let Some(id) = pending {
                            let lang = self.language.get();
                            self.learn_status.set(translate_format(
                                lang,
                                "learn.move_controller",
                                "Move a controller to bind {param}",
                                &[("param", &id)],
                            ));
                        }
                    }
                }
                redraw_custom_view(cx, "piano-keyboard-widget");
                redraw_custom_view(cx, "lissajous-scope-widget");
                redraw_custom_view(cx, "stereo-vu-meter-widget");
                redraw_custom_view(cx, "spectrum-analyzer-widget");
                redraw_custom_view(cx, "velocity-curve-editor");
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
                if let Some(mut preset) = preset {
                    // Old presets carry the legacy exponent entry; convert it
                    // to curve points before applying so their touch is kept
                    // at the five curve points (piecewise-linear in between).
                    migrate_velocity_curve_params(&mut preset.params);
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
                    let display_name = preset_display_name(&preset, self.language.get());
                    self.selected_name.set(display_name.clone());
                    self.name_input.set(display_name);
                    self.browser_tags_input
                        .set(self.manager.read().tags_for(id).join(", "));
                    self.is_user_preset
                        .set(self.manager.read().is_user_preset(id));
                    self.refresh_preset_signals();
                    self.set_undo_state();
                }
            }
            PianoUiEvent::SetLanguage(lang) => {
                let lang = *lang;
                self.language.set(lang);
                self.language_atom.store(lang.index(), Ordering::Relaxed);
                if let Some(preset) = self.manager.read().get_preset(&self.selected_id.get()) {
                    let display_name = preset_display_name(preset, lang);
                    self.selected_name.set(display_name.clone());
                    self.name_input.set(display_name);
                }
                self.refresh_preset_signals();
                redraw_custom_view(cx, "piano-lid-widget");
                redraw_custom_view(cx, "mic-stage-widget");
            }
            PianoUiEvent::Undo => self.apply_history(cx, false),
            PianoUiEvent::Redo => self.apply_history(cx, true),
            PianoUiEvent::SetName(name) => self.name_input.set(name.clone()),
            PianoUiEvent::SaveAs => {
                let name = self.name_input.get().trim().to_string();
                let name = if name.is_empty() {
                    translate(
                        self.language.get(),
                        "piano.preset.default-name",
                        "Custom Piano",
                    )
                    .to_owned()
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
                    self.browser_tags_input.set(String::new());
                    self.is_user_preset.set(true);
                    self.refresh_preset_signals();
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
                        self.refresh_preset_signals();
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
                    self.refresh_preset_signals();
                    cx.emit(PianoUiEvent::SelectPreset("steinway_concert_d".to_string()));
                }
            }
        });

        event.map(|cancel: &CancelParamGestureEvent, _| {
            let ptr = cancel.param;
            let params = &self.params;
            let id = if ptr == params.inharmonicity_scale.as_ptr() {
                "inharm"
            } else if ptr == params.hammer_hardness.as_ptr() {
                "hardness"
            } else if ptr == params.unison_detuning.as_ptr() {
                "detune"
            } else if ptr == params.phantom_gain.as_ptr() {
                "phantom"
            } else if ptr == params.key_noise.as_ptr() {
                "keynoise"
            } else if ptr == params.damper_noise.as_ptr() {
                "dampernoise"
            } else if ptr == params.pedal_noise.as_ptr() {
                "pedalnoise"
            } else if ptr == params.mic_close.as_ptr() {
                "mic_close"
            } else if ptr == params.mic_player.as_ptr() {
                "mic_player"
            } else if ptr == params.mic_ambient.as_ptr() {
                "mic_ambient"
            } else if ptr == params.lid_angle.as_ptr() {
                "lid_angle"
            } else if ptr == params.velocity_point_25.as_ptr() {
                "velocity_point_25"
            } else if ptr == params.velocity_point_50.as_ptr() {
                "velocity_point_50"
            } else if ptr == params.velocity_point_75.as_ptr() {
                "velocity_point_75"
            } else if ptr == params.fx_eq_low.as_ptr() {
                "fx_eq_low"
            } else if ptr == params.fx_eq_mid.as_ptr() {
                "fx_eq_mid"
            } else if ptr == params.fx_eq_high.as_ptr() {
                "fx_eq_high"
            } else if ptr == params.fx_reverb_mix.as_ptr() {
                "fx_reverb_mix"
            } else if ptr == params.fx_reverb_decay.as_ptr() {
                "fx_reverb_decay"
            } else if ptr == params.sustain_pedal.as_ptr() {
                "sustain"
            } else if ptr == params.master_gain.as_ptr() {
                "gain"
            } else if ptr == params.una_corda.as_ptr() {
                "unacorda"
            } else {
                return;
            };
            self.suppress_undo.fetch_add(1, Ordering::Relaxed);
            let restored = unsafe { ptr.preview_plain(cancel.restore_normalized) };
            if self.undo.write().cancel_last_single_param(id, restored) {
                self.set_undo_state();
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
            } else if ptr == params.velocity_point_25.as_ptr() {
                ("velocity_point_25", params.velocity_point_25.value())
            } else if ptr == params.velocity_point_50.as_ptr() {
                ("velocity_point_50", params.velocity_point_50.value())
            } else if ptr == params.velocity_point_75.as_ptr() {
                ("velocity_point_75", params.velocity_point_75.value())
            } else if ptr == params.fx_eq_low.as_ptr() {
                ("fx_eq_low", params.fx_eq_low.value())
            } else if ptr == params.fx_eq_mid.as_ptr() {
                ("fx_eq_mid", params.fx_eq_mid.value())
            } else if ptr == params.fx_eq_high.as_ptr() {
                ("fx_eq_high", params.fx_eq_high.value())
            } else if ptr == params.fx_reverb_mix.as_ptr() {
                ("fx_reverb_mix", params.fx_reverb_mix.value())
            } else if ptr == params.fx_reverb_decay.as_ptr() {
                ("fx_reverb_decay", params.fx_reverb_decay.value())
            } else if ptr == params.sustain_pedal.as_ptr() {
                ("sustain", params.sustain_pedal.value())
            } else if ptr == params.master_gain.as_ptr() {
                ("gain", params.master_gain.value())
            } else if ptr == params.una_corda.as_ptr() {
                ("unacorda", f32::from(params.una_corda.value()))
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
                    self.set_undo_state();
                }
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
    sample_history: Arc<parking_lot::RwLock<Vec<f32>>>,
    sample_rate: f32,
    language_atom: Arc<AtomicU8>,
    gui_tx: Sender<EngineEvent>,
    preset_manager: Arc<parking_lot::RwLock<PresetManager>>,
    undo_manager: Arc<parking_lot::RwLock<UndoManager>>,
    editor_state: Arc<ViziaState>,
    last_cc: Arc<LastCc>,
) -> Option<ViziaEditor> {
    let lang = Language::from_index(language_atom.load(std::sync::atomic::Ordering::Relaxed));
    let selected_name = preset_manager
        .read()
        .get_preset("steinway_concert_d")
        .map(|preset| preset_display_name(preset, lang))
        .unwrap_or_else(|| "Steinway D Concert Grand".to_string());
    let manager = preset_manager;
    let language_initial = lang;
    let initial_id = "steinway_concert_d".to_string();

    let zoom_state = editor_state.clone();
    create_vizia_editor(editor_state, ViziaTheming::Custom, move |cx, _| {
        // Signals need to belong to this Vizia tree's reactive scope.
        let preset_name = Signal::new(selected_name.clone());
        let preset_id = Signal::new(initial_id.clone());
        let preset_choices = Signal::new(preset_choices(&manager.read(), language_initial));
        let browser_items_initial = preset_browser_items(&manager.read(), language_initial);
        let browser_open = Signal::new(false);
        let browser_search = Signal::new(String::new());
        let browser_category = Signal::new(None);
        let browser_favorites_only = Signal::new(false);
        let browser_categories = Signal::new(preset_browser_categories(&browser_items_initial));
        let browser_tags = Signal::new(preset_browser_tags(&browser_items_initial));
        let browser_selected_tags = Signal::new(Vec::new());
        let browser_tags_input = Signal::new(manager.read().tags_for(&initial_id).join(", "));
        let browser_filtered = Signal::new(browser_items_initial);
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
        // MIDI Learn: the ptr -> id table is built from the params Arc,
        // whose allocation never moves, so the pointers stay valid.
        let learn_arm = LearnArm::new(build_ptr_to_id_map(&params));
        let learn_status = Signal::new(String::new());
        setup_vizia_fonts(cx);
        if let Err(err) = add_base_theme(cx) {
            eprintln!("Failed to load shared Vizia theme: {err:?}");
        }
        PianoUiState {
            params: params.clone(),
            manager: manager.clone(),
            undo: undo_manager.clone(),
            selected_name: preset_name.clone(),
            selected_id: preset_id,
            preset_choices: preset_choices.clone(),
            browser_open: browser_open.clone(),
            browser_search: browser_search.clone(),
            browser_category: browser_category.clone(),
            browser_favorites_only: browser_favorites_only.clone(),
            browser_categories: browser_categories.clone(),
            browser_filtered: browser_filtered.clone(),
            browser_tags: browser_tags.clone(),
            browser_selected_tags: browser_selected_tags.clone(),
            browser_tags_input: browser_tags_input.clone(),
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
            learn_arm: learn_arm.clone(),
            learn_status: learn_status.clone(),
            last_cc: last_cc.clone(),
            last_seen_seq: last_cc.seq(),
            last_seen_pending: None,
        }
        .build(cx);
        let display_timer = cx.add_timer(std::time::Duration::from_millis(33), None, |cx, _| {
            cx.emit(PianoUiEvent::RefreshDisplay)
        });
        cx.start_timer(display_timer);

        let params_ui = params.clone();
        let learn_ui = learn_arm.clone();
        let learn_status_ui = learn_status.clone();
        let peak_l_ui = peak_l.clone();
        let peak_r_ui = peak_r.clone();
        let active_keys_low_ui = active_keys_low.clone();
        let active_keys_high_ui = active_keys_high.clone();
        let key_velocities_ui = key_velocities.clone();
        let recent_orbit_t_ui = recent_orbit_t.clone();
        let recent_orbit_p_ui = recent_orbit_p.clone();
        let sample_history_ui = sample_history.clone();
        let gui_tx_ui = gui_tx.clone();
        let name_input_ui = name_input.clone();
        let preset_name_ui = preset_name.clone();
        let preset_choices_ui = preset_choices.clone();
        let browser_open_ui = browser_open.clone();
        let browser_search_ui = browser_search.clone();
        let browser_category_ui = browser_category.clone();
        let browser_favorites_only_ui = browser_favorites_only.clone();
        let browser_categories_ui = browser_categories.clone();
        let browser_filtered_ui = browser_filtered.clone();
        let browser_tags_ui = browser_tags.clone();
        let browser_selected_tags_ui = browser_selected_tags.clone();
        let browser_tags_input_ui = browser_tags_input.clone();
        let language_view_ui = language_view.clone();
        let is_user_preset_controls = is_user_preset_ui.clone();
        let can_undo_controls = can_undo_ui.clone();
        let can_redo_controls = can_redo_ui.clone();
        let zoom_state_ui = zoom_state.clone();
        let presets_dir_ui = manager.read().user_presets_dir().to_path_buf();

        Binding::new(cx, language_ui, move |cx| {
            let lang = language_ui.get();
            let params = params_ui.clone();
            let learn = learn_ui.clone();
            let learn_status = learn_status_ui.clone();
            let peak_l = peak_l_ui.clone();
            let peak_r = peak_r_ui.clone();
            let active_keys_low = active_keys_low_ui.clone();
            let active_keys_high = active_keys_high_ui.clone();
            let key_velocities = key_velocities_ui.clone();
            let recent_orbit_t = recent_orbit_t_ui.clone();
            let recent_orbit_p = recent_orbit_p_ui.clone();
            let sample_history = sample_history_ui.clone();
            let gui_tx = gui_tx_ui.clone();
            let initial_preset_name = preset_name_ui.clone();
            let preset_choices = preset_choices_ui.clone();
            let browser_open = browser_open_ui.clone();
            let browser_search = browser_search_ui.clone();
            let browser_category = browser_category_ui.clone();
            let browser_favorites_only = browser_favorites_only_ui.clone();
            let browser_categories = browser_categories_ui.clone();
            let browser_filtered = browser_filtered_ui.clone();
            let browser_tags = browser_tags_ui.clone();
            let browser_selected_tags = browser_selected_tags_ui.clone();
            let browser_tags_input = browser_tags_input_ui.clone();
            let browser_current_name = initial_preset_name.clone();
            let name_input = name_input_ui.clone();
            let language_view = language_view_ui.clone();
            let is_user_preset = is_user_preset_controls.clone();
            let can_undo = can_undo_controls.clone();
            let can_redo = can_redo_controls.clone();
            let zoom_state = zoom_state_ui.clone();
            let theme_signal = zoom_state.theme_signal();
            let theme_state = zoom_state.clone();
            let presets_dir = presets_dir_ui.clone();
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
                        name_width: 220.0,
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
                            zoom_control(
                                cx,
                                zoom_state.clone(),
                                translate(lang, "gui.zoom", "Zoom"),
                            );
                        })
                        .horizontal_gap(Pixels(8.0));
                    },
                );

                // MIDI Learn row: toggle button + status line.
                HStack::new(cx, |cx| {
                    let learn_mode = learn.mode;
                    Binding::new(cx, learn_mode, move |cx| {
                        let on = learn_mode.get();
                        Button::new(cx, |cx| {
                            Label::new(cx, if on { "LEARN ●" } else { "LEARN" })
                        })
                        .class("btn-action")
                        .on_press(|cx| {
                            cx.emit(PianoUiEvent::ToggleLearn);
                        })
                        .width(Pixels(64.0));
                    });
                    Label::new(cx, learn_status.clone()).class("small-note");
                    Element::new(cx)
                        .width(Stretch(1.0))
                        .pointer_events(PointerEvents::None);
                    theme_toggle_button(cx, theme_state.clone(), lang);
                })
                .horizontal_gap(Pixels(8.0));

                // Standalone-only audio/MIDI device row; hidden in plugin builds.
                device_settings_row(cx, presets_dir, lang);

                HStack::new(cx, |cx| {
                    PianoLidWidget::new(cx, params.clone(), lang)
                        .width(Stretch(1.0))
                        .height(Pixels(150.0))
                        .overflow(Overflow::Hidden)
                        .tooltip(move |cx| {
                            Tooltip::new(cx, |cx| {
                                Label::new(
                                    cx,
                                    translate(
                                        lang,
                                        "piano.tooltip.lid",
                                        "Drag to open or close the lid.",
                                    ),
                                )
                                .width(Pixels(240.0));
                            })
                        });
                    MicStageWidget::new(cx, params.clone(), lang)
                        .width(Stretch(1.0))
                        .height(Pixels(150.0))
                        .tooltip(move |cx| {
                            Tooltip::new(cx, |cx| {
                                Label::new(
                                    cx,
                                    translate(
                                        lang,
                                        "piano.tooltip.mic_stage",
                                        "Drag a mic up or down to change its gain.",
                                    ),
                                )
                                .width(Pixels(240.0));
                            })
                        });
                })
                .height(Pixels(154.0))
                .horizontal_gap(Pixels(8.0));

                HStack::new(cx, |cx| {
                    VStack::new(cx, |cx| {
                        Label::new(
                            cx,
                            translate(lang, "piano.rack_pedals", "PEDALS & MECHANICS"),
                        )
                        .class("rack-title");
                        HStack::new(cx, |cx| {
                            knob(
                                cx,
                                translate(lang, "piano.sustain", "Sustain:"),
                                &params.sustain_pedal,
                                translate(lang, "piano.tooltip.sustain", "How much the sustain pedal holds notes after you lift the keys."),
                            &learn);
                            knob(
                                cx,
                                translate(lang, "piano.key_action", "Key Action:"),
                                &params.key_noise,
                                translate(lang, "piano.tooltip.key_action", "Loudness of keybed thumps and escapement clicks."),
                            &learn);
                        })
                        .horizontal_gap(Pixels(8.0));
                        HStack::new(cx, |cx| {
                            knob(
                                cx,
                                translate(lang, "piano.damper_noise", "Damper Noise:"),
                                &params.damper_noise,
                                translate(lang, "piano.tooltip.damper_noise", "Noise of dampers lifting off and landing on the strings."),
                            &learn);
                            knob(
                                cx,
                                translate(lang, "piano.pedal_shock", "Pedal Shock:"),
                                &params.pedal_noise,
                                translate(lang, "piano.tooltip.pedal_shock", "Thump from the pedal mechanism itself."),
                            &learn);
                        })
                        .horizontal_gap(Pixels(8.0));
                    })
                    .class("rack-box")
                    .width(Stretch(1.0));
                    VStack::new(cx, |cx| {
                        Label::new(cx, translate(lang, "piano.rack_string", "STRING & HAMMER"))
                            .class("rack-title");
                        HStack::new(cx, |cx| {
                            knob(
                                cx,
                                translate(lang, "piano.inharmonicity", "Inharmonicity B:"),
                                &params.inharmonicity_scale,
                                translate(lang, "piano.tooltip.inharmonicity", "String stiffness (B factor): higher values push upper partials sharper."),
                            &learn);
                            knob(
                                cx,
                                translate(lang, "piano.hammer_hardness", "Hammer Hardness:"),
                                &params.hammer_hardness,
                                translate(lang, "piano.tooltip.hammer_hardness", "Hammer felt hardness: harder hammers sound brighter and more aggressive."),
                            &learn);
                        })
                        .horizontal_gap(Pixels(8.0));
                        HStack::new(cx, |cx| {
                            knob(
                                cx,
                                translate(lang, "piano.unison_detune", "Unison Detune:"),
                                &params.unison_detuning,
                                translate(lang, "piano.tooltip.unison_detune", "Detuning spread between the strings of each note's trichord."),
                            &learn);
                            knob(
                                cx,
                                translate(lang, "piano.phantom_partials", "Phantom Partials:"),
                                &params.phantom_gain,
                                translate(lang, "piano.tooltip.phantom_partials", "Longitudinal phantom partials: extra high shimmer from string vibration."),
                            &learn);
                        })
                        .horizontal_gap(Pixels(8.0));
                    })
                    .class("rack-box")
                    .width(Stretch(1.0));
                    VStack::new(cx, |cx| {
                        Label::new(
                            cx,
                            translate(lang, "piano.rack_spatial", "SPATIAL MICS & LID"),
                        )
                        .class("rack-title");
                        HStack::new(cx, |cx| {
                            knob(
                                cx,
                                translate(lang, "piano.mic_close", "Close Mic:"),
                                &params.mic_close,
                                translate(lang, "piano.tooltip.mic_close", "Close microphone pair over the bridges: direct, detailed sound."),
                            &learn);
                            knob(
                                cx,
                                translate(lang, "piano.mic_player", "Player Mic:"),
                                &params.mic_player,
                                translate(lang, "piano.tooltip.mic_player", "Microphones at the player's seat: what the pianist hears."),
                            &learn);
                        })
                        .horizontal_gap(Pixels(8.0));
                        HStack::new(cx, |cx| {
                            knob(
                                cx,
                                translate(lang, "piano.mic_ambient", "Ambient Mic:"),
                                &params.mic_ambient,
                                translate(lang, "piano.tooltip.mic_ambient", "Hall microphones in the room: spacious, reverberant sound."),
                            &learn);
                            knob(
                                cx,
                                translate(lang, "piano.lid_angle", "Lid Angle:"),
                                &params.lid_angle,
                                translate(lang, "piano.tooltip.lid_angle", "Grand lid opening angle: wider sounds louder and brighter."),
                            &learn);
                        })
                        .horizontal_gap(Pixels(8.0));
                    })
                    .class("rack-box")
                    .width(Stretch(1.0));
                    VStack::new(cx, |cx| {
                        Label::new(cx, translate(lang, "piano.rack_output", "OUTPUT"))
                            .class("rack-title");
                        HStack::new(cx, |cx| {
                            knob(
                                cx,
                                translate(lang, "piano.master_gain", "Master Gain:"),
                                &params.master_gain,
                                translate(lang, "piano.tooltip.master_gain", "Overall output volume."),
                            &learn);
                            VStack::new(cx, |cx| {
                                let label = translate(lang, "piano.velocity_curve", "Touch Curve:");
                                Label::new(cx, label.strip_suffix(':').unwrap_or(label))
                                    .class("knob-label");
                                VelocityCurveEditorWidget::new(
                                    cx,
                                    &params.velocity_point_25,
                                    &params.velocity_point_50,
                                    &params.velocity_point_75,
                                )
                                .width(Pixels(120.0))
                                .height(Pixels(56.0));
                            })
                            .class("knob-cell")
                            .width(Pixels(120.0))
                            .tooltip(move |cx| {
                                // Narrow label: this cell sits at the window's right edge,
                                // where a centered tooltip wider than ~130px would overflow
                                // (Vizia's auto-shift does not re-run once laid out).
                                Tooltip::new(cx, |cx| {
                                    Label::new(
                                        cx,
                                        translate(
                                            lang,
                                            "piano.tooltip.velocity_curve",
                                            "Drag points to shape velocity. Double-click / right-click resets a point.",
                                        ),
                                    )
                                    .width(Pixels(110.0));
                                })
                            });
                        })
                        .horizontal_gap(Pixels(8.0));
                        ParamButton::new(cx, &params.una_corda)
                            .with_label(translate(
                                lang,
                                "piano.una_corda",
                                "Una Corda",
                            ))
                            .width(Stretch(1.0));
                    })
                    .class("rack-box")
                    .width(Stretch(1.0));
                })
                .height(Pixels(170.0))
                .horizontal_gap(Pixels(8.0));

                VStack::new(cx, |cx| {
                    Label::new(cx, translate(lang, "fx.rack", "FX")).class("rack-title");
                    HStack::new(cx, |cx| {
                        knob(
                            cx,
                            translate(lang, "fx.eq_low", "EQ Low:"),
                            &params.fx_eq_low,
                            translate(
                                lang,
                                "fx.tooltip.eq_low",
                                "Low shelf EQ at 120 Hz: boosts or cuts bass.",
                            ),
                            &learn,
                        );
                        knob(
                            cx,
                            translate(lang, "fx.eq_mid", "EQ Mid:"),
                            &params.fx_eq_mid,
                            translate(
                                lang,
                                "fx.tooltip.eq_mid",
                                "Peaking EQ at 1 kHz: boosts or cuts the midrange.",
                            ),
                            &learn,
                        );
                        knob(
                            cx,
                            translate(lang, "fx.eq_high", "EQ High:"),
                            &params.fx_eq_high,
                            translate(
                                lang,
                                "fx.tooltip.eq_high",
                                "High shelf EQ at 8 kHz: boosts or cuts treble.",
                            ),
                            &learn,
                        );
                        knob(
                            cx,
                            translate(lang, "fx.reverb_mix", "Reverb Mix:"),
                            &params.fx_reverb_mix,
                            translate(
                                lang,
                                "fx.tooltip.reverb_mix",
                                "Reverb amount: 0 is dry, higher adds hall sound.",
                            ),
                            &learn,
                        );
                        knob(
                            cx,
                            translate(lang, "fx.reverb_decay", "Reverb Decay:"),
                            &params.fx_reverb_decay,
                            translate(
                                lang,
                                "fx.tooltip.reverb_decay",
                                "Reverb length: longer decay means a bigger hall.",
                            ),
                            &learn,
                        );
                    })
                    .horizontal_gap(Pixels(8.0));
                })
                .class("rack-box")
                .width(Stretch(1.0))
                .height(Pixels(110.0));

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
                Label::new(cx, translate(lang, "piano.keyboard_hint", "Virtual 88-Key Keyboard (A0-C8) | Play via mouse or QWERTY keys (A-K / W,E,T,Y,U), Z/X to shift octave")).class("hint-text");
                HStack::new(cx, |cx| {
                    Label::new(cx, translate(lang, "gui.spectrum", "Spectrum")).class("hint-text");
                    SpectrumAnalyzerWidget::new(cx, sample_history.clone(), sample_rate)
                        .width(Stretch(1.0))
                        .height(Pixels(56.0));
                })
                .height(Pixels(56.0))
                .horizontal_gap(Pixels(8.0));
                HStack::new(cx, |cx| {
                    Label::new(cx, translate(lang, "gui.waveform", "Waveform")).class("hint-text");
                    WaveformWidget::new(cx, sample_history.clone())
                        .width(Stretch(1.0))
                        .height(Pixels(56.0));
                })
                .height(Pixels(56.0))
                .horizontal_gap(Pixels(8.0));
            })
            .toggle_class("theme-light", theme_signal);

            preset_browser(
                cx,
                lang,
                PresetBrowserSignals {
                    open: browser_open,
                    search: browser_search,
                    category: browser_category,
                    favorites_only: browser_favorites_only,
                    categories: browser_categories,
                    filtered: browser_filtered,
                    tags: browser_tags,
                    selected_tags: browser_selected_tags,
                    tags_input: browser_tags_input,
                    current_name: browser_current_name,
                },
            );
        });
    })
}
