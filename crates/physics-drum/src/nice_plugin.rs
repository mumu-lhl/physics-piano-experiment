//! nice-plug adapter for the fixed-state drum kit.

use crate::{DrumEngine, gui::create_vizia_drum_editor, sanitize_floating_point_environment};
use nice_plug::prelude::*;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use vizia_plug::ViziaState;

#[derive(Params)]
pub struct PhysicsDrumParams {
    #[persist = "editor-state-drum-v2"]
    pub editor_state: Arc<ViziaState>,

    #[id = "snare_tightness"]
    pub snare_tightness: FloatParam,
    #[id = "snare_decay"]
    pub snare_decay: FloatParam,
    #[id = "hihat_open"]
    pub hihat_open: FloatParam,
    #[id = "cymbal_decay"]
    pub cymbal_decay: FloatParam,
    #[id = "gain"]
    pub master_gain: FloatParam,
}

impl Default for PhysicsDrumParams {
    fn default() -> Self {
        Self {
            editor_state: ViziaState::new(|| (900, 575)),
            snare_tightness: FloatParam::new(
                "Snare Tightness",
                0.62,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_unit(" %")
            .with_value_to_string(formatters::v2s_f32_percentage(0))
            .with_string_to_value(formatters::s2v_f32_percentage()),
            snare_decay: FloatParam::new(
                "Snare Decay",
                0.58,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_unit(" %")
            .with_value_to_string(formatters::v2s_f32_percentage(0))
            .with_string_to_value(formatters::s2v_f32_percentage()),
            hihat_open: FloatParam::new(
                "Hi-Hat Open",
                0.85,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_unit(" %")
            .with_value_to_string(formatters::v2s_f32_percentage(0))
            .with_string_to_value(formatters::s2v_f32_percentage()),
            cymbal_decay: FloatParam::new(
                "Cymbal Decay",
                0.70,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
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
pub enum GuiDrumEvent {
    Hit { note: u8, velocity: f64 },
}

pub struct PhysicsDrum {
    pub params: Arc<PhysicsDrumParams>,
    pub engine: DrumEngine,
    pub voice_energies_shared: Arc<[AtomicU32; 10]>,
    pub gui_event_tx: crossbeam_channel::Sender<GuiDrumEvent>,
    gui_event_rx: crossbeam_channel::Receiver<GuiDrumEvent>,
}

impl Default for PhysicsDrum {
    fn default() -> Self {
        let (gui_event_tx, gui_event_rx) = crossbeam_channel::bounded(256);
        Self {
            params: Arc::new(PhysicsDrumParams::default()),
            engine: DrumEngine::new(44_100.0),
            voice_energies_shared: Arc::new([
                AtomicU32::new(0),
                AtomicU32::new(0),
                AtomicU32::new(0),
                AtomicU32::new(0),
                AtomicU32::new(0),
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

impl Plugin for PhysicsDrum {
    const NAME: &'static str = "Physics Drum";
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
        self.engine = DrumEngine::new(buffer_config.sample_rate as f64);
        true
    }

    fn reset(&mut self) {
        self.engine.reset();
        while self.gui_event_rx.try_recv().is_ok() {}
        for energy in self.voice_energies_shared.iter() {
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

        // Pad clicks are delivered through a bounded queue and never block the
        // audio callback, even while the GUI is being resized or redrawn.
        while let Ok(event) = self.gui_event_rx.try_recv() {
            match event {
                GuiDrumEvent::Hit { note, velocity } => self.engine.trigger(note, velocity),
            }
        }

        self.engine
            .set_snare_tightness(self.params.snare_tightness.value() as f64);
        self.engine
            .set_snare_decay(self.params.snare_decay.value() as f64);
        self.engine
            .set_hi_hat_open(self.params.hihat_open.value() as f64);
        self.engine
            .set_cymbal_decay(self.params.cymbal_decay.value() as f64);
        self.engine
            .set_master_gain(self.params.master_gain.value() as f64);

        let mut next_event = context.next_event();
        for (sample_index, channel_samples) in buffer.iter_samples().enumerate() {
            while let Some(event) = next_event {
                if event.timing() > sample_index as u32 {
                    break;
                }
                match event {
                    NoteEvent::NoteOn { note, velocity, .. } => {
                        self.engine.trigger(note, velocity as f64);
                    }
                    NoteEvent::NoteOff { note, .. } => self.engine.release(note),
                    NoteEvent::MidiCC { cc, value, .. } if cc == 4 => {
                        // General-MIDI foot controller drives the continuous
                        // hi-hat opening without waiting for the next hit.
                        self.engine.set_hi_hat_open(value as f64);
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

        let energies = [
            self.engine.crash.energy(),
            self.engine.ride.energy(),
            self.engine.hats.energy(),
            self.engine.hats.energy(),
            self.engine.toms[1].top.energy() + self.engine.toms[1].bottom.energy(),
            self.engine.toms[1].top.energy() + self.engine.toms[1].bottom.energy(),
            self.engine.toms[0].top.energy() + self.engine.toms[0].bottom.energy(),
            self.engine.snare.top.energy() + self.engine.snare.bottom.energy(),
            self.engine.kick.top.energy() + self.engine.kick.bottom.energy(),
            self.engine.hats.energy(),
        ];
        for (shared, energy) in self.voice_energies_shared.iter().zip(energies) {
            let visual_energy = (energy / (energy + 1.0)).sqrt().clamp(0.0, 1.0) as f32;
            shared.store(visual_energy.to_bits(), Ordering::Relaxed);
        }
        ProcessStatus::Normal
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        create_vizia_drum_editor(
            self.params.clone(),
            self.voice_energies_shared.clone(),
            self.gui_event_tx.clone(),
        )
    }
}

impl ClapPlugin for PhysicsDrum {
    const CLAP_ID: &'static str = "org.mumulhl.physics-drum";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Hybrid physical-modeling General MIDI drum kit");
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

impl Vst3Plugin for PhysicsDrum {
    const VST3_CLASS_ID: [u8; 16] = *b"PhysicsDrumSynth";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Instrument, Vst3SubCategory::Synth];
}

#[cfg(all(test, debug_assertions))]
mod no_alloc_regression_tests {
    use super::PhysicsDrum;
    use nice_assert_no_alloc::{assert_no_alloc, violation_count};

    #[test]
    fn prepared_engine_sample_does_not_allocate() {
        let mut plugin = PhysicsDrum::default();
        let before = violation_count();
        assert_no_alloc(|| {
            let _ = plugin.engine.process_sample();
        });
        assert_eq!(violation_count(), before);
    }
}
