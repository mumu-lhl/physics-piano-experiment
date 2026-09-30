//! Native CLAP plugin and standalone synthesizer built with nice-plug and Vizia for Physics Guitar.

use nice_plug::prelude::*;
use physics_presets::{PresetManager, UndoManager, guitar_factory_presets};
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU32, Ordering};
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

    /// 12" Guitar Cabinet Simulation Filter
    #[id = "cab_enabled"]
    pub cab_enabled: BoolParam,

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

            cab_enabled: BoolParam::new("12\" Cabinet", true),

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
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum GuiGuitarEvent {
    NoteOn { string_index: u8, fret: u8 },
    NoteOff { string_index: u8, fret: u8 },
}

pub struct PhysicsGuitar {
    pub params: Arc<PhysicsGuitarParams>,
    pub engine: GuitarEngine,
    pub sample_rate: f32,

    // Shared state for GUI animation (atomic / lock-free)
    pub active_frets_shared: Arc<[AtomicU8; 6]>,
    pub string_energies_shared: Arc<[AtomicU32; 6]>,
    // Lock-free event queue from GUI clicks/releases into audio engine
    pub gui_event_tx: crossbeam_channel::Sender<GuiGuitarEvent>,
    pub gui_event_rx: crossbeam_channel::Receiver<GuiGuitarEvent>,
    pub language: Arc<AtomicU8>,
    pub preset_manager: Arc<parking_lot::RwLock<PresetManager>>,
    pub undo_manager: Arc<parking_lot::RwLock<UndoManager>>,
}

impl Default for PhysicsGuitar {
    fn default() -> Self {
        let sample_rate = 44100.0;
        let engine = GuitarEngine::new(
            sample_rate,
            GuitarStringSetType::Electric010,
            GuitarInstrumentMode::Electric,
        );

        let default_lang = Language::from_system_locale();
        let lang_code = if default_lang == Language::SimplifiedChinese {
            1
        } else {
            0
        };

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

        let (gui_event_tx, gui_event_rx) = crossbeam_channel::bounded(256);

        Self {
            params: Arc::new(PhysicsGuitarParams::default()),
            engine,
            sample_rate: sample_rate as f32,
            active_frets_shared,
            string_energies_shared,
            gui_event_tx,
            gui_event_rx,
            language: Arc::new(AtomicU8::new(lang_code)),
            preset_manager: Arc::new(parking_lot::RwLock::new(PresetManager::new(
                "guitar",
                guitar_factory_presets(),
            ))),
            undo_manager: Arc::new(parking_lot::RwLock::new(UndoManager::default())),
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

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
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
        true
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // 1. Process GUI manual fret events from lock-free queue
        while let Ok(event) = self.gui_event_rx.try_recv() {
            match event {
                GuiGuitarEvent::NoteOn { string_index, fret } => {
                    if (1..=6).contains(&string_index) {
                        let s_i = (string_index - 1) as usize;
                        let open_note = self.engine.router.open_notes[s_i];
                        let midi_note = open_note + fret;
                        self.engine.strings[s_i].set_fret(fret);
                        self.engine.strings[s_i].pluck(
                            &self.engine.exciter,
                            self.engine.pluck_pos_ratio,
                            0.85,
                        );
                        self.engine.active_notes_on_string[s_i] = Some(midi_note);
                    }
                }
                GuiGuitarEvent::NoteOff { string_index, .. } => {
                    if (1..=6).contains(&string_index) {
                        self.engine.release_string(string_index);
                    }
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
        self.engine.amp_cab.cab_enabled = self.params.cab_enabled.value();
        self.engine.pluck_pos_ratio = self.params.pluck_pos.value() as f64;
        self.engine.master_volume = self.params.master_gain.value() as f64;

        // 3. Sample-accurate MIDI / MPE dispatch
        let mut next_event = context.next_event();
        for (sample_idx, channel_samples) in buffer.iter_samples().enumerate() {
            while let Some(event) = next_event {
                if event.timing() > sample_idx as u32 {
                    break;
                }
                match event {
                    NoteEvent::NoteOn {
                        channel,
                        note,
                        velocity,
                        ..
                    } => {
                        self.engine.note_on(channel, note, velocity as f64);
                    }
                    NoteEvent::NoteOff { channel, note, .. } => {
                        self.engine.note_off(channel, note);
                    }
                    NoteEvent::MidiPitchBend { channel, value, .. } => {
                        // nice-plug normalizes value to [0.0, 1.0], where 0.5 is center (0 bend)
                        // Map to +/- 12 semitones
                        let bend_semitones = (value as f64 - 0.5) * 2.0 * 12.0;
                        self.engine.pitch_bend(channel, bend_semitones);
                    }
                    _ => {}
                }
                next_event = context.next_event();
            }

            let (sl, sr) = self.engine.process_sample();
            let mut channels = channel_samples.into_iter();
            if let Some(left) = channels.next() {
                *left = sl as f32;
            }
            if let Some(right) = channels.next() {
                *right = sr as f32;
            }
        }

        // 4. Update atomic shared states for GUI visualization
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
        }

        ProcessStatus::Normal
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        create_vizia_guitar_editor(
            self.params.clone(),
            self.active_frets_shared.clone(),
            self.string_energies_shared.clone(),
            self.language.clone(),
            self.gui_event_tx.clone(),
            self.preset_manager.clone(),
            self.undo_manager.clone(),
            self.params.editor_state.clone(),
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
            })
            .unwrap();
        let violations_before = violation_count();

        assert_no_alloc(|| {
            assert!(matches!(
                plugin.gui_event_rx.try_recv(),
                Ok(GuiGuitarEvent::NoteOn {
                    string_index: 0,
                    fret: 0
                })
            ));
        });

        assert_eq!(violation_count(), violations_before);
    }
}
