//! Vizia Plug editor for the physical guitar.

use crossbeam_channel::Sender;
use nice_plug::prelude::Param;
use std::sync::Arc;
use std::sync::atomic::{AtomicI32, AtomicU8, AtomicU32, Ordering};
use vizia_plug::vizia::prelude::*;
use vizia_plug::widgets::util::ModifiersExt;
use vizia_plug::widgets::*;
use vizia_plug::{ViziaEditor, ViziaState, ViziaTheming, create_vizia_editor};

use crate::core::{ChordQuality, ChordRoot, ChordShape, StrumDirection, chord_shape};
use crate::gui::fretboard_view::GuitarFretboardWidget;
use crate::nice_plugin::{GuiGuitarEvent, PhysicsGuitarParams};
use physics_presets::{ParamTransition, Preset, PresetManager, UndoManager};
use physics_ui::Language;
use physics_ui::{
    CancelParamGestureEvent, PresetBrowserAction, PresetBrowserItem, PresetBrowserSignals,
    PresetPanelAction, PresetPanelLayout, PresetPanelSignals, SpectrumAnalyzerWidget,
    StereoVuMeterWidget, add_base_theme, discrete_selector as shared_discrete_selector,
    filter_browser_items, map_param_history_event, parameter_knob, preset_browser,
    preset_browser_categories, preset_browser_items, preset_choices, preset_display_name,
    preset_panel, redraw_custom_view, set_param, setup_vizia_fonts, translate,
};
use std::collections::{HashMap, HashSet};

#[derive(Clone)]
struct DiscreteSelections {
    mode: Signal<i32>,
    pluck_style: Signal<i32>,
    pickup_pos: Signal<i32>,
    pickup_type: Signal<i32>,
    cabinet_model: Signal<i32>,
    groove_pattern: Signal<i32>,
}

#[derive(Clone, Copy)]
enum DiscreteParam {
    Mode,
    PluckStyle,
    PickupPosition,
    PickupType,
    CabinetModel,
    GroovePattern,
}

pub const EDITOR_WIDTH: u32 = 1080;
pub const EDITOR_HEIGHT: u32 = 809;

fn knob<P: Param + 'static>(
    cx: &mut Context,
    label: &'static str,
    param: &P,
    tooltip: &'static str,
) {
    parameter_knob(cx, label, param, 84.0, tooltip);
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

/// Recomputes the chord rack's fingering readout ("C  x 3 2 0 1 0")
/// from the currently selected root and quality.
fn refresh_chord_display(
    root: &Signal<i32>,
    quality: &Signal<i32>,
    display: &Signal<String>,
) {
    let r = ChordRoot::from_index(root.get());
    let q = ChordQuality::from_index(quality.get());
    let shape = chord_shape(r, q);
    display.set(format!("{}  {}", ChordShape::name(r, q), shape.fingering_text()));
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
        DiscreteParam::CabinetModel => set_param(cx, &params.cabinet_model, value),
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
    browser_open: Signal<bool>,
    browser_search: Signal<String>,
    browser_category: Signal<Option<String>>,
    browser_favorites_only: Signal<bool>,
    browser_categories: Signal<Vec<String>>,
    browser_filtered: Signal<Vec<PresetBrowserItem>>,
    language: Signal<Language>,
    language_atom: Arc<AtomicU8>,
    name_input: Signal<String>,
    is_user_preset: Signal<bool>,
    can_undo: Signal<bool>,
    can_redo: Signal<bool>,
    suppress_undo: Arc<AtomicU32>,
    discrete: DiscreteSelections,
    gui_tx: Sender<GuiGuitarEvent>,
    held_qwerty_keys: HashMap<Code, u8>,
    held_string_keys: HashMap<Code, u8>,
    held_nav_keys: HashSet<Code>,
    octave_offset: i8,
}

#[derive(Debug)]
enum GuitarUiEvent {
    PreviousPreset,
    NextPreset,
    SetLanguage(Language),
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
        "amp_bass" => params.amp_bass.value(),
        "amp_middle" => params.amp_middle.value(),
        "amp_treble" => params.amp_treble.value(),
        "amp_presence" => params.amp_presence.value(),
        "amp_sag" => params.amp_sag.value(),
        "cab_enabled" => {
            if params.cab_enabled.value() {
                1.0
            } else {
                0.0
            }
        }
        "cabinet_model" => params.cabinet_model.value() as f32,
        "use_ir_cab" => {
            if params.use_ir_cab.value() {
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
        "fx_eq_low" => params.fx_eq_low.value(),
        "fx_eq_mid" => params.fx_eq_mid.value(),
        "fx_eq_high" => params.fx_eq_high.value(),
        "fx_reverb_mix" => params.fx_reverb_mix.value(),
        "fx_reverb_decay" => params.fx_reverb_decay.value(),
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
        "amp_bass" => set_param(cx, &params.amp_bass, value),
        "amp_middle" => set_param(cx, &params.amp_middle, value),
        "amp_treble" => set_param(cx, &params.amp_treble, value),
        "amp_presence" => set_param(cx, &params.amp_presence, value),
        "amp_sag" => set_param(cx, &params.amp_sag, value),
        "cab_enabled" => set_param(cx, &params.cab_enabled, value >= 0.5),
        "cabinet_model" => set_param(cx, &params.cabinet_model, value.round() as i32),
        "use_ir_cab" => set_param(cx, &params.use_ir_cab, value >= 0.5),
        "strum_speed" => set_param(cx, &params.strum_speed, value),
        "fret_buzz" => set_param(cx, &params.fret_buzz, value),
        "finger_squeak" => set_param(cx, &params.finger_squeak, value),
        "groove_pattern" => set_param(cx, &params.groove_pattern, value.round() as i32),
        "groove_bpm" => set_param(cx, &params.groove_bpm, value),
        "gain" => set_param(cx, &params.master_gain, value),
        "fx_eq_low" => set_param(cx, &params.fx_eq_low, value),
        "fx_eq_mid" => set_param(cx, &params.fx_eq_mid, value),
        "fx_eq_high" => set_param(cx, &params.fx_eq_high, value),
        "fx_reverb_mix" => set_param(cx, &params.fx_reverb_mix, value),
        "fx_reverb_decay" => set_param(cx, &params.fx_reverb_decay, value),
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
        "amp_bass",
        "amp_middle",
        "amp_treble",
        "amp_presence",
        "amp_sag",
        "cab_enabled",
        "cabinet_model",
        "use_ir_cab",
        "strum_speed",
        "fret_buzz",
        "finger_squeak",
        "groove_pattern",
        "groove_bpm",
        "gain",
        "fx_eq_low",
        "fx_eq_mid",
        "fx_eq_high",
        "fx_reverb_mix",
        "fx_reverb_decay",
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

    /// Refresh the preset dropdown rows and the browser rows/categories
    /// after any preset list or filter change.
    fn refresh_preset_signals(&self) {
        let manager = self.manager.read();
        let lang = self.language.get();
        self.preset_choices.set(preset_choices(&manager, lang));
        let items = preset_browser_items(&manager, lang);
        self.browser_categories
            .set(preset_browser_categories(&items));
        self.browser_filtered.set(filter_browser_items(
            &items,
            &self.browser_search.get(),
            self.browser_category.get().as_deref(),
            self.browser_favorites_only.get(),
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
            PresetPanelAction::SelectLanguage(language) => {
                cx.emit(GuitarUiEvent::SetLanguage(*language))
            }
            PresetPanelAction::Select(id) => cx.emit(GuitarUiEvent::SelectPreset(id.clone())),
            PresetPanelAction::SetName(name) => cx.emit(GuitarUiEvent::SetName(name.clone())),
            PresetPanelAction::SaveAs => cx.emit(GuitarUiEvent::SaveAs),
            PresetPanelAction::Rename => cx.emit(GuitarUiEvent::Rename),
            PresetPanelAction::Overwrite => cx.emit(GuitarUiEvent::Overwrite),
            PresetPanelAction::Delete => cx.emit(GuitarUiEvent::Delete),
            PresetPanelAction::Undo => cx.emit(GuitarUiEvent::Undo),
            PresetPanelAction::Redo => cx.emit(GuitarUiEvent::Redo),
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
            PresetBrowserAction::Select(id) => {
                cx.emit(GuitarUiEvent::SelectPreset(id.clone()));
                self.browser_open.set(false);
            }
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
                    .cabinet_model
                    .set(self.params.cabinet_model.value());
                self.discrete
                    .groove_pattern
                    .set(self.params.groove_pattern.value());
                redraw_custom_view(cx, "guitar-fretboard-widget");
                redraw_custom_view(cx, "stereo-vu-meter-widget");
                redraw_custom_view(cx, "spectrum-analyzer-widget");
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
                    let display_name = preset_display_name(&preset, self.language.get());
                    self.selected_name.set(display_name.clone());
                    self.name_input.set(display_name);
                    self.is_user_preset
                        .set(self.manager.read().is_user_preset(id));
                    self.refresh_preset_signals();
                    self.update_history_state();
                }
            }
            GuitarUiEvent::SetLanguage(lang) => {
                let lang = *lang;
                self.language.set(lang);
                self.language_atom.store(lang.index(), Ordering::Relaxed);
                self.refresh_preset_signals();
                if let Some(preset) = self.manager.read().get_preset(&self.selected_id.get()) {
                    let display = preset_display_name(preset, lang);
                    self.selected_name.set(display.clone());
                    self.name_input.set(display);
                }
                redraw_custom_view(cx, "guitar-fretboard-widget");
            }
            GuitarUiEvent::SetName(name) => self.name_input.set(name.clone()),
            GuitarUiEvent::SaveAs => {
                let name = self.name_input.get().trim().to_string();
                let name = if name.is_empty() {
                    translate(
                        self.language.get(),
                        "guitar.preset.default-name",
                        "Custom Guitar",
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
                    "guitar",
                    guitar_snapshot(&self.params),
                );
                if self.manager.write().save_user_preset(preset).is_ok() {
                    self.selected_id.set(id);
                    self.selected_name.set(name.clone());
                    self.name_input.set(name);
                    self.is_user_preset.set(true);
                    self.refresh_preset_signals();
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
                        self.refresh_preset_signals();
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
                    self.refresh_preset_signals();
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
            } else if ptr == params.amp_bass.as_ptr() {
                "amp_bass"
            } else if ptr == params.amp_middle.as_ptr() {
                "amp_middle"
            } else if ptr == params.amp_treble.as_ptr() {
                "amp_treble"
            } else if ptr == params.amp_presence.as_ptr() {
                "amp_presence"
            } else if ptr == params.amp_sag.as_ptr() {
                "amp_sag"
            } else if ptr == params.cab_enabled.as_ptr() {
                "cab_enabled"
            } else if ptr == params.cabinet_model.as_ptr() {
                "cabinet_model"
            } else if ptr == params.use_ir_cab.as_ptr() {
                "use_ir_cab"
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
            } else if ptr == params.amp_bass.as_ptr() {
                ("amp_bass", params.amp_bass.value())
            } else if ptr == params.amp_middle.as_ptr() {
                ("amp_middle", params.amp_middle.value())
            } else if ptr == params.amp_treble.as_ptr() {
                ("amp_treble", params.amp_treble.value())
            } else if ptr == params.amp_presence.as_ptr() {
                ("amp_presence", params.amp_presence.value())
            } else if ptr == params.amp_sag.as_ptr() {
                ("amp_sag", params.amp_sag.value())
            } else if ptr == params.cab_enabled.as_ptr() {
                (
                    "cab_enabled",
                    if params.cab_enabled.value() { 1.0 } else { 0.0 },
                )
            } else if ptr == params.cabinet_model.as_ptr() {
                ("cabinet_model", params.cabinet_model.value() as f32)
            } else if ptr == params.use_ir_cab.as_ptr() {
                (
                    "use_ir_cab",
                    if params.use_ir_cab.value() { 1.0 } else { 0.0 },
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
                        cx.emit(GuitarUiEvent::Undo);
                    } else if *code == Code::KeyY || (*code == Code::KeyZ && cx.modifiers().shift())
                    {
                        cx.emit(GuitarUiEvent::Redo);
                    }
                    meta.consume();
                    return;
                }
                // Octave shifts with Z / X when not holding Command
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
                // Number keys 1..=6: pluck open guitar strings 1..=6 directly
                const STRING_KEYS: &[(Code, u8)] = &[
                    (Code::Digit1, 1),
                    (Code::Digit2, 2),
                    (Code::Digit3, 3),
                    (Code::Digit4, 4),
                    (Code::Digit5, 5),
                    (Code::Digit6, 6),
                ];
                if let Some((_, string_idx)) = STRING_KEYS.iter().find(|(k, _)| k == code) {
                    if !self.held_string_keys.contains_key(code) {
                        self.held_string_keys.insert(*code, *string_idx);
                        let _ = self.gui_tx.send(GuiGuitarEvent::NoteOn {
                            string_index: *string_idx,
                            fret: 0,
                            velocity: 0.88,
                        });
                    }
                    meta.consume();
                    return;
                }
                // Chromatic note keys: A-K (C4 base, shifted by octave_offset)
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
                        let midi = (*base as i16 + self.octave_offset as i16).clamp(40, 88) as u8;
                        self.held_qwerty_keys.insert(*code, midi);
                        let _ = self.gui_tx.send(GuiGuitarEvent::MidiNoteOn {
                            note: midi,
                            velocity: 0.85,
                        });
                    }
                    meta.consume();
                }

            }
            WindowEvent::KeyUp(code, _) => {
                self.held_nav_keys.remove(code);
                if let Some(string_idx) = self.held_string_keys.remove(code) {
                    let _ = self.gui_tx.send(GuiGuitarEvent::NoteOff {
                        string_index: string_idx,
                        fret: 0,
                    });
                    meta.consume();
                }
                if let Some(midi) = self.held_qwerty_keys.remove(code) {
                    let _ = self.gui_tx.send(GuiGuitarEvent::MidiNoteOff { note: midi });
                    meta.consume();
                }
            }
            WindowEvent::FocusOut => {
                self.held_nav_keys.clear();
                for (_, string_idx) in self.held_string_keys.drain() {
                    let _ = self.gui_tx.send(GuiGuitarEvent::NoteOff {
                        string_index: string_idx,
                        fret: 0,
                    });
                }
                for (_, midi) in self.held_qwerty_keys.drain() {
                    let _ = self.gui_tx.send(GuiGuitarEvent::MidiNoteOff { note: midi });
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
pub fn create_vizia_guitar_editor(
    params: Arc<PhysicsGuitarParams>,
    active_frets_shared: Arc<[AtomicU8; 6]>,
    string_energies_shared: Arc<[AtomicU32; 6]>,
    string_profiles_shared: Arc<[[AtomicI32; 16]; 6]>,
    peak_l: Arc<atomic_float::AtomicF32>,
    peak_r: Arc<atomic_float::AtomicF32>,
    sample_history: Arc<parking_lot::RwLock<Vec<f32>>>,
    sample_rate: f32,
    language_atom: Arc<AtomicU8>,
    gui_tx: Sender<GuiGuitarEvent>,
    preset_manager: Arc<parking_lot::RwLock<PresetManager>>,
    undo_manager: Arc<parking_lot::RwLock<UndoManager>>,
    editor_state: Arc<ViziaState>,
) -> Option<ViziaEditor> {
    let lang = Language::from_index(language_atom.load(Ordering::Relaxed));
    let selected_name = preset_manager
        .read()
        .get_preset("strat_clean_chime")
        .map(|preset| preset_display_name(preset, lang))
        .unwrap_or_else(|| "Strat Clean Chime".to_string());
    let zoom_state = editor_state.clone();
    create_vizia_editor(editor_state, ViziaTheming::Custom, move |cx, _| {
        // Signals must be created while Vizia is building this editor so their
        // scope remains alive for the lifetime of the view tree.
        let preset_name = Signal::new(selected_name.clone());
        let preset_id = Signal::new("strat_clean_chime".to_string());
        let preset_choices = Signal::new(preset_choices(&preset_manager.read(), lang));
        let browser_items_initial = preset_browser_items(&preset_manager.read(), lang);
        let browser_open = Signal::new(false);
        let browser_search = Signal::new(String::new());
        let browser_category = Signal::new(None);
        let browser_favorites_only = Signal::new(false);
        let browser_categories = Signal::new(preset_browser_categories(&browser_items_initial));
        let browser_filtered = Signal::new(browser_items_initial);
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
            cabinet_model: Signal::new(params.cabinet_model.value()),
            groove_pattern: Signal::new(params.groove_pattern.value()),
        };
        // Chord library state: GUI-only, not a plugin parameter and not
        // part of presets. It lives outside the language Binding below so
        // switching languages does not reset the selection.
        let chord_root = Signal::new(0);
        let chord_quality = Signal::new(0);
        let chord_direction = Signal::new(0);
        let chord_display = Signal::new(String::new());
        refresh_chord_display(&chord_root, &chord_quality, &chord_display);
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
            browser_open: browser_open.clone(),
            browser_search: browser_search.clone(),
            browser_category: browser_category.clone(),
            browser_favorites_only: browser_favorites_only.clone(),
            browser_categories: browser_categories.clone(),
            browser_filtered: browser_filtered.clone(),
            language,
            language_atom: language_atom.clone(),
            name_input: name_input.clone(),
            is_user_preset,
            can_undo,
            can_redo,
            suppress_undo,
            discrete: discrete.clone(),
            gui_tx: gui_tx.clone(),
            held_qwerty_keys: HashMap::new(),
            held_string_keys: HashMap::new(),
            held_nav_keys: HashSet::new(),
            octave_offset: 0,
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
        let string_profiles_ui = string_profiles_shared.clone();
        let name_input_ui = name_input.clone();
        let preset_name_ui = preset_name.clone();
        let preset_choices_ui = preset_choices.clone();
        let browser_open_ui = browser_open.clone();
        let browser_search_ui = browser_search.clone();
        let browser_category_ui = browser_category.clone();
        let browser_favorites_only_ui = browser_favorites_only.clone();
        let browser_categories_ui = browser_categories.clone();
        let browser_filtered_ui = browser_filtered.clone();
        let language_view_ui = language_view.clone();
        let is_user_preset_controls = is_user_preset_ui.clone();
        let can_undo_controls = can_undo_ui.clone();
        let can_redo_controls = can_redo_ui.clone();
        let discrete_controls = discrete.clone();
        let chord_root_ui = chord_root.clone();
        let chord_quality_ui = chord_quality.clone();
        let chord_direction_ui = chord_direction.clone();
        let chord_display_ui = chord_display.clone();
        let zoom_state_ui = zoom_state.clone();
        let peak_l_ui = peak_l.clone();
        let peak_r_ui = peak_r.clone();
        let sample_history_ui = sample_history.clone();

        Binding::new(cx, language_ui, move |cx| {
            let lang = language_ui.get();
            let params = params_ui.clone();
            let gui_tx = gui_tx_ui.clone();
            let active_frets_shared = active_frets_ui.clone();
            let string_energies_shared = string_energies_ui.clone();
            let string_profiles_shared = string_profiles_ui.clone();
            let initial_preset_name = preset_name_ui.clone();
            let preset_choices = preset_choices_ui.clone();
            let browser_open = browser_open_ui.clone();
            let browser_search = browser_search_ui.clone();
            let browser_category = browser_category_ui.clone();
            let browser_favorites_only = browser_favorites_only_ui.clone();
            let browser_categories = browser_categories_ui.clone();
            let browser_filtered = browser_filtered_ui.clone();
            let name_input = name_input_ui.clone();
            let language_view = language_view_ui.clone();
            let is_user_preset = is_user_preset_controls.clone();
            let can_undo = can_undo_controls.clone();
            let can_redo = can_redo_controls.clone();
            let discrete = discrete_controls.clone();
            let chord_root = chord_root_ui.clone();
            let chord_quality = chord_quality_ui.clone();
            let chord_direction = chord_direction_ui.clone();
            let chord_display = chord_display_ui.clone();
            let zoom_state = zoom_state_ui.clone();
            let peak_l = peak_l_ui.clone();
            let peak_r = peak_r_ui.clone();
            let sample_history = sample_history_ui.clone();
            VStack::new(cx, move |cx| {
                let mode_options = vec![
                    (
                        0,
                        translate(lang, "guitar.mode_electric", "Electric").to_string(),
                    ),
                    (
                        1,
                        translate(lang, "guitar.mode_acoustic", "Acoustic").to_string(),
                    ),
                ];
                let pluck_options = vec![
                    (
                        0,
                        translate(lang, "guitar.pluck_plectrum", "Plectrum").to_string(),
                    ),
                    (
                        1,
                        translate(lang, "guitar.pluck_finger", "Finger").to_string(),
                    ),
                ];
                let pickup_position_options = vec![
                    (
                        0,
                        translate(lang, "guitar.pickup_bridge", "Bridge").to_string(),
                    ),
                    (1, translate(lang, "guitar.pickup_mid", "Mid").to_string()),
                    (2, translate(lang, "guitar.pickup_neck", "Neck").to_string()),
                    (
                        3,
                        translate(lang, "guitar.pickup.bridge-neck", "Bridge + Neck").to_string(),
                    ),
                    (
                        4,
                        translate(lang, "guitar.pickup.bridge-mid", "Bridge + Mid").to_string(),
                    ),
                ];
                let pickup_type_options = vec![
                    (
                        0,
                        translate(lang, "guitar.pickup_single", "Single").to_string(),
                    ),
                    (
                        1,
                        translate(lang, "guitar.pickup_humbucker", "Humbucker").to_string(),
                    ),
                ];
                let cabinet_options = vec![
                    (
                        0,
                        translate(lang, "guitar.cab.vintage30", "Vintage 30 4x12").to_string(),
                    ),
                    (
                        1,
                        translate(lang, "guitar.cab.twin_reverb", "Twin Reverb 2x12").to_string(),
                    ),
                    (
                        2,
                        translate(lang, "guitar.cab.greenback", "Greenback 4x12").to_string(),
                    ),
                ];
                let groove_options = (0..=4)
                    .map(|index| {
                        (
                            index,
                            match index {
                                1 => {
                                    translate(lang, "guitar.groove.1", "Folk 4/4 Basic").to_owned()
                                }
                                2 => {
                                    translate(lang, "guitar.groove.2", "Ballad 6/8 Arp").to_owned()
                                }
                                3 => {
                                    translate(lang, "guitar.groove.3", "Funk 16th Mute").to_owned()
                                }
                                4 => translate(lang, "guitar.groove.4", "Rock 8th Chug").to_owned(),
                                _ => translate(lang, "guitar.groove.0", "Off (Manual)").to_owned(),
                            },
                        )
                    })
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
                        preset_label: Some(translate(lang, "guitar.preset_label", "Preset:")),
                        preset_width: 180.0,
                        name_width: 170.0,
                        save_as_width: 64.0,
                        rename_width: 58.0,
                        overwrite_width: 68.0,
                        delete_width: 48.0,
                    },
                    move |cx| {
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

                HStack::new(cx, |cx| {
                    // Rack 1: INSTRUMENT
                    VStack::new(cx, |cx| {
                        Label::new(cx, translate(lang, "guitar.rack_instrument", "INSTRUMENT"))
                            .class("rack-title");
                        discrete_selector(
                            cx,
                            translate(lang, "guitar.mode_param_label", "Mode:"),
                            discrete.mode.clone(),
                            mode_options.clone(),
                            params.clone(),
                            DiscreteParam::Mode,
                        );
                        discrete_selector(
                            cx,
                            translate(lang, "guitar.pluck_style_label", "Style:"),
                            discrete.pluck_style.clone(),
                            pluck_options.clone(),
                            params.clone(),
                            DiscreteParam::PluckStyle,
                        );
                        discrete_selector(
                            cx,
                            translate(lang, "guitar.pickup_pos_label", "Pickup:"),
                            discrete.pickup_pos.clone(),
                            pickup_position_options.clone(),
                            params.clone(),
                            DiscreteParam::PickupPosition,
                        );
                        discrete_selector(
                            cx,
                            translate(lang, "guitar.pickup_type_label", "Coil:"),
                            discrete.pickup_type.clone(),
                            pickup_type_options.clone(),
                            params.clone(),
                            DiscreteParam::PickupType,
                        );
                    })
                    .class("rack-box")
                    .width(Stretch(1.0));

                    // Rack 2: STOMPBOX & EXPRESSION
                    VStack::new(cx, |cx| {
                        Label::new(
                            cx,
                            translate(lang, "guitar.rack_stompbox", "STOMPBOX & EXPRESSION"),
                        )
                        .class("rack-title");
                        HStack::new(cx, |cx| {
                            knob(
                                cx,
                                translate(lang, "guitar.tone_label", "Tone:"),
                                &params.tone,
                                translate(lang, "guitar.tooltip.tone", "Passive tone control: darker to brighter."),
                            );
                            knob(
                                cx,
                                translate(lang, "guitar.palm_mute_label", "PalmMute:"),
                                &params.palm_mute,
                                translate(lang, "guitar.tooltip.palm_mute", "Palm muting depth on picked notes."),
                            );
                            knob(
                                cx,
                                translate(lang, "guitar.pluck_pos_label", "PluckPos:"),
                                &params.pluck_pos,
                                translate(lang, "guitar.tooltip.pluck_pos", "Where the string is plucked, from bridge to neck."),
                            );
                        })
                        .horizontal_gap(Pixels(6.0));
                        HStack::new(cx, |cx| {
                            knob(
                                cx,
                                translate(lang, "guitar.strum_speed_label", "Speed:"),
                                &params.strum_speed,
                                translate(lang, "guitar.tooltip.strum_speed", "Speed of the strum across the strings."),
                            );
                            knob(
                                cx,
                                translate(lang, "guitar.fret_buzz_label", "Buzz:"),
                                &params.fret_buzz,
                                translate(lang, "guitar.tooltip.fret_buzz", "Fret buzz amount."),
                            );
                            knob(
                                cx,
                                translate(lang, "guitar.finger_squeak_label", "Squeak:"),
                                &params.finger_squeak,
                                translate(lang, "guitar.tooltip.finger_squeak", "Finger squeak noise when shifting positions."),
                            );
                        })
                        .horizontal_gap(Pixels(6.0));
                    })
                    .class("rack-box")
                    .width(Stretch(1.0));

                    // Rack 3: VINTAGE AMP HEAD
                    VStack::new(cx, |cx| {
                        Label::new(cx, translate(lang, "guitar.rack_amp", "VINTAGE AMP HEAD"))
                            .class("rack-title");
                        HStack::new(cx, |cx| {
                            knob(
                                cx,
                                translate(lang, "guitar.amp_drive_label", "Drive:"),
                                &params.amp_drive,
                                translate(lang, "guitar.tooltip.amp_drive", "Amp overdrive amount."),
                            );
                            knob(
                                cx,
                                translate(lang, "guitar.amp_bass_label", "Bass:"),
                                &params.amp_bass,
                                translate(lang, "guitar.tooltip.amp_bass", "Amp bass EQ."),
                            );
                            knob(
                                cx,
                                translate(lang, "guitar.amp_middle_label", "Middle:"),
                                &params.amp_middle,
                                translate(lang, "guitar.tooltip.amp_middle", "Amp middle EQ."),
                            );
                        })
                        .horizontal_gap(Pixels(6.0));
                        HStack::new(cx, |cx| {
                            knob(
                                cx,
                                translate(lang, "guitar.amp_treble_label", "Treble:"),
                                &params.amp_treble,
                                translate(lang, "guitar.tooltip.amp_treble", "Amp treble EQ."),
                            );
                            knob(
                                cx,
                                translate(lang, "guitar.amp_presence_label", "Presence:"),
                                &params.amp_presence,
                                translate(lang, "guitar.tooltip.amp_presence", "Amp presence: high-frequency air and bite."),
                            );
                            knob(
                                cx,
                                translate(lang, "guitar.amp_sag_label", "SAG:"),
                                &params.amp_sag,
                                translate(lang, "guitar.tooltip.amp_sag", "Power-supply sag: how much the amp compresses under load."),
                            );
                        })
                        .horizontal_gap(Pixels(6.0));
                    })
                    .class("rack-box")
                    .width(Stretch(1.0));

                    // Rack 4: CABINET & MASTER
                    VStack::new(cx, |cx| {
                        Label::new(
                            cx,
                            translate(lang, "guitar.rack_cab_master", "CABINET & MASTER"),
                        )
                        .class("rack-title");
                        discrete_selector(
                            cx,
                            translate(lang, "guitar.cabinet_model_label", "Cabinet:"),
                            discrete.cabinet_model.clone(),
                            cabinet_options,
                            params.clone(),
                            DiscreteParam::CabinetModel,
                        );
                        HStack::new(cx, |cx| {
                            ParamButton::new(cx, &params.cab_enabled).with_label(translate(
                                lang,
                                "guitar.cab_enabled",
                                "12\" Cab",
                            ));
                            ParamButton::new(cx, &params.use_ir_cab).with_label(translate(
                                lang,
                                "guitar.use_ir_cab",
                                "UPOLS IR",
                            ));
                        })
                        .horizontal_gap(Pixels(4.0));
                        discrete_selector(
                            cx,
                            translate(lang, "guitar.groove_pattern_label", "Pattern:"),
                            discrete.groove_pattern.clone(),
                            groove_options,
                            params.clone(),
                            DiscreteParam::GroovePattern,
                        );
                        HStack::new(cx, |cx| {
                            knob(
                                cx,
                                translate(lang, "guitar.bpm_label", "BPM:"),
                                &params.groove_bpm,
                                translate(lang, "guitar.tooltip.bpm", "Tempo for the groove pattern player."),
                            );
                            knob(
                                cx,
                                translate(lang, "guitar.master_gain_label", "Volume:"),
                                &params.master_gain,
                                translate(lang, "guitar.tooltip.master_gain", "Overall output volume."),
                            );
                        })
                        .horizontal_gap(Pixels(6.0));
                    })
                    .class("rack-box")
                    .width(Stretch(1.0));
                })
                .height(Pixels(200.0))
                .horizontal_gap(Pixels(8.0));

                VStack::new(cx, |cx| {
                    Label::new(cx, translate(lang, "fx.rack", "FX")).class("rack-title");
                    HStack::new(cx, |cx| {
                        knob(
                            cx,
                            translate(lang, "fx.eq_low", "EQ Low:"),
                            &params.fx_eq_low,
                            translate(lang, "fx.tooltip.eq_low", "Low shelf EQ at 120 Hz: boosts or cuts bass."),
                        );
                        knob(
                            cx,
                            translate(lang, "fx.eq_mid", "EQ Mid:"),
                            &params.fx_eq_mid,
                            translate(lang, "fx.tooltip.eq_mid", "Peaking EQ at 1 kHz: boosts or cuts the midrange."),
                        );
                        knob(
                            cx,
                            translate(lang, "fx.eq_high", "EQ High:"),
                            &params.fx_eq_high,
                            translate(lang, "fx.tooltip.eq_high", "High shelf EQ at 8 kHz: boosts or cuts treble."),
                        );
                        knob(
                            cx,
                            translate(lang, "fx.reverb_mix", "Reverb Mix:"),
                            &params.fx_reverb_mix,
                            translate(lang, "fx.tooltip.reverb_mix", "Reverb amount: 0 is dry, higher adds hall sound."),
                        );
                        knob(
                            cx,
                            translate(lang, "fx.reverb_decay", "Reverb Decay:"),
                            &params.fx_reverb_decay,
                            translate(lang, "fx.tooltip.reverb_decay", "Reverb length: longer decay means a bigger hall."),
                        );
                    })
                    .horizontal_gap(Pixels(6.0));
                })
                .class("rack-box")
                .width(Stretch(1.0))
                .height(Pixels(110.0));

                // Chord library: pick a chord, see its fingering, strum it.
                // Selection is GUI-only state; only the Strum button sends
                // an event to the audio thread.
                VStack::new(cx, |cx| {
                    Label::new(cx, translate(lang, "guitar.chord.rack", "CHORDS"))
                        .class("rack-title");
                    HStack::new(cx, |cx| {
                        let root_sig = chord_root.clone();
                        let quality_sig = chord_quality.clone();
                        let display_sig = chord_display.clone();
                        shared_discrete_selector(
                            cx,
                            translate(lang, "guitar.chord.root_label", "Root:"),
                            chord_root.clone(),
                            ChordRoot::ALL
                                .iter()
                                .enumerate()
                                .map(|(i, r)| (i as i32, r.name().to_string()))
                                .collect(),
                            56.0,
                            move |_cx, value| {
                                root_sig.set(value);
                                refresh_chord_display(&root_sig, &quality_sig, &display_sig);
                            },
                        );
                        let root_sig = chord_root.clone();
                        let quality_sig = chord_quality.clone();
                        let display_sig = chord_display.clone();
                        shared_discrete_selector(
                            cx,
                            translate(lang, "guitar.chord.quality_label", "Chord:"),
                            chord_quality.clone(),
                            vec![
                                (
                                    0,
                                    translate(lang, "guitar.chord.quality.major", "Major")
                                        .to_string(),
                                ),
                                (
                                    1,
                                    translate(lang, "guitar.chord.quality.minor", "Minor")
                                        .to_string(),
                                ),
                                (
                                    2,
                                    translate(lang, "guitar.chord.quality.dom7", "7").to_string(),
                                ),
                                (
                                    3,
                                    translate(lang, "guitar.chord.quality.maj7", "Maj7")
                                        .to_string(),
                                ),
                                (
                                    4,
                                    translate(lang, "guitar.chord.quality.min7", "Min7")
                                        .to_string(),
                                ),
                            ],
                            56.0,
                            move |_cx, value| {
                                quality_sig.set(value);
                                refresh_chord_display(&root_sig, &quality_sig, &display_sig);
                            },
                        );
                        let direction_sig = chord_direction.clone();
                        shared_discrete_selector(
                            cx,
                            translate(lang, "guitar.chord.direction_label", "Strum:"),
                            chord_direction.clone(),
                            vec![
                                (
                                    0,
                                    translate(lang, "guitar.chord.direction.auto", "Auto")
                                        .to_string(),
                                ),
                                (
                                    1,
                                    translate(lang, "guitar.chord.direction.down", "Down")
                                        .to_string(),
                                ),
                                (
                                    2,
                                    translate(lang, "guitar.chord.direction.up", "Up").to_string(),
                                ),
                            ],
                            56.0,
                            move |_cx, value| {
                                direction_sig.set(value);
                            },
                        );
                        let strum_root = chord_root.clone();
                        let strum_quality = chord_quality.clone();
                        let strum_direction = chord_direction.clone();
                        let strum_tx = gui_tx.clone();
                        Button::new(cx, |cx| {
                            Label::new(cx, translate(lang, "guitar.chord.strum", "Strum"))
                        })
                        .class("btn-action")
                        .on_press(move |_cx| {
                            let shape = chord_shape(
                                ChordRoot::from_index(strum_root.get()),
                                ChordQuality::from_index(strum_quality.get()),
                            );
                            let direction = match strum_direction.get() {
                                1 => StrumDirection::Down,
                                2 => StrumDirection::Up,
                                _ => StrumDirection::Auto,
                            };
                            let _ = strum_tx.send(GuiGuitarEvent::StrumChord {
                                frets: shape.frets,
                                velocity: 0.9,
                                direction,
                            });
                        })
                        .width(Pixels(64.0));
                        Label::new(cx, chord_display.clone()).width(Stretch(1.0));
                    })
                    .horizontal_gap(Pixels(6.0));
                })
                .class("rack-box")
                .width(Stretch(1.0))
                .height(Pixels(64.0));

                GuitarFretboardWidget::new(
                    cx,
                    gui_tx.clone(),
                    active_frets_shared.clone(),
                    string_energies_shared.clone(),
                    string_profiles_shared.clone(),
                )
                .height(Pixels(210.0))
                .width(Stretch(1.0));
                Label::new(
                    cx,
                    translate(
                        lang,
                        "guitar.fretboard_hint",
                        "Interactive 6-String Fretboard (Click/Drag strums · Keys 1-6 open strings · A-L notes · Z/X octaves):",
                    ),
                )
                .class("hint-text");
                HStack::new(cx, |cx| {
                    Label::new(cx, translate(lang, "gui.spectrum", "Spectrum")).class("hint-text");
                    SpectrumAnalyzerWidget::new(cx, sample_history.clone(), sample_rate)
                        .width(Stretch(1.0))
                        .height(Pixels(56.0));
                })
                .height(Pixels(56.0))
                .horizontal_gap(Pixels(8.0));
            });

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
                },
            );
        });
    })
}
