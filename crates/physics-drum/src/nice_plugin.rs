//! nice-plug adapter for the fixed-state drum kit.

use crate::{DrumEngine, gui::create_vizia_drum_editor, sanitize_floating_point_environment};
use nice_plug::prelude::*;
use std::num::NonZeroU32;
use std::sync::Arc;
use vizia_plug::ViziaState;

#[derive(Params)]
pub struct PhysicsDrumParams {
    #[persist = "editor-state-drum-v1"]
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
            editor_state: ViziaState::new(|| (660, 260)),
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

pub struct PhysicsDrum {
    pub params: Arc<PhysicsDrumParams>,
    pub engine: DrumEngine,
}

impl Default for PhysicsDrum {
    fn default() -> Self {
        Self {
            params: Arc::new(PhysicsDrumParams::default()),
            engine: DrumEngine::new(44_100.0),
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
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        sanitize_floating_point_environment();
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
        ProcessStatus::Normal
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        create_vizia_drum_editor(self.params.clone())
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
