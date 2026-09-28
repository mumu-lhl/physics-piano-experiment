//! Native CLAP Plugin & Standalone Synthesizer built with nih-plug and vizia.

use atomic_float::AtomicF32;
use nih_plug::prelude::*;
use nih_plug_vizia::ViziaState;
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;

use crate::engine::{EngineEvent, EngineOutEvent, PianoEngine};
use crate::gui::{create_vizia_piano_editor, default_vizia_state, Language};
use physics_presets::{piano_factory_presets, PresetManager, UndoManager};

#[derive(Params)]
pub struct PhysicsPianoParams {
    #[persist = "editor-state"]
    pub editor_state: Arc<ViziaState>,

    /// Sustain Pedal Depth [0.0 ~ 1.0] (Half-pedaling)
    #[id = "sustain"]
    pub sustain_pedal: FloatParam,

    /// Soft Pedal (Una Corda)
    #[id = "unacorda"]
    pub una_corda: BoolParam,

    /// String Inharmonicity B factor scale [0.2 ~ 2.5]
    #[id = "inharm"]
    pub inharmonicity_scale: FloatParam,

    /// Hammer Felt Hardness scale [0.5 ~ 2.5]
    #[id = "hardness"]
    pub hammer_hardness: FloatParam,

    /// Trichord Unison Detuning Spread [0.0 ~ 3.0]
    #[id = "detune"]
    pub unison_detuning: FloatParam,

    /// Longitudinal Phantom Partial Coupling Gain [0.0 ~ 2.0]
    #[id = "phantom"]
    pub phantom_gain: FloatParam,

    /// Keybed thump and escapement click noise level [0.0 ~ 2.0]
    #[id = "keynoise"]
    pub key_noise: FloatParam,

    /// Damper lift whoosh and restrike buzz noise level [0.0 ~ 2.0]
    #[id = "dampernoise"]
    pub damper_noise: FloatParam,

    /// Pedal trapwork shock impulse noise level [0.0 ~ 2.0]
    #[id = "pedalnoise"]
    pub pedal_noise: FloatParam,

    /// Close Microphone Gain [-60 dB ~ +6 dB]
    #[id = "mic_close"]
    pub mic_close: FloatParam,

    /// Player Seated Binaural Microphone Gain [-60 dB ~ +6 dB]
    #[id = "mic_player"]
    pub mic_player: FloatParam,

    /// Ambient Hall Decca Tree Microphone Gain [-60 dB ~ +6 dB]
    #[id = "mic_ambient"]
    pub mic_ambient: FloatParam,

    /// Grand Piano Continuous Lid Opening Angle [0.0 ~ 60.0 deg]
    #[id = "lid_angle"]
    pub lid_angle: FloatParam,

    /// Master Volume Gain [-30 dB ~ +6 dB]
    #[id = "gain"]
    pub master_gain: FloatParam,

    /// Keyboard Velocity Touch Sensitivity [-1.0 (Soft) ~ +1.0 (Hard)]
    #[id = "velocity_curve"]
    pub velocity_curve: FloatParam,
}

impl Default for PhysicsPianoParams {
    fn default() -> Self {
        Self {
            editor_state: default_vizia_state(),

            sustain_pedal: FloatParam::new(
                "Sustain Pedal",
                0.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_unit(" %")
            .with_value_to_string(formatters::v2s_f32_percentage(0))
            .with_string_to_value(formatters::s2v_f32_percentage()),

            una_corda: BoolParam::new("Una Corda", false),

            inharmonicity_scale: FloatParam::new(
                "Stiffness (B)",
                1.0,
                FloatRange::Linear { min: 0.2, max: 2.5 },
            )
            .with_unit(" x")
            .with_value_to_string(formatters::v2s_f32_rounded(2)),

            hammer_hardness: FloatParam::new(
                "Hammer Hardness",
                1.0,
                FloatRange::Linear { min: 0.5, max: 2.5 },
            )
            .with_unit(" x")
            .with_value_to_string(formatters::v2s_f32_rounded(2)),

            unison_detuning: FloatParam::new(
                "Unison Detune",
                1.0,
                FloatRange::Linear { min: 0.0, max: 3.0 },
            )
            .with_unit(" x")
            .with_value_to_string(formatters::v2s_f32_rounded(2)),

            phantom_gain: FloatParam::new(
                "Phantom Partials",
                1.0,
                FloatRange::Linear { min: 0.0, max: 2.0 },
            )
            .with_unit(" x")
            .with_value_to_string(formatters::v2s_f32_rounded(2)),

            key_noise: FloatParam::new(
                "Key Action",
                1.0,
                FloatRange::Linear { min: 0.0, max: 2.0 },
            )
            .with_unit(" x")
            .with_value_to_string(formatters::v2s_f32_rounded(2)),

            damper_noise: FloatParam::new(
                "Damper Noise",
                1.0,
                FloatRange::Linear { min: 0.0, max: 2.0 },
            )
            .with_unit(" x")
            .with_value_to_string(formatters::v2s_f32_rounded(2)),

            pedal_noise: FloatParam::new(
                "Pedal Shock",
                1.0,
                FloatRange::Linear { min: 0.0, max: 2.0 },
            )
            .with_unit(" x")
            .with_value_to_string(formatters::v2s_f32_rounded(2)),

            mic_close: FloatParam::new(
                "Close Mic",
                0.0,
                FloatRange::Linear { min: -60.0, max: 6.0 },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            mic_player: FloatParam::new(
                "Player Mic",
                -3.0,
                FloatRange::Linear { min: -60.0, max: 6.0 },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            mic_ambient: FloatParam::new(
                "Ambient Mic",
                -6.0,
                FloatRange::Linear { min: -60.0, max: 6.0 },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),

            lid_angle: FloatParam::new(
                "Lid Angle",
                45.0,
                FloatRange::Linear { min: 0.0, max: 60.0 },
            )
            .with_unit("°")
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
            .with_smoother(SmoothingStyle::Logarithmic(50.0))
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_gain_to_db(1))
            .with_string_to_value(formatters::s2v_f32_gain_to_db()),

            velocity_curve: FloatParam::new(
                "Touch Curve",
                0.0,
                FloatRange::Linear { min: -1.0, max: 1.0 },
            )
            .with_unit("")
            .with_value_to_string(formatters::v2s_f32_rounded(2)),
        }
    }
}

pub struct PhysicsPiano {
    params: Arc<PhysicsPianoParams>,
    engine: PianoEngine,

    // Lock-free queues between GUI and audio thread for virtual keyboard playing
    gui_event_tx: crossbeam_channel::Sender<EngineEvent>,
    gui_event_rx: crossbeam_channel::Receiver<EngineEvent>,

    // Real-time visualization state shared with GUI (lock-free atomics)
    peak_l: Arc<AtomicF32>,
    peak_r: Arc<AtomicF32>,
    active_keys_low: Arc<AtomicU64>,   // Bitset for MIDI 21..84 (64 keys)
    active_keys_high: Arc<AtomicU64>,  // Bitset for MIDI 85..108 (24 keys)
    key_velocities: Arc<parking_lot::RwLock<[f32; 88]>>,
    recent_orbit_t: Arc<parking_lot::RwLock<Vec<f32>>>,
    recent_orbit_p: Arc<parking_lot::RwLock<Vec<f32>>>,
    language: Arc<AtomicU8>,           // 0: English, 1: SimplifiedChinese
    preset_manager: Arc<parking_lot::RwLock<PresetManager>>,
    undo_manager: Arc<parking_lot::RwLock<UndoManager>>,

    // Edge-triggered parameter tracking to prevent overwriting MIDI CC
    prev_sustain: f32,
    prev_una_corda: bool,

    // Scratch buffers
    events_scratch: Vec<EngineEvent>,
    out_events_scratch: Vec<EngineOutEvent>,
    scratch_l: Vec<f64>,
    scratch_r: Vec<f64>,

    // De-bounce/de-repeat for GUI virtual keyboard to prevent OS auto-repeat machine-gun re-strikes
    pending_gui_note_offs: HashMap<u8, usize>,
}

impl Default for PhysicsPiano {
    fn default() -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        let default_lang = Language::from_system_locale();
        let lang_code = if default_lang == Language::SimplifiedChinese { 1 } else { 0 };

        Self {
            params: Arc::new(PhysicsPianoParams::default()),
            engine: PianoEngine::new(48000.0, 35, true),
            gui_event_tx: tx,
            gui_event_rx: rx,
            peak_l: Arc::new(AtomicF32::new(0.0)),
            peak_r: Arc::new(AtomicF32::new(0.0)),
            active_keys_low: Arc::new(AtomicU64::new(0)),
            active_keys_high: Arc::new(AtomicU64::new(0)),
            key_velocities: Arc::new(parking_lot::RwLock::new([0.0; 88])),
            recent_orbit_t: Arc::new(parking_lot::RwLock::new(vec![0.0; 64])),
            recent_orbit_p: Arc::new(parking_lot::RwLock::new(vec![0.0; 64])),
            language: Arc::new(AtomicU8::new(lang_code)),
            preset_manager: Arc::new(parking_lot::RwLock::new(PresetManager::new(
                "piano",
                piano_factory_presets(),
            ))),
            undo_manager: Arc::new(parking_lot::RwLock::new(UndoManager::default())),
            prev_sustain: 0.0,
            prev_una_corda: false,
            events_scratch: Vec::with_capacity(64),
            out_events_scratch: Vec::with_capacity(64),
            scratch_l: vec![0.0; 512],
            scratch_r: vec![0.0; 512],
            pending_gui_note_offs: HashMap::new(),
        }
    }
}

impl Plugin for PhysicsPiano {
    const NAME: &'static str = "Physics Piano";
    const VENDOR: &'static str = "Mumulhl";
    const URL: &'static str = "https://github.com/mumu-lhl/physics-piano-experiment";
    const EMAIL: &'static str = "";
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
        self.engine = PianoEngine::new(buffer_config.sample_rate as f64, 35, true);
        let max_samples = buffer_config.max_buffer_size as usize;
        self.scratch_l = vec![0.0; max_samples];
        self.scratch_r = vec![0.0; max_samples];

        // Seed initial parameters
        self.prev_sustain = self.params.sustain_pedal.value();
        self.prev_una_corda = self.params.una_corda.value();
        self.engine.set_sustain_pedal(self.prev_sustain > 0.01, self.prev_sustain as f64);
        self.engine.set_una_corda(self.prev_una_corda);
        self.pending_gui_note_offs.clear();
        true
    }

    fn reset(&mut self) {
        self.engine.reset();
        self.active_keys_low.store(0, Ordering::Relaxed);
        self.active_keys_high.store(0, Ordering::Relaxed);
        self.pending_gui_note_offs.clear();
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let num_samples = buffer.samples();
        if self.scratch_l.len() < num_samples {
            self.scratch_l.resize(num_samples, 0.0);
            self.scratch_r.resize(num_samples, 0.0);
        }

        // 1. Gather Host Note and MIDI CC Events
        self.events_scratch.clear();
        self.out_events_scratch.clear();

        while let Some(event) = context.next_event() {
            match event {
                NoteEvent::NoteOn { note, velocity, timing, .. } => {
                    let key_idx = (note as i32 - 21) as usize;
                    if key_idx < 88 {
                        if let Some(mut vels) = self.key_velocities.try_write() {
                            vels[key_idx] = velocity;
                        }
                    }
                    self.events_scratch.push(EngineEvent::NoteOn {
                        time: timing as usize,
                        key: note,
                        velocity: velocity as f64,
                    });
                }
                NoteEvent::NoteOff { note, timing, .. } => {
                    self.events_scratch.push(EngineEvent::NoteOff {
                        time: timing as usize,
                        key: note,
                    });
                }
                NoteEvent::MidiPitchBend { value, timing, .. } => {
                    // value is in [0.0, 1.0], center is 0.5. Standard bend is +/- 2 semitones = +/- 200 cents.
                    let cents = (value as f64 - 0.5) * 2.0 * 200.0;
                    self.events_scratch.push(EngineEvent::PitchBend {
                        time: timing as usize,
                        cents,
                    });
                }
                NoteEvent::MidiCC { cc, value, timing, .. } => {
                    if cc == 64 {
                        let depth = value as f64;
                        self.events_scratch.push(EngineEvent::SustainPedal {
                            time: timing as usize,
                            depth,
                        });
                    } else if cc == 67 {
                        self.events_scratch.push(EngineEvent::UnaCorda {
                            time: timing as usize,
                            enabled: value >= 0.5,
                        });
                    } else if cc == 11 {
                        let gain = (value as f64).clamp(0.0, 1.0);
                        self.events_scratch.push(EngineEvent::Expression {
                            time: timing as usize,
                            gain,
                        });
                    }
                }
                _ => {}
            }
        }

        // 2. Consume events triggered by on-screen virtual keyboard or computer keyboard
        while let Ok(gui_ev) = self.gui_event_rx.try_recv() {
            match gui_ev {
                EngineEvent::NoteOn { key, velocity, .. } => {
                    // If this key was pending release due to X11/OS auto-repeat, cancel release and suppress re-strike!
                    if self.pending_gui_note_offs.remove(&key).is_some() {
                        // Key remains held down; do not re-strike the string or accumulate energy!
                        continue;
                    }

                    let key_idx = (key as i32 - 21) as usize;
                    if key_idx < 88 {
                        if let Some(mut vels) = self.key_velocities.try_write() {
                            vels[key_idx] = velocity as f32;
                        }
                    }
                    self.events_scratch.push(gui_ev);
                }
                EngineEvent::NoteOff { key, .. } => {
                    // Delay NoteOff by 45ms (~2160 samples at 48kHz) to filter out OS auto-repeat releases
                    let debounce_samples = (self.engine.sample_rate * 0.045) as usize;
                    self.pending_gui_note_offs.insert(key, debounce_samples);
                }
                _ => {
                    self.events_scratch.push(gui_ev);
                }
            }
        }

        // Process pending NoteOff events whose debounce window has elapsed (finger actually released)
        if !self.pending_gui_note_offs.is_empty() {
            let mut expired_keys = Vec::new();
            for (&k, remaining) in self.pending_gui_note_offs.iter_mut() {
                if *remaining <= num_samples {
                    expired_keys.push(k);
                } else {
                    *remaining -= num_samples;
                }
            }
            for k in expired_keys {
                self.pending_gui_note_offs.remove(&k);
                self.events_scratch.push(EngineEvent::NoteOff { time: 0, key: k });
            }
        }

        // Sync parameter changes only on change to avoid overriding incoming MIDI CC 64/67
        let sustain_val = self.params.sustain_pedal.value();
        if (sustain_val - self.prev_sustain).abs() > 1e-4 {
            self.prev_sustain = sustain_val;
            self.engine.set_sustain_pedal(sustain_val > 0.01, sustain_val as f64);
        }

        let una_val = self.params.una_corda.value();
        if una_val != self.prev_una_corda {
            self.prev_una_corda = una_val;
            self.engine.set_una_corda(una_val);
        }

        // Voicing & Physical parameters
        self.engine.set_inharmonicity_scale(self.params.inharmonicity_scale.value() as f64);
        self.engine.set_hammer_hardness(self.params.hammer_hardness.value() as f64);
        self.engine.set_unison_detuning(self.params.unison_detuning.value() as f64);
        self.engine.set_phantom_gain(self.params.phantom_gain.value() as f64);
        self.engine.set_velocity_curve(self.params.velocity_curve.value() as f64);

        // Tier 6: Micro-mechanical noise gains
        self.engine.set_key_noise_gain(self.params.key_noise.value() as f64);
        self.engine.set_damper_noise_gain(self.params.damper_noise.value() as f64);
        self.engine.set_pedal_noise_gain(self.params.pedal_noise.value() as f64);

        // Tier 7: Spatial Multi-Microphone gains & Lid Baffle
        let close_g = util::db_to_gain(self.params.mic_close.value());
        let player_g = util::db_to_gain(self.params.mic_player.value());
        let amb_g = util::db_to_gain(self.params.mic_ambient.value());
        self.engine.set_mic_gains(close_g as f64, player_g as f64, amb_g as f64);
        self.engine.set_lid_angle(self.params.lid_angle.value() as f64);

        // 3. Step Physical Simulation
        self.engine.process_block(
            num_samples,
            &self.events_scratch,
            &mut self.out_events_scratch,
            &mut self.scratch_l[..num_samples],
            &mut self.scratch_r[..num_samples],
        );

/// Transparent tanh-based soft limiter ensuring output never exceeds 0.99 (0.0 dBFS)
#[inline]
fn soft_limit(x: f32) -> f32 {
    if x.abs() <= 0.82 {
        x
    } else {
        let sign = x.signum();
        let mag = x.abs();
        let excess = mag - 0.82;
        sign * (0.82 + 0.17 * (excess / 0.17).tanh())
    }
}

        // 4. Mix to output buffer with master gain, polyphony headroom scale (0.08), and soft limiter
        let total_gain = self.params.master_gain.value() * 0.08f32;
        let mut max_l = 0.0f32;
        let mut max_r = 0.0f32;

        let channel_slices = buffer.as_slice();
        let (ch_left, ch_right) = channel_slices.split_at_mut(1);
        let left_out = &mut ch_left[0];
        let right_out = &mut ch_right[0];

        for s in 0..num_samples {
            let out_l = soft_limit(self.scratch_l[s] as f32 * total_gain);
            let out_r = soft_limit(self.scratch_r[s] as f32 * total_gain);
            left_out[s] = out_l;
            right_out[s] = out_r;
            max_l = max_l.max(out_l.abs());
            max_r = max_r.max(out_r.abs());
        }

        // Update peak meter smoothly
        let prev_l = self.peak_l.load(std::sync::atomic::Ordering::Relaxed);
        let prev_r = self.peak_r.load(std::sync::atomic::Ordering::Relaxed);
        self.peak_l.store(prev_l * 0.92 + max_l * 0.08, std::sync::atomic::Ordering::Relaxed);
        self.peak_r.store(prev_r * 0.92 + max_r * 0.08, std::sync::atomic::Ordering::Relaxed);

        // Sync physically depressed keys for GUI keyboard visualization using lock-free atomics
        let mut low_mask = 0u64;
        let mut high_mask = 0u64;
        for &k in &self.engine.depressed_keys {
            if (21..85).contains(&k) {
                low_mask |= 1u64 << (k - 21);
            } else if (85..=108).contains(&k) {
                high_mask |= 1u64 << (k - 85);
            }
        }
        self.active_keys_low.store(low_mask, Ordering::Relaxed);
        self.active_keys_high.store(high_mask, Ordering::Relaxed);

        if let Some(mut vels) = self.key_velocities.try_write() {
            for k in 21..=108u8 {
                let idx = (k - 21) as usize;
                if !self.engine.depressed_keys.contains(&k) {
                    vels[idx] = 0.0;
                }
            }
        }

        // Sync bridge dual-polarization orbit for oscilloscope visualizer
        if let Some(mut orb_t) = self.recent_orbit_t.try_write() {
            if let Some(mut orb_p) = self.recent_orbit_p.try_write() {
                orb_t.clear();
                orb_p.clear();
                let step = (num_samples / 48).max(1);
                for i in (0..num_samples).step_by(step) {
                    orb_t.push((self.scratch_l[i] as f32).clamp(-1.0, 1.0));
                    orb_p.push((self.scratch_r[i] as f32).clamp(-1.0, 1.0));
                }
            }
        }

        ProcessStatus::Normal
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        create_vizia_piano_editor(
            self.params.clone(),
            self.peak_l.clone(),
            self.peak_r.clone(),
            self.active_keys_low.clone(),
            self.active_keys_high.clone(),
            self.key_velocities.clone(),
            self.recent_orbit_t.clone(),
            self.recent_orbit_p.clone(),
            self.language.clone(),
            self.gui_event_tx.clone(),
            self.preset_manager.clone(),
            self.undo_manager.clone(),
            self.params.editor_state.clone(),
        )
    }
}

impl ClapPlugin for PhysicsPiano {
    const CLAP_ID: &'static str = "org.eu.mumulhl.physics-piano";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("First-principles physical modeling acoustic piano synthesizer");
    const CLAP_MANUAL_URL: Option<&'static str> =
        Some("https://github.com/mumu-lhl/physics-piano-experiment#readme");
    const CLAP_SUPPORT_URL: Option<&'static str> =
        Some("https://github.com/mumu-lhl/physics-piano-experiment/issues");
    const CLAP_FEATURES: &'static [ClapFeature] =
        &[ClapFeature::Instrument, ClapFeature::Synthesizer];
}

impl Vst3Plugin for PhysicsPiano {
    const VST3_CLASS_ID: [u8; 16] = *b"PhysicsPianoMumu";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[
        Vst3SubCategory::Instrument,
        Vst3SubCategory::Synth,
        Vst3SubCategory::Stereo,
    ];
}

