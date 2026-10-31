//! Native CLAP plugin and standalone synthesizer built with nice-plug and Vizia for Physics Guitar.

use atomic_float::AtomicF32;
use nice_plug::prelude::*;
use physics_dsp::{
    FxChain, GUITAR_BEND_RANGE_SEMITONES, SOSTENUTO_CC, bend_semitones, cc_pedal_down,
    sanitize_floating_point_environment,
};
use physics_presets::{PresetManager, UndoManager, guitar_factory_presets};
use physics_ui::{LastCc, apply_midi_cc, build_param_map};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicI32, AtomicU8, AtomicU32, Ordering};
use vizia_plug::ViziaState;

use crate::core::groove::GroovePattern;
use crate::core::pickup::{PickupSelector, PickupType};
use crate::core::pluck::PluckStyle;
use crate::engine::{GuitarEngine, GuitarInstrumentMode};
use crate::gui::{Language, create_vizia_guitar_editor, default_vizia_state};
use crate::params::GuitarStringSetType;

#[derive(Params)]
pub struct PhysicsGuitarParams {
    #[persist = "editor-state-v10"]
    pub editor_state: Arc<ViziaState>,

    /// Instrument Mode: 0 = Electric, 1 = Acoustic
    #[id = "mode"]
    pub mode: IntParam,

    /// Pluck Exciter Style: 0 = Plectrum / Pick, 1 = Finger Flesh
    #[id = "pluck_style"]
    pub pluck_style: IntParam,

    /// Electric Guitar Pickup Position: 0 = Bridge, 1 = Middle, 2 = Neck, 3 = Bridge+Neck, 4 = Bridge+Middle
    #[id = "pickup_pos"]
    pub pickup_pos: IntParam,

    /// Electric Guitar Pickup Type: 0 = Single-Coil, 1 = Humbucker
    #[id = "pickup_type"]
    pub pickup_type: IntParam,

    /// Passive RLC Tone knob [0.0 ~ 1.0] (0% = Dark Jazz, 100% = Open chime)
    #[id = "tone"]
    pub tone: FloatParam,

    /// Palm Mute Depth [0.0 ~ 1.0] (0% = Open ring, 100% = Tight chug)
    #[id = "palmmute"]
    pub palm_mute: FloatParam,

    /// Pluck Position along string [0.05 ~ 0.35] (Sul Ponticello to Sul Tasto)
    #[id = "pluckpos"]
    pub pluck_pos: FloatParam,

    /// 12AX7 Tube Preamp Drive [0.0 ~ 1.0] (Clean to saturated overdrive)
    #[id = "amp_drive"]
    pub amp_drive: FloatParam,

    /// Amp Tone Stack Bass [0.0 ~ 1.0]
    #[id = "amp_bass"]
    pub amp_bass: FloatParam,

    /// Amp Tone Stack Middle [0.0 ~ 1.0]
    #[id = "amp_middle"]
    pub amp_middle: FloatParam,

    /// Amp Tone Stack Treble [0.0 ~ 1.0]
    #[id = "amp_treble"]
    pub amp_treble: FloatParam,

    /// Amp Tone Stack Presence [0.0 ~ 1.0]
    #[id = "amp_presence"]
    pub amp_presence: FloatParam,

    /// Power Amp SAG Compression [0.0 ~ 1.0]
    #[id = "amp_sag"]
    pub amp_sag: FloatParam,

    /// 12" Guitar Cabinet Simulation Filter
    #[id = "cab_enabled"]
    pub cab_enabled: BoolParam,

    /// Cabinet Profile: 0 = Celestion Vintage 30, 1 = Fender Twin Reverb, 2 = Marshall Greenback
    #[id = "cabinet_model"]
    pub cabinet_model: IntParam,

    /// Partitioned Convolution IR Cabinet Mode
    #[id = "use_ir_cab"]
    pub use_ir_cab: BoolParam,

    /// Smart Strum Speed [0.0 ms ~ 50.0 ms] (0 = instant solo, 18ms = acoustic strum)
    #[id = "strum_speed"]
    pub strum_speed: FloatParam,

    /// Fret Buzz / String Clatter Sensitivity [0.0 ~ 1.0]
    #[id = "fret_buzz"]
    pub fret_buzz: FloatParam,

    /// Wound String Finger Squeak / Slide Noise [0.0 ~ 1.0]
    #[id = "finger_squeak"]
    pub finger_squeak: FloatParam,

    /// Groove Accompaniment Pattern [0 = Off, 1 = Folk 4/4, 2 = Ballad 6/8, 3 = Funk 16th, 4 = Rock 8th]
    #[id = "groove_pattern"]
    pub groove_pattern: IntParam,

    /// Groove Accompaniment Manual BPM [40.0 ~ 240.0]
    #[id = "groove_bpm"]
    pub groove_bpm: FloatParam,

    /// Master Output Gain [-30 dB ~ +6 dB]
    #[id = "gain"]
    pub master_gain: FloatParam,

    /// FX: low-shelf EQ gain at 120 Hz [-12 dB ~ +12 dB]
    #[id = "fx_eq_low"]
    pub fx_eq_low: FloatParam,

    /// FX: peaking EQ gain at 1 kHz [-12 dB ~ +12 dB]
    #[id = "fx_eq_mid"]
    pub fx_eq_mid: FloatParam,

    /// FX: high-shelf EQ gain at 8 kHz [-12 dB ~ +12 dB]
    #[id = "fx_eq_high"]
    pub fx_eq_high: FloatParam,

    /// FX: reverb dry/wet mix [0.0 ~ 1.0], 0 = dry
    #[id = "fx_reverb_mix"]
    pub fx_reverb_mix: FloatParam,

    /// FX: reverb decay [0.0 ~ 1.0], higher = longer tail
    #[id = "fx_reverb_decay"]
    pub fx_reverb_decay: FloatParam,
}

impl Default for PhysicsGuitarParams {
    fn default() -> Self {
        Self {
            editor_state: default_vizia_state(),

            mode: IntParam::new("Instrument Mode", 0, IntRange::Linear { min: 0, max: 1 }),
            pluck_style: IntParam::new("Pluck Style", 0, IntRange::Linear { min: 0, max: 1 }),
            pickup_pos: IntParam::new("Pickup Position", 0, IntRange::Linear { min: 0, max: 4 }),
            pickup_type: IntParam::new("Pickup Type", 1, IntRange::Linear { min: 0, max: 1 }), // default Humbucker

            tone: FloatParam::new(
                "Passive Tone",
                1.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_unit(" %")
            .with_value_to_string(formatters::v2s_f32_percentage(0))
            .with_string_to_value(formatters::s2v_f32_percentage()),

            palm_mute: FloatParam::new("Palm Mute", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),

            pluck_pos: FloatParam::new(
                "Pluck Position",
                0.15,
                FloatRange::Linear {
                    min: 0.05,
                    max: 0.35,
                },
            )
            .with_unit(" L")
            .with_value_to_string(formatters::v2s_f32_rounded(2)),

            amp_drive: FloatParam::new(
                "12AX7 Drive",
                0.25,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_unit(" %")
            .with_value_to_string(formatters::v2s_f32_percentage(0))
            .with_string_to_value(formatters::s2v_f32_percentage()),

            amp_bass: FloatParam::new("Amp Bass", 0.50, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),

            amp_middle: FloatParam::new(
                "Amp Middle",
                0.50,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_unit(" %")
            .with_value_to_string(formatters::v2s_f32_percentage(0))
            .with_string_to_value(formatters::s2v_f32_percentage()),

            amp_treble: FloatParam::new(
                "Amp Treble",
                0.50,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_unit(" %")
            .with_value_to_string(formatters::v2s_f32_percentage(0))
            .with_string_to_value(formatters::s2v_f32_percentage()),

            amp_presence: FloatParam::new(
                "Amp Presence",
                0.50,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_unit(" %")
            .with_value_to_string(formatters::v2s_f32_percentage(0))
            .with_string_to_value(formatters::s2v_f32_percentage()),

            amp_sag: FloatParam::new("Power SAG", 0.35, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),

            cab_enabled: BoolParam::new("12\" Cabinet", true),
            cabinet_model: IntParam::new("Cabinet Model", 0, IntRange::Linear { min: 0, max: 2 }),
            use_ir_cab: BoolParam::new("UPOLS IR Cab", false),

            strum_speed: FloatParam::new(
                "Strum Speed",
                18.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 50.0,
                },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            fret_buzz: FloatParam::new(
                "Fret Clatter",
                0.35,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_unit(" %")
            .with_value_to_string(formatters::v2s_f32_percentage(0))
            .with_string_to_value(formatters::s2v_f32_percentage()),

            finger_squeak: FloatParam::new(
                "Finger Squeak",
                0.40,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_unit(" %")
            .with_value_to_string(formatters::v2s_f32_percentage(0))
            .with_string_to_value(formatters::s2v_f32_percentage()),

            groove_pattern: IntParam::new("Groove Pattern", 0, IntRange::Linear { min: 0, max: 4 }),

            groove_bpm: FloatParam::new(
                "Groove BPM",
                120.0,
                FloatRange::Linear {
                    min: 40.0,
                    max: 240.0,
                },
            )
            .with_unit(" BPM")
            .with_value_to_string(formatters::v2s_f32_rounded(0)),

            master_gain: FloatParam::new(
                "Master Gain",
                util::db_to_gain(0.0),
                FloatRange::Skewed {
                    min: util::db_to_gain(-30.0),
                    max: util::db_to_gain(6.0),
                    factor: FloatRange::gain_skew_factor(-30.0, 6.0),
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_gain_to_db(1))
            .with_string_to_value(formatters::s2v_f32_gain_to_db()),

            fx_eq_low: FloatParam::new(
                "EQ Low",
                0.0,
                FloatRange::Linear {
                    min: -12.0,
                    max: 12.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            fx_eq_mid: FloatParam::new(
                "EQ Mid",
                0.0,
                FloatRange::Linear {
                    min: -12.0,
                    max: 12.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            fx_eq_high: FloatParam::new(
                "EQ High",
                0.0,
                FloatRange::Linear {
                    min: -12.0,
                    max: 12.0,
                },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            fx_reverb_mix: FloatParam::new(
                "Reverb Mix",
                0.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_value_to_string(formatters::v2s_f32_rounded(2)),

            fx_reverb_decay: FloatParam::new(
                "Reverb Decay",
                0.5,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_value_to_string(formatters::v2s_f32_rounded(2)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GuiGuitarEvent {
    NoteOn {
        string_index: u8,
        fret: u8,
        velocity: f32,
    },
    NoteOff {
        string_index: u8,
        fret: u8,
    },
    MidiNoteOn {
        note: u8,
        velocity: f32,
    },
    MidiNoteOff {
        note: u8,
    },
    /// Strum a chord from the editor's chord library. `frets` is stored
    /// low string → high string (string 6 → string 1), `None` = muted.
    /// The direction applies to this strum only; the strummer's saved
    /// direction is restored afterwards, like the groove player does.
    StrumChord {
        frets: [Option<u8>; 6],
        velocity: f32,
        direction: crate::core::StrumDirection,
    },
}

/// Transparent tanh-based soft limiter ensuring output never exceeds 0.99 (0.0 dBFS).
/// Same curve as the piano plugin. Applied AFTER the insert FX in the
/// sample loop: the EQ can add up to +12 dB per band to the engine
/// output, so the engine's own limiter (which runs before the FX)
/// cannot bound the final output. The engine-level limiter is kept:
/// it bounds `GuitarEngine` output for engine-only consumers.
#[inline]
fn soft_limit(x: f64) -> f64 {
    if x.abs() <= 0.82 {
        x
    } else {
        let sign = x.signum();
        let mag = x.abs();
        let excess = mag - 0.82;
        sign * (0.82 + 0.17 * (excess / 0.17).tanh())
    }
}

/// Clamps a host event's sample offset to the last sample of the
/// block. Events timestamped at/past the block end are played at
/// the block end instead of being dropped with the block (and
/// everything after them).
#[inline]
fn clamp_event_timing(timing: u32, num_samples: usize) -> u32 {
    timing.min(num_samples.saturating_sub(1) as u32)
}

pub struct PhysicsGuitar {
    pub params: Arc<PhysicsGuitarParams>,
    pub engine: GuitarEngine,
    pub sample_rate: f32,

    // Insert FX (3-band EQ + reverb) applied to the engine output
    pub fx: FxChain,

    // Shared state for GUI animation (atomic / lock-free)
    pub active_frets_shared: Arc<[AtomicU8; 6]>,
    pub string_energies_shared: Arc<[AtomicU32; 6]>,
    pub string_profiles_shared: Arc<[[AtomicI32; 16]; 6]>,
    // Stereo output peak for the VU meter (same smoothing as piano)
    pub peak_l: Arc<AtomicF32>,
    pub peak_r: Arc<AtomicF32>,
    // Mono output history for the spectrum analyzer
    pub sample_history: Arc<parking_lot::RwLock<Vec<f32>>>,
    // Lock-free event queue from GUI clicks/releases into audio engine
    pub gui_event_tx: crossbeam_channel::Sender<GuiGuitarEvent>,
    pub gui_event_rx: crossbeam_channel::Receiver<GuiGuitarEvent>,
    pub language: Arc<AtomicU8>,
    pub preset_manager: Arc<parking_lot::RwLock<PresetManager>>,
    pub undo_manager: Arc<parking_lot::RwLock<UndoManager>>,

    // MIDI Learn: id -> ParamPtr built once from the params Arc (pointers
    // stay valid: the Arc is kept for the plugin's lifetime), plus the
    // last received CC for the editor's Learn polling.
    pub param_map: HashMap<String, ParamPtr>,
    pub last_cc: Arc<LastCc>,

    // De-bounce/de-repeat for GUI virtual keyboard to prevent OS auto-repeat machine-gun re-strikes
    pending_gui_midi_note_offs: [usize; 128],
    pending_gui_string_note_offs: [usize; 7],
}

impl Default for PhysicsGuitar {
    fn default() -> Self {
        let sample_rate = 44100.0;
        let engine = GuitarEngine::new(
            sample_rate,
            GuitarStringSetType::Electric010,
            GuitarInstrumentMode::Electric,
        );

        let lang_code = Language::from_system_locale().index();

        let active_frets_shared = Arc::new([
            AtomicU8::new(255),
            AtomicU8::new(255),
            AtomicU8::new(255),
            AtomicU8::new(255),
            AtomicU8::new(255),
            AtomicU8::new(255),
        ]);

        let string_energies_shared = Arc::new([
            AtomicU32::new(0),
            AtomicU32::new(0),
            AtomicU32::new(0),
            AtomicU32::new(0),
            AtomicU32::new(0),
            AtomicU32::new(0),
        ]);

        let string_profiles_shared = Arc::new(std::array::from_fn(|_| {
            std::array::from_fn(|_| AtomicI32::new(0))
        }));

        let peak_l = Arc::new(AtomicF32::new(0.0));
        let peak_r = Arc::new(AtomicF32::new(0.0));
        let sample_history = Arc::new(parking_lot::RwLock::new(vec![
            0.0;
            physics_ui::SPECTRUM_HISTORY_LEN
        ]));

        let (gui_event_tx, gui_event_rx) = crossbeam_channel::bounded(256);

        let params = Arc::new(PhysicsGuitarParams::default());
        let param_map = build_param_map(&params);
        let mut preset_manager = PresetManager::new("guitar", guitar_factory_presets());
        // Prune Learn bindings whose param id no longer exists (e.g.
        // after a rename), so stale bindings do not accumulate in
        // midi_mappings.json and silently no-op on CC input.
        let valid_param_ids: HashSet<String> = params
            .param_map()
            .into_iter()
            .map(|(id, _, _)| id)
            .collect();
        preset_manager.prune_stale_midi_mappings(&valid_param_ids);

        Self {
            params,
            param_map,
            last_cc: Arc::new(LastCc::new()),
            engine,
            sample_rate: sample_rate as f32,
            fx: FxChain::new(sample_rate),
            active_frets_shared,
            string_energies_shared,
            string_profiles_shared,
            peak_l,
            peak_r,
            sample_history,
            gui_event_tx,
            gui_event_rx,
            language: Arc::new(AtomicU8::new(lang_code)),
            preset_manager: Arc::new(parking_lot::RwLock::new(preset_manager)),
            undo_manager: Arc::new(parking_lot::RwLock::new(UndoManager::default())),
            pending_gui_midi_note_offs: [0; 128],
            pending_gui_string_note_offs: [0; 7],
        }
    }
}

impl PhysicsGuitar {
    /// Apply one incoming MIDI CC: always record it for the editor's Learn
    /// polling, drive the sostenuto pedal when CC66 arrives, and drive
    /// the bound parameter when one is learned. Reserved pedal /
    /// expression CCs (64/67/11) are recorded but otherwise ignored,
    /// mirroring the piano. The manager lock is only tried, never
    /// waited on, so a contended GUI thread can't stall the audio
    /// thread; the next CC movement retries.
    fn handle_midi_cc(&mut self, cc: u8, value: f32) {
        self.last_cc.record(cc);
        // Reserved pedal/expression CCs, mirroring the piano: CC64
        // (sustain), CC67 (soft pedal) and CC11 (expression) change
        // no parameter and cannot be Learn-bound (the manager
        // refuses excluded CCs). Guitar implements no sustain /
        // soft-pedal / expression semantics, so the reservation is
        // the whole fix: the pedal must not be stealable by Learn
        // and must behave the same on every instrument. CC66
        // (sostenuto) keeps its own handling below.
        if cc == 64 || cc == 67 || cc == 11 {
            return;
        }
        if cc == SOSTENUTO_CC {
            self.engine.set_sostenuto_pedal(cc_pedal_down(value as f64));
            return;
        }
        if let Some(manager) = self.preset_manager.try_read() {
            apply_midi_cc(&self.param_map, manager.midi_mappings(), cc, value);
        }
    }
}

impl Plugin for PhysicsGuitar {
    const NAME: &'static str = "Physics Guitar";
    const VENDOR: &'static str = "Mumulhl";
    const URL: &'static str = "https://github.com/mumu-lhl/physics-piano-experiment";
    const EMAIL: &'static str = "mumulhl@example.com";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: None,
        main_output_channels: NonZeroU32::new(2),
        ..AudioIOLayout::const_default()
    }];

    const MIDI_INPUT: MidiConfig = MidiConfig::MidiCCs;
    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();
    type BackgroundTask = ();
    type Editor = vizia_plug::ViziaEditor;

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn activate(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl ActivateContext<Self>,
    ) -> bool {
        sanitize_floating_point_environment();
        self.sample_rate = buffer_config.sample_rate;
        let mode_val = self.params.mode.value();
        let (initial_set, initial_mode) = if mode_val == 1 {
            (
                GuitarStringSetType::Acoustic012,
                GuitarInstrumentMode::Acoustic,
            )
        } else {
            (
                GuitarStringSetType::Electric010,
                GuitarInstrumentMode::Electric,
            )
        };
        self.engine = GuitarEngine::new(self.sample_rate as f64, initial_set, initial_mode);
        self.fx = FxChain::new(buffer_config.sample_rate as f64);
        true
    }

    fn reset(&mut self) {
        self.engine.reset();
        while self.gui_event_rx.try_recv().is_ok() {}
        self.pending_gui_midi_note_offs.fill(0);
        self.pending_gui_string_note_offs.fill(0);
        for f in self.active_frets_shared.iter() {
            f.store(255, Ordering::Relaxed);
        }
        for e in self.string_energies_shared.iter() {
            e.store(0, Ordering::Relaxed);
        }
        self.fx.reset();
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        sanitize_floating_point_environment();
        let debounce_samples = (self.engine.sample_rate * 0.045) as usize; // 45ms auto-repeat filter window

        // 1. Process GUI manual fret events from lock-free queue
        while let Ok(event) = self.gui_event_rx.try_recv() {
            match event {
                GuiGuitarEvent::NoteOn {
                    string_index,
                    fret,
                    velocity,
                } => {
                    if (1..=6).contains(&string_index) {
                        let s_u = string_index as usize;
                        if self.pending_gui_string_note_offs[s_u] > 0 {
                            // Key remains held down (OS auto-repeat); cancel pending release and suppress re-strike!
                            self.pending_gui_string_note_offs[s_u] = 0;
                            continue;
                        }

                        let s_i = (string_index - 1) as usize;
                        let open_note = self.engine.router.open_notes[s_i];
                        let midi_note = open_note + fret;
                        // A GUI re-strike supersedes any sostenuto
                        // capture, exactly as `note_on` does.
                        self.engine.clear_sostenuto_capture(s_i);
                        self.engine.strings[s_i].set_fret(fret);
                        self.engine.strings[s_i].pluck(
                            &self.engine.exciter,
                            self.engine.pluck_pos_ratio,
                            velocity as f64,
                        );
                        self.engine.active_notes_on_string[s_i] = Some(midi_note);
                    }
                }
                GuiGuitarEvent::NoteOff { string_index, .. } => {
                    if (1..=6).contains(&string_index) {
                        self.pending_gui_string_note_offs[string_index as usize] = debounce_samples;
                    }
                }
                GuiGuitarEvent::MidiNoteOn { note, velocity } => {
                    let n = note as usize;
                    if n < 128 && self.pending_gui_midi_note_offs[n] > 0 {
                        // Key remains held down (OS auto-repeat); cancel pending release and suppress re-strike!
                        self.pending_gui_midi_note_offs[n] = 0;
                        continue;
                    }
                    self.engine.note_on(0, note, velocity as f64);
                }
                GuiGuitarEvent::MidiNoteOff { note } => {
                    let n = note as usize;
                    if n < 128 {
                        self.pending_gui_midi_note_offs[n] = debounce_samples;
                    } else {
                        self.engine.note_off(0, note);
                    }
                }
                GuiGuitarEvent::StrumChord {
                    frets,
                    velocity,
                    direction,
                } => {
                    let shape = crate::core::ChordShape { frets };
                    self.engine
                        .strum_chord_shape(&shape, velocity as f64, direction);
                }
            }
        }

        // Process pending NoteOff events whose debounce window has elapsed (finger actually released)
        let num_samples = buffer.samples();
        for note in 0..128 {
            if self.pending_gui_midi_note_offs[note] > 0 {
                if self.pending_gui_midi_note_offs[note] <= num_samples {
                    self.pending_gui_midi_note_offs[note] = 0;
                    self.engine.note_off(0, note as u8);
                } else {
                    self.pending_gui_midi_note_offs[note] -= num_samples;
                }
            }
        }
        for s in 1..=6 {
            if self.pending_gui_string_note_offs[s] > 0 {
                if self.pending_gui_string_note_offs[s] <= num_samples {
                    self.pending_gui_string_note_offs[s] = 0;
                    self.engine.release_string(s as u8);
                } else {
                    self.pending_gui_string_note_offs[s] -= num_samples;
                }
            }
        }

        // 2. Sync plugin parameters to engine
        let mode_val = self.params.mode.value();
        let target_mode = if mode_val == 1 {
            GuitarInstrumentMode::Acoustic
        } else {
            GuitarInstrumentMode::Electric
        };
        self.engine.set_mode(target_mode);

        let style_val = self.params.pluck_style.value();
        self.engine.set_pluck_style(if style_val == 1 {
            PluckStyle::FingerFlesh
        } else {
            PluckStyle::Plectrum
        });

        let pos_val = self.params.pickup_pos.value();
        self.engine.pickup.selector = match pos_val {
            1 => PickupSelector::Middle,
            2 => PickupSelector::Neck,
            3 => PickupSelector::BridgeAndNeck,
            4 => PickupSelector::BridgeAndMiddle,
            _ => PickupSelector::Bridge,
        };

        let type_val = self.params.pickup_type.value();
        self.engine.pickup.set_pickup_type(if type_val == 1 {
            PickupType::Humbucker
        } else {
            PickupType::SingleCoil
        });

        self.engine.set_tone(self.params.tone.value() as f64);
        self.engine
            .set_palm_mute(self.params.palm_mute.value() as f64);
        self.engine
            .set_fret_buzz(self.params.fret_buzz.value() as f64);
        self.engine
            .strummer
            .set_strum_speed_ms(self.params.strum_speed.value() as f64);
        self.engine.squeak.squeak_level = self.params.finger_squeak.value() as f64;
        self.engine.groove.set_pattern(GroovePattern::from_index(
            self.params.groove_pattern.value(),
        ));
        if let Some(tempo) = context.transport().tempo {
            self.engine.groove.set_bpm(tempo);
        } else {
            self.engine
                .groove
                .set_bpm(self.params.groove_bpm.value() as f64);
        }
        self.engine.amp_cab.drive = self.params.amp_drive.value() as f64;
        self.engine.amp_cab.set_tone_stack(
            self.params.amp_bass.value() as f64,
            self.params.amp_middle.value() as f64,
            self.params.amp_treble.value() as f64,
            self.params.amp_presence.value() as f64,
        );
        self.engine.amp_cab.sag = self.params.amp_sag.value() as f64;
        self.engine.amp_cab.cab_enabled = self.params.cab_enabled.value();
        // Route through `set_cabinet_model` (change-guarded, reloads
        // the IR preset) instead of assigning the field directly,
        // which left the cabinet filters and IR on the old model.
        self.engine.set_cabinet_model(match self.params.cabinet_model.value() {
            1 => crate::core::amp_cab::CabinetModel::TwinReverb,
            2 => crate::core::amp_cab::CabinetModel::Greenback,
            _ => crate::core::amp_cab::CabinetModel::Vintage30,
        });
        self.engine
            .amp_cab
            .set_use_ir_cabinet(self.params.use_ir_cab.value());
        self.engine.pluck_pos_ratio = self.params.pluck_pos.value() as f64;
        self.engine.master_volume = self.params.master_gain.value() as f64;

        // Insert FX (3-band EQ + reverb) on the engine output. Neutral by
        // default (0 dB EQ, dry mix), so existing presets sound unchanged
        // until FX is dialed in. EQ and reverb are linear, so placing FX
        // after the master volume is equivalent to placing it before;
        // the final output is soft-limited after the FX in the sample
        // loop below, since the EQ can add +12 dB per band.
        self.fx.set_sample_rate(self.sample_rate as f64);
        self.fx.set_params(
            self.params.fx_eq_low.value() as f64,
            self.params.fx_eq_mid.value() as f64,
            self.params.fx_eq_high.value() as f64,
            self.params.fx_reverb_mix.value() as f64,
            self.params.fx_reverb_decay.value() as f64,
        );

        // 3. Sample-accurate MIDI / MPE dispatch
        let mut next_event = context.next_event();
        let mut max_l = 0.0f32;
        let mut max_r = 0.0f32;
        for (sample_idx, channel_samples) in buffer.iter_samples().enumerate() {
            while let Some(event) = next_event {
                if clamp_event_timing(event.timing(), num_samples) > sample_idx as u32 {
                    break;
                }
                match event {
                    NoteEvent::NoteOn {
                        channel,
                        key,
                        velocity,
                        ..
                    } => {
                        // A missing key/channel is "no value", never
                        // note 0 / channel 0 (the piano's pattern).
                        // The pending event must be advanced before
                        // `continue`, or this loop spins on it.
                        let Some(note) = key.number() else {
                            next_event = context.next_event();
                            continue;
                        };
                        let Some(channel_number) = channel.number() else {
                            next_event = context.next_event();
                            continue;
                        };
                        let n = note as usize;
                        if n < 128 {
                            self.pending_gui_midi_note_offs[n] = 0;
                        }
                        self.engine
                            .note_on(channel_number, note, velocity as f64);
                    }
                    NoteEvent::NoteOff { channel, key, .. } => {
                        let Some(note) = key.number() else {
                            next_event = context.next_event();
                            continue;
                        };
                        let Some(channel_number) = channel.number() else {
                            next_event = context.next_event();
                            continue;
                        };
                        self.engine.note_off(channel_number, note);
                    }
                    NoteEvent::MidiPitchBend { channel, value, .. } => {
                        // nice-plug normalizes value to [0.0, 1.0], where 0.5 is center (0 bend)
                        // Shared bend conversion: +/- 12 semitones.
                        let bend_semitones =
                            bend_semitones(value as f64, GUITAR_BEND_RANGE_SEMITONES);
                        self.engine.pitch_bend(channel, bend_semitones);
                    }
                    NoteEvent::MidiCC { cc, value, .. } => {
                        self.handle_midi_cc(cc, value);
                    }
                    _ => {}
                }
                next_event = context.next_event();
            }

            let (sl, sr) = self.engine.process_sample();
            let (sl, sr) = self.fx.process_sample(sl, sr);
            // Soft-limit AFTER the FX (same ordering and 0.99 bound as
            // the piano plugin): the engine limits before the FX, and
            // the EQ can add +12 dB/band after it.
            let (sl, sr) = (soft_limit(sl), soft_limit(sr));
            let mut channels = channel_samples.into_iter();
            if let Some(left) = channels.next() {
                *left = sl as f32;
            }
            if let Some(right) = channels.next() {
                *right = sr as f32;
            }
            max_l = max_l.max((sl as f32).abs());
            max_r = max_r.max((sr as f32).abs());
        }

        // 4. Update atomic shared states for GUI visualization
        // Update peak meter smoothly (same 0.92/0.08 smoothing as piano)
        let prev_l = self.peak_l.load(Ordering::Relaxed);
        let prev_r = self.peak_r.load(Ordering::Relaxed);
        self.peak_l
            .store(prev_l * 0.92 + max_l * 0.08, Ordering::Relaxed);
        self.peak_r
            .store(prev_r * 0.92 + max_r * 0.08, Ordering::Relaxed);

        // Append mono output to the spectrum history (skip on contention).
        if let Some(mut hist) = self.sample_history.try_write() {
            let slices = buffer.as_slice();
            if slices.len() >= 2 {
                let n = num_samples.min(slices[0].len()).min(slices[1].len());
                if hist.len() + n > physics_ui::SPECTRUM_HISTORY_LEN {
                    let excess = hist.len() + n - physics_ui::SPECTRUM_HISTORY_LEN;
                    hist.drain(..excess);
                }
                hist.extend(
                    slices[0][..n]
                        .iter()
                        .zip(slices[1][..n].iter())
                        .map(|(l, r)| (l + r) * 0.5),
                );
            }
        }
        let mut profile_buf = [0.0f32; 16];
        for i in 0..6 {
            let string = &self.engine.strings[i];
            let fret_val = if string.is_held {
                string.current_fret
            } else {
                255
            };
            self.active_frets_shared[i].store(fret_val, Ordering::Relaxed);

            let energy = (string.total_energy() * 1000.0).clamp(0.0, 1.0) as f32;
            self.string_energies_shared[i].store(energy.to_bits(), Ordering::Relaxed);

            string.sample_spatial_profile(&mut profile_buf);
            for k in 0..16 {
                let fixed = (profile_buf[k] * 10000.0) as i32;
                self.string_profiles_shared[i][k].store(fixed, Ordering::Relaxed);
            }
        }

        ProcessStatus::Normal
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Self::Editor> {
        create_vizia_guitar_editor(
            self.params.clone(),
            self.active_frets_shared.clone(),
            self.string_energies_shared.clone(),
            self.string_profiles_shared.clone(),
            self.peak_l.clone(),
            self.peak_r.clone(),
            self.sample_history.clone(),
            self.sample_rate,
            self.language.clone(),
            self.gui_event_tx.clone(),
            self.preset_manager.clone(),
            self.undo_manager.clone(),
            self.params.editor_state.clone(),
            self.last_cc.clone(),
        )
    }
}

impl ClapPlugin for PhysicsGuitar {
    const CLAP_ID: &'static str = "org.mumulhl.physics-guitar";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Physical modeling acoustic & electric guitar synthesizer");
    const CLAP_MANUAL_URL: Option<&'static str> =
        Some("https://github.com/mumu-lhl/physics-piano-experiment");
    const CLAP_SUPPORT_URL: Option<&'static str> =
        Some("https://github.com/mumu-lhl/physics-piano-experiment");
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::Instrument,
        ClapFeature::Synthesizer,
        ClapFeature::Stereo,
    ];
}

impl Vst3Plugin for PhysicsGuitar {
    const VST3_CLASS_ID: [u8; 16] = *b"PhysicsGuitarSyn";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Instrument, Vst3SubCategory::Synth];
}

nice_export_clap!(PhysicsGuitar);
nice_export_vst3!(PhysicsGuitar);

#[cfg(all(test, debug_assertions))]
mod no_alloc_regression_tests {
    use super::*;
    use nice_assert_no_alloc::{assert_no_alloc, violation_count};

    #[test]
    fn draining_gui_events_does_not_deallocate_on_the_audio_thread() {
        let plugin = PhysicsGuitar::default();
        plugin
            .gui_event_tx
            .send(GuiGuitarEvent::NoteOn {
                string_index: 0,
                fret: 0,
                velocity: 0.85,
            })
            .unwrap();
        let violations_before = violation_count();

        assert_no_alloc(|| {
            assert!(matches!(
                plugin.gui_event_rx.try_recv(),
                Ok(GuiGuitarEvent::NoteOn {
                    string_index: 0,
                    fret: 0,
                    velocity: _,
                })
            ));
        });

        assert_eq!(violation_count(), violations_before);
    }
}

#[cfg(test)]
mod midi_learn_tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    struct TempDirGuard(PathBuf);

    impl TempDirGuard {
        fn new(tag: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("ppe-midi-test-{}-{}", tag, std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).expect("tempdir");
            Self(path)
        }
    }

    impl Drop for TempDirGuard {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn midi_cc_drives_learned_param_and_records_last_cc() {
        // The manager is swapped for a temp-dir one so binding (which saves
        // midi_mappings.json) never touches the user's real data dir.
        let dir = TempDirGuard::new("guitar");
        let mut plugin = PhysicsGuitar::default();
        plugin.preset_manager = Arc::new(parking_lot::RwLock::new(PresetManager::with_custom_dir(
            "guitar",
            guitar_factory_presets(),
            dir.0.clone(),
        )));
        assert!(
            plugin
                .preset_manager
                .write()
                .bind_midi_cc(7, "tone")
                .unwrap()
        );
        plugin.handle_midi_cc(7, 0.13);
        assert!((plugin.params.tone.value() - 0.13).abs() < 1e-6);
        assert_eq!(plugin.last_cc.cc(), 7);
        assert_eq!(plugin.last_cc.seq(), 1);
        // An unmapped CC records but changes nothing.
        plugin.handle_midi_cc(8, 0.9);
        assert_eq!(plugin.last_cc.cc(), 8);
        assert_eq!(plugin.last_cc.seq(), 2);
        assert!((plugin.params.tone.value() - 0.13).abs() < 1e-6);
    }

    #[test]
    fn sostenuto_cc_drives_engine_not_learn() {
        let mut plugin = PhysicsGuitar::default();
        plugin.handle_midi_cc(66, 1.0);
        assert!(plugin.engine.sostenuto_down);
        assert_eq!(plugin.last_cc.cc(), 66);
        plugin.handle_midi_cc(66, 0.0);
        assert!(!plugin.engine.sostenuto_down);
    }

    #[test]
    fn reserved_ccs_are_recorded_but_change_nothing_and_cannot_be_bound() {
        // The manager refuses to bind excluded CCs at all.
        let dir = TempDirGuard::new("guitar-reserved");
        let mut plugin = PhysicsGuitar::default();
        plugin.preset_manager = Arc::new(parking_lot::RwLock::new(PresetManager::with_custom_dir(
            "guitar",
            guitar_factory_presets(),
            dir.0.clone(),
        )));
        for cc in [64u8, 67, 11] {
            assert!(
                !plugin
                    .preset_manager
                    .write()
                    .bind_midi_cc(cc, "tone")
                    .unwrap(),
                "CC{cc} must not be bindable"
            );
        }
        // And the handler never drives a parameter or the pedal with
        // them, mirroring the piano: they are recorded for Learn
        // polling only, so a sustain pedal can never be stolen by
        // Learn or move a knob on guitar.
        assert!(
            plugin
                .preset_manager
                .write()
                .bind_midi_cc(7, "tone")
                .unwrap()
        );
        let tone_before = plugin.params.tone.value();
        for cc in [64u8, 67, 11] {
            plugin.handle_midi_cc(cc, 1.0);
            assert_eq!(plugin.last_cc.cc(), cc);
            assert!(!plugin.engine.sostenuto_down);
            assert_eq!(plugin.params.tone.value(), tone_before);
        }
        // Sostenuto (CC66) keeps working.
        plugin.handle_midi_cc(66, 1.0);
        assert!(plugin.engine.sostenuto_down);
    }

    #[test]
    fn param_map_covers_all_param_ids() {
        let plugin = PhysicsGuitar::default();
        let ids = plugin.params.param_map();
        assert!(!ids.is_empty());
        assert_eq!(plugin.param_map.len(), ids.len());
        for (id, _, _) in &ids {
            assert!(plugin.param_map.contains_key(id), "missing {id}");
        }
    }
}

#[cfg(test)]
mod event_timing_tests {
    use super::clamp_event_timing;

    #[test]
    fn event_timing_clamped_to_last_sample_of_block() {
        assert_eq!(clamp_event_timing(0, 128), 0);
        assert_eq!(clamp_event_timing(127, 128), 127);
        assert_eq!(clamp_event_timing(128, 128), 127);
        assert_eq!(clamp_event_timing(10_000, 128), 127);
        assert_eq!(clamp_event_timing(5, 1), 0);
    }
}

#[cfg(test)]
mod output_limiter_tests {
    use super::*;

    #[test]
    fn soft_limit_bounds_output_at_piano_ceiling() {
        assert_eq!(soft_limit(0.5), 0.5);
        assert_eq!(soft_limit(-0.5), -0.5);
        // The tanh curve asymptotes to exactly 0.99 from below.
        assert!(soft_limit(10.0) <= 0.99 && soft_limit(10.0) > 0.98);
        assert!(soft_limit(-10.0) >= -0.99 && soft_limit(-10.0) < -0.98);
    }

    #[test]
    fn boosted_fx_output_is_bounded_after_soft_limit() {
        // The plugin's `process` loop cannot be driven without a host
        // `ProcessContext`, so this tests the exact composition that
        // loop performs: engine output -> FxChain -> `soft_limit`.
        let mut fx = FxChain::new(44100.0);
        fx.set_params(12.0, 12.0, 12.0, 0.0, 0.5);
        let mut peak_unlimited = 0.0f64;
        let mut peak_limited = 0.0f64;
        for n in 0..44100 {
            // 1 kHz (mid-band centre) at 0.9: +12 dB must push the
            // FX output well past full scale without the limiter.
            let x = 0.9 * (2.0 * std::f64::consts::PI * 1000.0 * n as f64 / 44100.0).sin();
            let (l, _) = fx.process_sample(x, x);
            peak_unlimited = peak_unlimited.max(l.abs());
            peak_limited = peak_limited.max(soft_limit(l).abs());
        }
        assert!(
            peak_unlimited > 0.99,
            "boosted FX output did not exceed the ceiling (peak={peak_unlimited}); test would not catch a missing limiter"
        );
        assert!(peak_limited <= 0.99, "limited peak={peak_limited}");

        // Same bound for a loud engine signal through boosted FX.
        let mut engine = GuitarEngine::new(
            44100.0,
            GuitarStringSetType::Electric010,
            GuitarInstrumentMode::Electric,
        );
        engine.strummer.set_strum_speed_ms(0.0);
        engine.note_on(6, 40, 1.0);
        let mut fx = FxChain::new(44100.0);
        fx.set_params(12.0, 12.0, 12.0, 0.0, 0.5);
        for _ in 0..44100 {
            let (l, r) = engine.process_sample();
            let (l, r) = fx.process_sample(l, r);
            assert!(soft_limit(l).abs() <= 0.99);
            assert!(soft_limit(r).abs() <= 0.99);
        }
    }

    #[test]
    fn plugin_reset_clears_engine_state() {
        let mut plugin = PhysicsGuitar::default();
        plugin.engine.strummer.set_strum_speed_ms(0.0);
        plugin.engine.note_on(6, 40, 0.9);
        for _ in 0..100 {
            plugin.engine.process_sample();
        }
        assert!(plugin.engine.strings.iter().any(|s| s.total_energy() > 0.0));
        plugin.reset();
        assert!(plugin.engine.strings.iter().all(|s| s.total_energy() == 0.0));
        assert_eq!(plugin.engine.active_notes_on_string, [None; 6]);
    }
}
