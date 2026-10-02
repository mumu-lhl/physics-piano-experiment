//! nice-plug adapter for the allocation-free bass engine.

use crate::{
    BassEngine, BassMode, PluckStyle, gui::create_vizia_bass_editor,
    sanitize_floating_point_environment,
};
use nice_plug::prelude::*;
use physics_presets::{PresetManager, UndoManager, bass_factory_presets};
use std::num::NonZeroU32;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU32, Ordering};
use vizia_plug::ViziaState;

#[derive(Params)]
pub struct PhysicsBassParams {
    #[persist = "editor-state-bass-v2"]
    pub editor_state: Arc<ViziaState>,

    /// 0 = electric, 1 = acoustic wooden bass.
    #[id = "mode"]
    pub mode: IntParam,
    /// 0 = finger, 1 = pick, 2 = slap, 3 = pop, 4 = ghost.
    #[id = "pluck_style"]
    pub pluck_style: IntParam,
    /// Four-string or five-string layout.
    #[id = "five_string"]
    pub five_string: BoolParam,
    /// Finite pickup position, from neck to bridge.
    #[id = "pickup_position"]
    pub pickup_position: FloatParam,
    /// Pluck position along the vibrating string.
    #[id = "pluck_position"]
    pub pluck_position: FloatParam,
    /// Passive pickup tone.
    #[id = "tone"]
    pub tone: FloatParam,
    /// Fret/buzz contact amount.
    #[id = "fret_buzz"]
    pub fret_buzz: FloatParam,
    /// Acoustic body contribution in electric mode.
    #[id = "body_mix"]
    pub body_mix: FloatParam,
    #[id = "gain"]
    pub master_gain: FloatParam,
}

impl Default for PhysicsBassParams {
    fn default() -> Self {
        Self {
            editor_state: ViziaState::new(|| (1120, 650)),
            mode: IntParam::new("Mode", 0, IntRange::Linear { min: 0, max: 1 }),
            pluck_style: IntParam::new("Pluck Style", 0, IntRange::Linear { min: 0, max: 5 }),
            five_string: BoolParam::new("Five String", false),
            pickup_position: FloatParam::new(
                "Pickup Position",
                0.18,
                FloatRange::Linear {
                    min: 0.06,
                    max: 0.42,
                },
            )
            .with_smoother(SmoothingStyle::Linear(20.0))
            .with_unit(" L")
            .with_value_to_string(formatters::v2s_f32_rounded(2)),
            pluck_position: FloatParam::new(
                "Pluck Position",
                0.18,
                FloatRange::Linear {
                    min: 0.06,
                    max: 0.45,
                },
            )
            .with_smoother(SmoothingStyle::Linear(20.0))
            .with_unit(" L")
            .with_value_to_string(formatters::v2s_f32_rounded(2)),
            tone: FloatParam::new("Tone", 0.72, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_smoother(SmoothingStyle::Linear(20.0))
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),
            fret_buzz: FloatParam::new(
                "Fret Buzz",
                0.30,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_smoother(SmoothingStyle::Linear(20.0))
            .with_unit(" %")
            .with_value_to_string(formatters::v2s_f32_percentage(0))
            .with_string_to_value(formatters::s2v_f32_percentage()),
            body_mix: FloatParam::new("Body Mix", 0.75, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_smoother(SmoothingStyle::Linear(20.0))
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),
            master_gain: FloatParam::new(
                "Master Gain",
                util::db_to_gain(0.0),
                FloatRange::Skewed {
                    min: util::db_to_gain(-30.0),
                    max: util::db_to_gain(6.0),
                    factor: FloatRange::gain_skew_factor(-30.0, 6.0),
                },
            )
            .with_smoother(SmoothingStyle::Logarithmic(50.0))
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_gain_to_db(1))
            .with_string_to_value(formatters::s2v_f32_gain_to_db()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GuiBassEvent {
    NoteOn {
        string_index: u8,
        fret: u8,
        velocity: f64,
    },
    NoteOff {
        string_index: u8,
    },
    MidiNoteOn {
        note: u8,
        velocity: f64,
    },
    MidiNoteOff {
        note: u8,
    },
}

pub struct PhysicsBass {
    pub params: Arc<PhysicsBassParams>,
    pub engine: BassEngine,
    sample_rate: f64,
    pub active_frets_shared: Arc<[AtomicU8; 5]>,
    pub string_energies_shared: Arc<[AtomicU32; 5]>,
    pub gui_event_tx: crossbeam_channel::Sender<GuiBassEvent>,
    gui_event_rx: crossbeam_channel::Receiver<GuiBassEvent>,
    pub language: Arc<AtomicU8>,
    pub preset_manager: Arc<parking_lot::RwLock<PresetManager>>,
    pub undo_manager: Arc<parking_lot::RwLock<UndoManager>>,

    // De-bounce/de-repeat for GUI virtual keyboard to prevent OS auto-repeat machine-gun re-strikes
    pending_gui_midi_note_offs: [usize; 128],
    pending_gui_string_note_offs: [usize; 5],
}

impl Default for PhysicsBass {
    fn default() -> Self {
        let (gui_event_tx, gui_event_rx) = crossbeam_channel::bounded(256);
        let language = physics_ui::Language::from_system_locale().index();
        Self {
            params: Arc::new(PhysicsBassParams::default()),
            engine: BassEngine::new(44_100.0, BassMode::Electric, false),
            sample_rate: 44_100.0,
            active_frets_shared: Arc::new([
                AtomicU8::new(255),
                AtomicU8::new(255),
                AtomicU8::new(255),
                AtomicU8::new(255),
                AtomicU8::new(255),
            ]),
            string_energies_shared: Arc::new([
                AtomicU32::new(0),
                AtomicU32::new(0),
                AtomicU32::new(0),
                AtomicU32::new(0),
                AtomicU32::new(0),
            ]),
            gui_event_tx,
            gui_event_rx,
            language: Arc::new(AtomicU8::new(language)),
            preset_manager: Arc::new(parking_lot::RwLock::new(PresetManager::new(
                "bass",
                bass_factory_presets(),
            ))),
            undo_manager: Arc::new(parking_lot::RwLock::new(UndoManager::default())),
            pending_gui_midi_note_offs: [0; 128],
            pending_gui_string_note_offs: [0; 5],
        }
    }
}


impl PhysicsBass {
    fn sync_parameters(&mut self) {
        self.engine.set_mode(if self.params.mode.value() == 0 {
            BassMode::Electric
        } else {
            BassMode::Acoustic
        });
        self.engine.set_five_string(self.params.five_string.value());
        self.engine
            .set_pluck_style(match self.params.pluck_style.value() {
                1 => PluckStyle::Pick,
                2 => PluckStyle::Slap,
                3 => PluckStyle::Pop,
                4 => PluckStyle::Ghost,
                5 => PluckStyle::Arco,
                _ => PluckStyle::Finger,
            });
    }

    #[inline]
    fn apply_smoothed_parameters(&mut self) {
        self.engine
            .set_pickup_position(self.params.pickup_position.smoothed.next() as f64);
        self.engine
            .set_pluck_position(self.params.pluck_position.smoothed.next() as f64);
        self.engine
            .set_tone(self.params.tone.smoothed.next() as f64);
        self.engine
            .set_fret_buzz(self.params.fret_buzz.smoothed.next() as f64);
        self.engine
            .set_body_mix(self.params.body_mix.smoothed.next() as f64);
        self.engine
            .set_master_gain(self.params.master_gain.smoothed.next() as f64);
    }

    fn reset_smoothers(&self) {
        self.params
            .pickup_position
            .smoothed
            .reset(self.params.pickup_position.value());
        self.params
            .pluck_position
            .smoothed
            .reset(self.params.pluck_position.value());
        self.params.tone.smoothed.reset(self.params.tone.value());
        self.params
            .fret_buzz
            .smoothed
            .reset(self.params.fret_buzz.value());
        self.params
            .body_mix
            .smoothed
            .reset(self.params.body_mix.value());
        self.params
            .master_gain
            .smoothed
            .reset(self.params.master_gain.value());
    }
}

impl Plugin for PhysicsBass {
    const NAME: &'static str = "Physics Bass";
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
        sanitize_floating_point_environment();
        self.sample_rate = buffer_config.sample_rate as f64;
        self.engine = BassEngine::new(
            self.sample_rate,
            if self.params.mode.value() == 0 {
                BassMode::Electric
            } else {
                BassMode::Acoustic
            },
            self.params.five_string.value(),
        );
        self.reset_smoothers();
        self.sync_parameters();
        self.apply_smoothed_parameters();
        true
    }

    fn reset(&mut self) {
        self.engine.reset();
        self.reset_smoothers();
        self.sync_parameters();
        self.apply_smoothed_parameters();
        while self.gui_event_rx.try_recv().is_ok() {}
        self.pending_gui_midi_note_offs.fill(0);
        self.pending_gui_string_note_offs.fill(0);
        for fret in self.active_frets_shared.iter() {
            fret.store(255, Ordering::Relaxed);
        }
        for energy in self.string_energies_shared.iter() {
            energy.store(0, Ordering::Relaxed);
        }
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        sanitize_floating_point_environment();

        let debounce_samples = (self.sample_rate * 0.045) as usize; // 45ms auto-repeat filter window

        // GUI fretboard gestures use a bounded lock-free queue. `try_recv()`
        // keeps the audio callback wait-free even if the user drags quickly.
        while let Ok(event) = self.gui_event_rx.try_recv() {
            match event {
                GuiBassEvent::NoteOn {
                    string_index,
                    fret,
                    velocity,
                } => {
                    let s_u = string_index as usize;
                    if s_u < 5 && self.pending_gui_string_note_offs[s_u] > 0 {
                        // Key remains held down (OS auto-repeat); cancel pending release and suppress re-strike!
                        self.pending_gui_string_note_offs[s_u] = 0;
                        continue;
                    }
                    self.engine
                        .note_on_string(string_index as usize, fret, velocity);
                }
                GuiBassEvent::NoteOff { string_index } => {
                    let s_u = string_index as usize;
                    if s_u < 5 {
                        self.pending_gui_string_note_offs[s_u] = debounce_samples;
                    } else {
                        self.engine.note_off_string(s_u);
                    }
                }
                GuiBassEvent::MidiNoteOn { note, velocity } => {
                    let n = note as usize;
                    if n < 128 && self.pending_gui_midi_note_offs[n] > 0 {
                        // Key remains held down (OS auto-repeat); cancel pending release and suppress re-strike!
                        self.pending_gui_midi_note_offs[n] = 0;
                        continue;
                    }
                    self.engine.note_on(note, velocity);
                }
                GuiBassEvent::MidiNoteOff { note } => {
                    let n = note as usize;
                    if n < 128 {
                        self.pending_gui_midi_note_offs[n] = debounce_samples;
                    } else {
                        self.engine.note_off(note);
                    }
                }
            }
        }

        // Process pending NoteOff events whose debounce window has elapsed (finger actually released)
        let num_samples = buffer.samples();
        for note in 0..128 {
            if self.pending_gui_midi_note_offs[note] > 0 {
                if self.pending_gui_midi_note_offs[note] <= num_samples {
                    self.pending_gui_midi_note_offs[note] = 0;
                    self.engine.note_off(note as u8);
                } else {
                    self.pending_gui_midi_note_offs[note] -= num_samples;
                }
            }
        }
        for s in 0..5 {
            if self.pending_gui_string_note_offs[s] > 0 {
                if self.pending_gui_string_note_offs[s] <= num_samples {
                    self.pending_gui_string_note_offs[s] = 0;
                    self.engine.note_off_string(s);
                } else {
                    self.pending_gui_string_note_offs[s] -= num_samples;
                }
            }
        }

        self.sync_parameters();
        let mut next_event = context.next_event();
        for (sample_index, channel_samples) in buffer.iter_samples().enumerate() {
            while let Some(event) = next_event {
                if event.timing() > sample_index as u32 {
                    break;
                }
                match event {
                    NoteEvent::NoteOn { note, velocity, .. } => {
                        let n = note as usize;
                        if n < 128 {
                            self.pending_gui_midi_note_offs[n] = 0;
                        }
                        self.engine.note_on(note, velocity as f64);
                    }
                    NoteEvent::NoteOff { note, .. } => {
                        let n = note as usize;
                        if n < 128 {
                            self.pending_gui_midi_note_offs[n] = 0;
                        }
                        self.engine.note_off(note);
                    }
                    NoteEvent::MidiPitchBend { value, .. } => {
                        self.engine.set_pitch_bend((value as f64 - 0.5) * 24.0);
                    }
                    _ => {}
                }
                next_event = context.next_event();
            }
            self.apply_smoothed_parameters();
            let (left, right) = self.engine.process_sample();
            let mut channels = channel_samples.into_iter();
            if let Some(channel) = channels.next() {
                *channel = left as f32;
            }
            if let Some(channel) = channels.next() {
                *channel = right as f32;
            }
        }

        for (index, voice) in self.engine.strings.iter().enumerate() {
            let fret = if voice.string.is_held {
                voice.current_fret
            } else {
                255
            };
            self.active_frets_shared[index].store(fret, Ordering::Relaxed);
            let energy = voice.string.energy();
            let visual_energy = (energy / (energy + 1.0)).sqrt().clamp(0.0, 1.0) as f32;
            self.string_energies_shared[index].store(visual_energy.to_bits(), Ordering::Relaxed);
        }
        ProcessStatus::Normal
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        create_vizia_bass_editor(
            self.params.clone(),
            self.active_frets_shared.clone(),
            self.string_energies_shared.clone(),
            self.gui_event_tx.clone(),
            self.language.clone(),
            self.preset_manager.clone(),
            self.undo_manager.clone(),
        )
    }
}

impl ClapPlugin for PhysicsBass {
    const CLAP_ID: &'static str = "org.mumulhl.physics-bass";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Physical-modeling electric and acoustic bass synthesizer");
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

impl Vst3Plugin for PhysicsBass {
    const VST3_CLASS_ID: [u8; 16] = *b"PhysicsBassSynth";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Instrument, Vst3SubCategory::Synth];
}

#[cfg(all(test, debug_assertions))]
mod no_alloc_regression_tests {
    use super::PhysicsBass;
    use nice_assert_no_alloc::{assert_no_alloc, violation_count};

    #[test]
    fn prepared_engine_sample_does_not_allocate() {
        let mut plugin = PhysicsBass::default();
        plugin.engine.note_on_string(1, 5, 1.0);
        let before = violation_count();
        assert_no_alloc(|| {
            for _ in 0..4_096 {
                let _ = plugin.engine.process_sample();
            }
        });
        assert_eq!(violation_count(), before);
    }
}
