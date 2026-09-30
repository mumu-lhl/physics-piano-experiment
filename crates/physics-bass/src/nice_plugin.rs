//! nice-plug adapter for the allocation-free bass engine.

use crate::{
    BassEngine, BassMode, PluckStyle, gui::create_vizia_bass_editor,
    sanitize_floating_point_environment,
};
use nice_plug::prelude::*;
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
    /// 0 = finger, 1 = pick, 2 = slap.
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
            pluck_style: IntParam::new("Pluck Style", 0, IntRange::Linear { min: 0, max: 2 }),
            five_string: BoolParam::new("Five String", false),
            pickup_position: FloatParam::new(
                "Pickup Position",
                0.18,
                FloatRange::Linear {
                    min: 0.06,
                    max: 0.42,
                },
            )
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
            .with_unit(" L")
            .with_value_to_string(formatters::v2s_f32_rounded(2)),
            tone: FloatParam::new("Tone", 0.72, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_unit(" %")
                .with_value_to_string(formatters::v2s_f32_percentage(0))
                .with_string_to_value(formatters::s2v_f32_percentage()),
            fret_buzz: FloatParam::new(
                "Fret Buzz",
                0.30,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_unit(" %")
            .with_value_to_string(formatters::v2s_f32_percentage(0))
            .with_string_to_value(formatters::s2v_f32_percentage()),
            body_mix: FloatParam::new("Body Mix", 0.75, FloatRange::Linear { min: 0.0, max: 1.0 })
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
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_gain_to_db(1))
            .with_string_to_value(formatters::s2v_f32_gain_to_db()),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum GuiBassEvent {
    NoteOn { note: u8, velocity: f64 },
    NoteOff { note: u8 },
}

pub struct PhysicsBass {
    pub params: Arc<PhysicsBassParams>,
    pub engine: BassEngine,
    sample_rate: f64,
    pub active_frets_shared: Arc<[AtomicU8; 5]>,
    pub string_energies_shared: Arc<[AtomicU32; 5]>,
    pub gui_event_tx: crossbeam_channel::Sender<GuiBassEvent>,
    gui_event_rx: crossbeam_channel::Receiver<GuiBassEvent>,
}

impl Default for PhysicsBass {
    fn default() -> Self {
        let (gui_event_tx, gui_event_rx) = crossbeam_channel::bounded(256);
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
                _ => PluckStyle::Finger,
            });
        self.engine
            .set_pickup_position(self.params.pickup_position.value() as f64);
        self.engine
            .set_pluck_position(self.params.pluck_position.value() as f64);
        self.engine.set_tone(self.params.tone.value() as f64);
        self.engine
            .set_fret_buzz(self.params.fret_buzz.value() as f64);
        self.engine
            .set_body_mix(self.params.body_mix.value() as f64);
        self.engine
            .set_master_gain(self.params.master_gain.value() as f64);
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
        true
    }

    fn reset(&mut self) {
        self.engine.reset();
        while self.gui_event_rx.try_recv().is_ok() {}
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

        // GUI fretboard gestures use a bounded lock-free queue. `try_recv()`
        // keeps the audio callback wait-free even if the user drags quickly.
        while let Ok(event) = self.gui_event_rx.try_recv() {
            match event {
                GuiBassEvent::NoteOn { note, velocity } => self.engine.note_on(note, velocity),
                GuiBassEvent::NoteOff { note } => self.engine.note_off(note),
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
                        self.engine.note_on(note, velocity as f64);
                    }
                    NoteEvent::NoteOff { note, .. } => self.engine.note_off(note),
                    NoteEvent::MidiPitchBend { value, .. } => {
                        self.engine.set_pitch_bend((value as f64 - 0.5) * 24.0);
                    }
                    _ => {}
                }
                next_event = context.next_event();
            }
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
        let before = violation_count();
        assert_no_alloc(|| {
            let _ = plugin.engine.process_sample();
        });
        assert_eq!(violation_count(), before);
    }
}
