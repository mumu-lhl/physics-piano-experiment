//! Vizia editor for the physical bass.
//!
//! The layout follows the piano/guitar editors: preset and language controls at
//! the top, parameter racks in the middle, and the playable instrument view at
//! the bottom.  The fretboard itself is a custom Skia view, not a collection of
//! tiny buttons, so it remains usable at plugin-sized window dimensions.

mod fretboard_view;

use crate::nice_plugin::{GuiBassEvent, PhysicsBassParams};
use fretboard_view::BassFretboardWidget;
use nice_plug::prelude::Param;
use physics_presets::{ParamTransition, Preset, PresetManager, UndoManager};
use physics_ui::{
    CancelParamGestureEvent, Language, LastCc, LearnArm, PresetBrowserAction, PresetBrowserItem,
    PresetBrowserSignals, PresetPanelAction, PresetPanelLayout, PresetPanelSignals,
    SpectrumAnalyzerWidget, StereoVuMeterWidget, WaveformWidget, add_base_theme, theme_toggle_button,
    build_ptr_to_id_map,
    discrete_selector, filter_browser_items, map_param_history_event, parameter_knob,
    parse_tags_input, preset_browser, preset_browser_categories, preset_browser_items,
    preset_browser_tags, preset_choices, preset_display_name, preset_panel, redraw_custom_view,
    set_param, setup_vizia_fonts, translate,
    translate_format,
};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicI32, AtomicU8, AtomicU32, Ordering};
use vizia_plug::vizia::prelude::*;
use vizia_plug::widgets::util::ModifiersExt;
use vizia_plug::widgets::{ParamButton, ParamButtonExt, RawParamEvent, zoom_control};
use vizia_plug::{ViziaEditor, ViziaTheming, create_vizia_editor};

pub const EDITOR_WIDTH: u32 = 1120;
pub const EDITOR_HEIGHT: u32 = 934;

#[derive(Clone)]
struct DiscreteSelections {
    mode: Signal<i32>,
    pluck_style: Signal<i32>,
}

#[derive(Debug)]
enum BassUiEvent {
    PreviousPreset,
    NextPreset,
    SetLanguage(Language),
    SelectPreset(String),
    SetName(String),
    SaveAs,
    Rename,
    Overwrite,
    Delete,
    Undo,
    Redo,
    ToggleLearn,
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
        "drive" => params.drive.value(),
        "gain" => params.master_gain.value(),
        "fx_eq_low" => params.fx_eq_low.value(),
        "fx_eq_mid" => params.fx_eq_mid.value(),
        "fx_eq_high" => params.fx_eq_high.value(),
        "fx_reverb_mix" => params.fx_reverb_mix.value(),
        "fx_reverb_decay" => params.fx_reverb_decay.value(),
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
        "drive" => set_param(cx, &params.drive, value),
        "gain" => set_param(cx, &params.master_gain, value),
        "fx_eq_low" => set_param(cx, &params.fx_eq_low, value),
        "fx_eq_mid" => set_param(cx, &params.fx_eq_mid, value),
        "fx_eq_high" => set_param(cx, &params.fx_eq_high, value),
        "fx_reverb_mix" => set_param(cx, &params.fx_reverb_mix, value),
        "fx_reverb_decay" => set_param(cx, &params.fx_reverb_decay, value),
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
        "drive",
        "gain",
        "fx_eq_low",
        "fx_eq_mid",
        "fx_eq_high",
        "fx_reverb_mix",
        "fx_reverb_decay",
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
    discrete: DiscreteSelections,
    gui_tx: crossbeam_channel::Sender<GuiBassEvent>,
    held_qwerty_keys: HashMap<Code, u8>,
    held_string_keys: HashMap<Code, u8>,
    held_nav_keys: HashSet<Code>,
    octave_offset: i8,
    // MIDI Learn state: the arm (mode + pending param id) lives in the
    // shared LearnArm; the poll bookkeeping below is handler-local.
    learn_arm: LearnArm,
    learn_status: Signal<String>,
    last_cc: Arc<LastCc>,
    last_seen_seq: u32,
    last_seen_pending: Option<String>,
}

impl BassUiState {
    fn update_history_state(&self) {
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
            PresetPanelAction::SelectLanguage(language) => {
                cx.emit(BassUiEvent::SetLanguage(*language))
            }
            PresetPanelAction::Select(id) => cx.emit(BassUiEvent::SelectPreset(id.clone())),
            PresetPanelAction::SetName(name) => cx.emit(BassUiEvent::SetName(name.clone())),
            PresetPanelAction::SaveAs => cx.emit(BassUiEvent::SaveAs),
            PresetPanelAction::Rename => cx.emit(BassUiEvent::Rename),
            PresetPanelAction::Overwrite => cx.emit(BassUiEvent::Overwrite),
            PresetPanelAction::Delete => cx.emit(BassUiEvent::Delete),
            PresetPanelAction::Undo => cx.emit(BassUiEvent::Undo),
            PresetPanelAction::Redo => cx.emit(BassUiEvent::Redo),
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
                cx.emit(BassUiEvent::SelectPreset(id.clone()));
                self.browser_open.set(false);
            }
        });

        event.map(|action: &BassUiEvent, _| match action {
            BassUiEvent::ToggleLearn => {
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
            BassUiEvent::RefreshDisplay => {
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
                self.discrete.mode.set(self.params.mode.value());
                self.discrete
                    .pluck_style
                    .set(self.params.pluck_style.value());
                redraw_custom_view(cx, "bass-fretboard-widget");
                redraw_custom_view(cx, "stereo-vu-meter-widget");
                redraw_custom_view(cx, "spectrum-analyzer-widget");
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
                    let display_name = preset_display_name(&preset, self.language.get());
                    self.selected_name.set(display_name.clone());
                    self.name_input.set(display_name);
                    self.browser_tags_input
                        .set(self.manager.read().tags_for(id).join(", "));
                    self.is_user_preset
                        .set(self.manager.read().is_user_preset(id));
                    self.refresh_preset_signals();
                    self.update_history_state();
                }
            }
            BassUiEvent::SetLanguage(language) => {
                let language = *language;
                self.language.set(language);
                self.language_atom
                    .store(language.index(), Ordering::Relaxed);
                if let Some(preset) = self.manager.read().get_preset(&self.selected_id.get()) {
                    let display_name = preset_display_name(preset, language);
                    self.selected_name.set(display_name.clone());
                    self.name_input.set(display_name);
                }
                self.refresh_preset_signals();
                redraw_custom_view(cx, "bass-fretboard-widget");
            }
            BassUiEvent::SetName(name) => self.name_input.set(name.clone()),
            BassUiEvent::SaveAs => {
                let name = self.name_input.get().trim().to_string();
                let name = if name.is_empty() {
                    translate(
                        self.language.get(),
                        "bass.preset.default-name",
                        "Custom Bass",
                    )
                    .to_owned()
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
                    self.browser_tags_input.set(String::new());
                    self.is_user_preset.set(true);
                    self.refresh_preset_signals();
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
                        self.refresh_preset_signals();
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
                    self.refresh_preset_signals();
                    cx.emit(BassUiEvent::SelectPreset("bass_finger_punch".to_string()));
                }
            }
            BassUiEvent::Undo => self.apply_history(cx, false),
            BassUiEvent::Redo => self.apply_history(cx, true),
        });

        event.map(|cancel: &CancelParamGestureEvent, _| {
            let ptr = cancel.param;
            let params = &self.params;
            let id = if ptr == params.mode.as_ptr() {
                "mode"
            } else if ptr == params.pluck_style.as_ptr() {
                "pluck_style"
            } else if ptr == params.five_string.as_ptr() {
                "five_string"
            } else if ptr == params.pickup_position.as_ptr() {
                "pickup_position"
            } else if ptr == params.pluck_position.as_ptr() {
                "pluck_position"
            } else if ptr == params.tone.as_ptr() {
                "tone"
            } else if ptr == params.fret_buzz.as_ptr() {
                "fret_buzz"
            } else if ptr == params.body_mix.as_ptr() {
                "body_mix"
            } else if ptr == params.drive.as_ptr() {
                "drive"
            } else if ptr == params.master_gain.as_ptr() {
                "gain"
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
            } else if ptr == params.drive.as_ptr() {
                ("drive", params.drive.value())
            } else if ptr == params.master_gain.as_ptr() {
                ("gain", params.master_gain.value())
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

        event.map(|window: &WindowEvent, meta| match window {
            WindowEvent::KeyDown(code, _) => {
                if cx.modifiers().command() {
                    if *code == Code::KeyZ && !cx.modifiers().shift() {
                        cx.emit(BassUiEvent::Undo);
                    } else if *code == Code::KeyY || (*code == Code::KeyZ && cx.modifiers().shift())
                    {
                        cx.emit(BassUiEvent::Redo);
                    }
                    meta.consume();
                    return;
                }
                // Octave shifts with Z / X when not holding Command
                if *code == Code::KeyZ || *code == Code::KeyX {
                    if self.held_nav_keys.insert(*code) {
                        if *code == Code::KeyZ && self.octave_offset > -36 {
                            self.octave_offset -= 12;
                        }
                        if *code == Code::KeyX && self.octave_offset < 0 {
                            self.octave_offset += 12;
                        }
                    }
                    meta.consume();
                    return;
                }
                // Number keys 1..=5: pluck bass open strings directly
                // 1 -> G2 (str 4), 2 -> D2 (str 3), 3 -> A1 (str 2), 4 -> E1 (str 1), 5 -> B0 (str 0)
                const BASS_STRING_KEYS: &[(Code, u8)] = &[
                    (Code::Digit1, 4),
                    (Code::Digit2, 3),
                    (Code::Digit3, 2),
                    (Code::Digit4, 1),
                    (Code::Digit5, 0),
                ];
                if let Some((_, string_idx)) = BASS_STRING_KEYS.iter().find(|(k, _)| k == code) {
                    if *string_idx == 0 && !self.params.five_string.value() {
                        meta.consume();
                        return;
                    }
                    if !self.held_string_keys.contains_key(code) {
                        self.held_string_keys.insert(*code, *string_idx);
                        let _ = self.gui_tx.try_send(GuiBassEvent::NoteOn {
                            string_index: *string_idx,
                            fret: 0,
                            velocity: 0.90,
                        });
                    }
                    meta.consume();
                    return;
                }
                // Chromatic note keys: A-K (C4 base, shifted by octave_offset, default -24 for C2)
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
                        let min_note = if self.params.five_string.value() {
                            23
                        } else {
                            28
                        };
                        let midi =
                            (*base as i16 + self.octave_offset as i16).clamp(min_note, 72) as u8;
                        self.held_qwerty_keys.insert(*code, midi);
                        let _ = self.gui_tx.try_send(GuiBassEvent::MidiNoteOn {
                            note: midi,
                            velocity: 0.88,
                        });
                    }
                    meta.consume();
                    return;
                }
            }
            WindowEvent::KeyUp(code, _) => {
                self.held_nav_keys.remove(code);
                if let Some(string_idx) = self.held_string_keys.remove(code) {
                    let _ = self.gui_tx.try_send(GuiBassEvent::NoteOff {
                        string_index: string_idx,
                    });
                    meta.consume();
                }
                if let Some(midi) = self.held_qwerty_keys.remove(code) {
                    let _ = self
                        .gui_tx
                        .try_send(GuiBassEvent::MidiNoteOff { note: midi });
                    meta.consume();
                }
            }
            WindowEvent::FocusOut => {
                self.held_nav_keys.clear();
                for (_, string_idx) in self.held_string_keys.drain() {
                    let _ = self.gui_tx.try_send(GuiBassEvent::NoteOff {
                        string_index: string_idx,
                    });
                }
                for (_, midi) in self.held_qwerty_keys.drain() {
                    let _ = self
                        .gui_tx
                        .try_send(GuiBassEvent::MidiNoteOff { note: midi });
                }
            }
            _ => {}
        });
    }
}

pub fn create_vizia_bass_editor(
    params: Arc<PhysicsBassParams>,
    active_frets: Arc<[std::sync::atomic::AtomicU8; 5]>,
    string_energies: Arc<[std::sync::atomic::AtomicU32; 5]>,
    string_profiles: Arc<[[AtomicI32; 32]; 5]>,
    peak_l: Arc<atomic_float::AtomicF32>,
    peak_r: Arc<atomic_float::AtomicF32>,
    sample_history: Arc<parking_lot::RwLock<Vec<f32>>>,
    sample_rate: f32,
    gui_tx: crossbeam_channel::Sender<GuiBassEvent>,
    language_atom: Arc<AtomicU8>,
    preset_manager: Arc<parking_lot::RwLock<PresetManager>>,
    undo_manager: Arc<parking_lot::RwLock<UndoManager>>,
    last_cc: Arc<LastCc>,
) -> Option<ViziaEditor> {
    let language_initial = Language::from_index(language_atom.load(Ordering::Relaxed));
    let selected_id = "bass_finger_punch".to_string();
    let selected_name = preset_manager
        .read()
        .get_preset(&selected_id)
        .map(|preset| preset_display_name(preset, language_initial))
        .unwrap_or_else(|| "Finger Punch".to_string());

    let editor_state = params.editor_state.clone();
    let zoom_state = editor_state.clone();
    create_vizia_editor(editor_state, ViziaTheming::Custom, move |cx, _| {
        setup_vizia_fonts(cx);
        if let Err(error) = add_base_theme(cx) {
            eprintln!("Failed to load shared Vizia theme: {error:?}");
        }
        let selected_name_signal = Signal::new(selected_name.clone());
        let selected_id_signal = Signal::new(selected_id.clone());
        let preset_choices_signal =
            Signal::new(preset_choices(&preset_manager.read(), language_initial));
        let browser_items_initial = preset_browser_items(&preset_manager.read(), language_initial);
        let browser_open = Signal::new(false);
        let browser_search = Signal::new(String::new());
        let browser_category = Signal::new(None);
        let browser_favorites_only = Signal::new(false);
        let browser_categories = Signal::new(preset_browser_categories(&browser_items_initial));
        let browser_tags = Signal::new(preset_browser_tags(&browser_items_initial));
        let browser_selected_tags = Signal::new(Vec::new());
        let browser_tags_input =
            Signal::new(preset_manager.read().tags_for(&selected_id).join(", "));
        let browser_filtered = Signal::new(browser_items_initial);
        let language = Signal::new(language_initial);
        let name_input = Signal::new(selected_name.clone());
        let is_user_preset = Signal::new(false);
        let can_undo = Signal::new(undo_manager.read().can_undo());
        let can_redo = Signal::new(undo_manager.read().can_redo());
        let discrete = DiscreteSelections {
            mode: Signal::new(params.mode.value()),
            pluck_style: Signal::new(params.pluck_style.value()),
        };
        // MIDI Learn: the ptr -> id table is built from the params Arc,
        // whose allocation never moves, so the pointers stay valid.
        let learn_arm = LearnArm::new(build_ptr_to_id_map(&params));
        let learn_status = Signal::new(String::new());

        BassUiState {
            params: params.clone(),
            manager: preset_manager.clone(),
            undo: undo_manager.clone(),
            selected_name: selected_name_signal.clone(),
            selected_id: selected_id_signal,
            preset_choices: preset_choices_signal.clone(),
            browser_open: browser_open.clone(),
            browser_search: browser_search.clone(),
            browser_category: browser_category.clone(),
            browser_favorites_only: browser_favorites_only.clone(),
            browser_categories: browser_categories.clone(),
            browser_filtered: browser_filtered.clone(),
            browser_tags: browser_tags.clone(),
            browser_selected_tags: browser_selected_tags.clone(),
            browser_tags_input: browser_tags_input.clone(),
            language: language.clone(),
            language_atom: language_atom.clone(),
            name_input: name_input.clone(),
            is_user_preset: is_user_preset.clone(),
            can_undo: can_undo.clone(),
            can_redo: can_redo.clone(),
            suppress_undo: Arc::new(AtomicU32::new(0)),
            discrete: discrete.clone(),
            gui_tx: gui_tx.clone(),
            held_qwerty_keys: HashMap::new(),
            held_string_keys: HashMap::new(),
            held_nav_keys: HashSet::new(),
            octave_offset: -24,
            learn_arm: learn_arm.clone(),
            learn_status: learn_status.clone(),
            last_cc: last_cc.clone(),
            last_seen_seq: last_cc.seq(),
            last_seen_pending: None,
        }
        .build(cx);

        let display_timer = cx.add_timer(std::time::Duration::from_millis(33), None, |cx, _| {
            cx.emit(BassUiEvent::RefreshDisplay)
        });
        cx.start_timer(display_timer);

        let params_ui = params.clone();
        let learn_ui = learn_arm.clone();
        let learn_status_ui = learn_status.clone();
        let language_ui = language.clone();
        let language_view = language.clone();
        let active_frets_ui = active_frets.clone();
        let string_energies_ui = string_energies.clone();
        let string_profiles_ui = string_profiles.clone();
        let gui_tx_ui = gui_tx.clone();
        let selected_name_ui = selected_name_signal;
        let preset_choices_ui = preset_choices_signal;
        let browser_open_ui = browser_open.clone();
        let browser_search_ui = browser_search.clone();
        let browser_category_ui = browser_category.clone();
        let browser_favorites_only_ui = browser_favorites_only.clone();
        let browser_categories_ui = browser_categories.clone();
        let browser_filtered_ui = browser_filtered.clone();
        let browser_tags_ui = browser_tags.clone();
        let browser_selected_tags_ui = browser_selected_tags.clone();
        let browser_tags_input_ui = browser_tags_input.clone();
        let name_input_ui = name_input;
        let is_user_preset_ui = is_user_preset;
        let can_undo_ui = can_undo;
        let can_redo_ui = can_redo;
        let discrete_ui = discrete;
        let language_atom_ui = language_atom.clone();
        let zoom_state_ui = zoom_state.clone();
        let peak_l_ui = peak_l.clone();
        let peak_r_ui = peak_r.clone();
        let sample_history_ui = sample_history.clone();

        Binding::new(cx, language_ui, move |cx| {
            let lang = language_ui.get();
            let params = params_ui.clone();
            let learn = learn_ui.clone();
            let learn_status = learn_status_ui.clone();
            let language_view = language_view.clone();
            let discrete = discrete_ui.clone();
            let active_frets = active_frets_ui.clone();
            let string_energies = string_energies_ui.clone();
            let string_profiles = string_profiles_ui.clone();
            let gui_tx = gui_tx_ui.clone();
            let language_atom = language_atom_ui.clone();
            let browser_open = browser_open_ui.clone();
            let browser_search = browser_search_ui.clone();
            let browser_category = browser_category_ui.clone();
            let browser_favorites_only = browser_favorites_only_ui.clone();
            let browser_categories = browser_categories_ui.clone();
            let browser_filtered = browser_filtered_ui.clone();
            let browser_tags = browser_tags_ui.clone();
            let browser_selected_tags = browser_selected_tags_ui.clone();
            let browser_tags_input = browser_tags_input_ui.clone();
            let browser_current_name = selected_name_ui.clone();
            let zoom_state = zoom_state_ui.clone();
            let theme_signal = zoom_state.theme_signal();
            let theme_state = zoom_state.clone();
            let peak_l = peak_l_ui.clone();
            let peak_r = peak_r_ui.clone();
            let sample_history = sample_history_ui.clone();
            VStack::new(cx, move |cx| {
                HStack::new(cx, |cx| {
                    VStack::new(cx, |cx| {
                        Label::new(cx, translate(lang, "bass.title", "PHYSICS BASS")).class("title");
                        Label::new(cx, translate(lang, "bass.subtitle", "FDTD stiff string · fret contact · finite pickup aperture · body modes")).class("subtitle");
                    })
                    .width(Stretch(1.0));
                    Label::new(cx, translate(lang, "bass.audition", "Click or drag the fretboard below to audition")).class("status");
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
                        preset_label: Some(translate(lang, "bass.preset", "Preset:")),
                        preset_width: 190.0,
                        name_width: 130.0,
                        save_as_width: 64.0,
                        rename_width: 58.0,
                        overwrite_width: 68.0,
                        delete_width: 48.0,
                    },
                    move |cx| {
                        // The audition hint already shows in the header
                        // above; repeating it here pushed the zoom
                        // control off the right edge of the bar.
                        StereoVuMeterWidget::new(cx, peak_l.clone(), peak_r.clone())
                            .width(Pixels(70.0))
                            .height(Pixels(24.0));
                        zoom_control(
                            cx,
                            zoom_state.clone(),
                            translate(lang, "gui.zoom", "Zoom"),
                        );
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
                            cx.emit(BassUiEvent::ToggleLearn);
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

                HStack::new(cx, |cx| {
                    VStack::new(cx, |cx| {
                        Label::new(cx, translate(lang, "bass.instrument", "INSTRUMENT")).class("rack-title");
                        discrete_selector(
                            cx,
                            translate(lang, "bass.model", "Model"),
                            discrete.mode.clone(),
                            vec![
                                (0, translate(lang, "bass.electric", "Electric").to_string()),
                                (1, translate(lang, "bass.acoustic", "Acoustic").to_string()),
                            ],
                            94.0,
                            {
                                let params = params.clone();
                                move |cx, value| set_param(cx, &params.mode, value)
                            },
                        );
                        discrete_selector(
                            cx,
                            translate(lang, "bass.exciter", "Exciter"),
                            discrete.pluck_style.clone(),
                            vec![
                                (0, translate(lang, "bass.finger", "Finger").to_string()),
                                (1, translate(lang, "bass.pick", "Pick").to_string()),
                                (2, translate(lang, "bass.slap", "Slap").to_string()),
                            ],
                            94.0,
                            {
                                let params = params.clone();
                                move |cx, value| set_param(cx, &params.pluck_style, value)
                            },
                        );
                        HStack::new(cx, |cx| {
                            Label::new(cx, translate(lang, "bass.tuning", "Tuning")).class("param-label");
                            ParamButton::new(cx, &params.five_string)
                                .with_label(translate(lang, "bass.five_string", "Five-string B0"));
                        })
                        .horizontal_gap(Pixels(6.0));
                    })
                    .class("rack-box")
                    .width(Stretch(1.0));

                    VStack::new(cx, |cx| {
                        Label::new(cx, translate(lang, "bass.string_pickup", "STRING / PICKUP")).class("rack-title");
                        HStack::new(cx, |cx| {
                            parameter_knob(
                                cx,
                                translate(lang, "bass.pluck_position", "Pluck position"),
                                &params.pluck_position,
                                120.0,
                                translate(lang, "bass.tooltip.pluck_position", "Where the string is plucked, from bridge to neck."), Some(learn.clone()));
                            parameter_knob(
                                cx,
                                translate(lang, "bass.pickup_position", "Pickup position"),
                                &params.pickup_position,
                                120.0,
                                translate(lang, "bass.tooltip.pickup_position", "Pickup position along the string."), Some(learn.clone()));
                        })
                        .horizontal_gap(Pixels(8.0));
                        HStack::new(cx, |cx| {
                            parameter_knob(
                                cx,
                                translate(lang, "bass.tone", "Tone"),
                                &params.tone,
                                120.0,
                                translate(lang, "bass.tooltip.tone", "Tone control: darker to brighter."), Some(learn.clone()));
                        })
                        .horizontal_gap(Pixels(8.0));
                    })
                    .class("rack-box")
                    .width(Stretch(1.0));

                    VStack::new(cx, |cx| {
                        Label::new(cx, translate(lang, "bass.contact_body", "CONTACT / BODY")).class("rack-title");
                        HStack::new(cx, |cx| {
                            parameter_knob(cx, translate(lang, "bass.fret_buzz", "Fret buzz"), &params.fret_buzz, 120.0, translate(lang, "bass.tooltip.fret_buzz", "Fret buzz amount."), Some(learn.clone()));
                            parameter_knob(cx, translate(lang, "bass.body_mix", "Body mix"), &params.body_mix, 120.0, translate(lang, "bass.tooltip.body_mix", "Body resonance mix: how much of the instrument body sound is blended in."), Some(learn.clone()));
                        })
                        .horizontal_gap(Pixels(8.0));
                        Label::new(cx, translate(lang, "bass.slap_note", "Slap shares the FDTD contact state")).class("small-note");
                    })
                    .class("rack-box")
                    .width(Stretch(1.0));

                    VStack::new(cx, |cx| {
                        Label::new(cx, translate(lang, "bass.output", "AMP & OUTPUT")).class("rack-title");
                        HStack::new(cx, |cx| {
                            parameter_knob(cx, translate(lang, "bass.drive", "Drive"), &params.drive, 120.0, translate(lang, "bass.tooltip.drive", "Drive amount: tube/FET saturation."), Some(learn.clone()));
                            parameter_knob(cx, translate(lang, "bass.master_gain", "Master gain"), &params.master_gain, 120.0, translate(lang, "bass.tooltip.master_gain", "Overall output volume."), Some(learn.clone()));
                        })
                        .horizontal_gap(Pixels(8.0));
                        Label::new(cx, translate(lang, "bass.output_note", "Electric: drive & pickup · Acoustic: cavity")).class("small-note");
                    })
                    .class("rack-box")
                    .width(Stretch(1.0));
                })
                .class("rack-row")
                .width(Stretch(1.0))
                .height(Pixels(185.0))
                .horizontal_gap(Pixels(8.0));

                VStack::new(cx, |cx| {
                    Label::new(cx, translate(lang, "fx.rack", "FX")).class("rack-title");
                    HStack::new(cx, |cx| {
                        parameter_knob(cx, translate(lang, "fx.eq_low", "EQ Low:"), &params.fx_eq_low, 120.0, translate(lang, "fx.tooltip.eq_low", "Low shelf EQ at 120 Hz: boosts or cuts bass."), Some(learn.clone()));
                        parameter_knob(cx, translate(lang, "fx.eq_mid", "EQ Mid:"), &params.fx_eq_mid, 120.0, translate(lang, "fx.tooltip.eq_mid", "Peaking EQ at 1 kHz: boosts or cuts the midrange."), Some(learn.clone()));
                        parameter_knob(cx, translate(lang, "fx.eq_high", "EQ High:"), &params.fx_eq_high, 120.0, translate(lang, "fx.tooltip.eq_high", "High shelf EQ at 8 kHz: boosts or cuts treble."), Some(learn.clone()));
                        parameter_knob(cx, translate(lang, "fx.reverb_mix", "Reverb Mix:"), &params.fx_reverb_mix, 120.0, translate(lang, "fx.tooltip.reverb_mix", "Reverb amount: 0 is dry, higher adds hall sound."), Some(learn.clone()));
                        parameter_knob(cx, translate(lang, "fx.reverb_decay", "Reverb Decay:"), &params.fx_reverb_decay, 120.0, translate(lang, "fx.tooltip.reverb_decay", "Reverb length: longer decay means a bigger hall."), Some(learn.clone()));
                    })
                    .horizontal_gap(Pixels(8.0));
                })
                .class("rack-box")
                .width(Stretch(1.0))
                .height(Pixels(110.0));

                Label::new(cx, translate(lang, "bass.fretboard_hint", "Open notes + 24 frets · Keys 1-5 open strings · A-L notes · Z/X octaves")).class("hint-text");
                BassFretboardWidget::new(
                    cx,
                    params,
                    active_frets,
                    string_energies,
                    string_profiles,
                    gui_tx,
                    language_atom,
                )
                .class("instrument-view")
                .width(Stretch(1.0))
                .height(Pixels(300.0))
                .overflow(Overflow::Hidden);
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
            .class("root")
            .toggle_class("theme-light", theme_signal)
            .width(Stretch(1.0))
            .height(Stretch(1.0));

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
