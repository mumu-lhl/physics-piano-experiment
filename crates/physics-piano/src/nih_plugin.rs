//! Native CLAP Plugin & Standalone Synthesizer built with nih-plug and egui.

use nih_plug::prelude::*;
use nih_plug_egui::{
    create_egui_editor,
    egui::{self, Color32, FontId, RichText, Vec2},
    EguiState,
};
use std::collections::HashSet;
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;

use crate::engine::{EngineEvent, EngineOutEvent, PianoEngine};
use crate::gui::{
    render_lissajous_scope, render_vu_meter,
    PianoKeyboardWidget, I18n, Language, setup_cjk_fonts,
};

#[derive(Params)]
pub struct PhysicsPianoParams {
    #[persist = "editor-state"]
    pub editor_state: Arc<EguiState>,

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
}

impl Default for PhysicsPianoParams {
    fn default() -> Self {
        Self {
            editor_state: EguiState::from_size(1080, 560),

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
    recent_orbit_t: Arc<parking_lot::RwLock<Vec<f32>>>,
    recent_orbit_p: Arc<parking_lot::RwLock<Vec<f32>>>,
    language: Arc<AtomicU8>,           // 0: English, 1: SimplifiedChinese

    // Edge-triggered parameter tracking to prevent overwriting MIDI CC
    prev_sustain: f32,
    prev_una_corda: bool,

    // Scratch buffers
    events_scratch: Vec<EngineEvent>,
    out_events_scratch: Vec<EngineOutEvent>,
    scratch_l: Vec<f64>,
    scratch_r: Vec<f64>,
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
            recent_orbit_t: Arc::new(parking_lot::RwLock::new(vec![0.0; 64])),
            recent_orbit_p: Arc::new(parking_lot::RwLock::new(vec![0.0; 64])),
            language: Arc::new(AtomicU8::new(lang_code)),
            prev_sustain: 0.0,
            prev_una_corda: false,
            events_scratch: Vec::with_capacity(64),
            out_events_scratch: Vec::with_capacity(64),
            scratch_l: vec![0.0; 512],
            scratch_r: vec![0.0; 512],
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
        true
    }

    fn reset(&mut self) {
        self.engine.reset();
        self.active_keys_low.store(0, Ordering::Relaxed);
        self.active_keys_high.store(0, Ordering::Relaxed);
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
                    }
                }
                _ => {}
            }
        }

        // 2. Consume events triggered by on-screen virtual keyboard
        while let Ok(gui_ev) = self.gui_event_rx.try_recv() {
            self.events_scratch.push(gui_ev);
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
            let out_l = soft_limit((self.scratch_l[s] as f32) * total_gain);
            let out_r = soft_limit((self.scratch_r[s] as f32) * total_gain);
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
        let params = self.params.clone();
        let peak_l = self.peak_l.clone();
        let peak_r = self.peak_r.clone();
        let keys_low_arc = self.active_keys_low.clone();
        let keys_high_arc = self.active_keys_high.clone();
        let orbit_t_arc = self.recent_orbit_t.clone();
        let orbit_p_arc = self.recent_orbit_p.clone();
        let language_arc = self.language.clone();
        let gui_tx = self.gui_event_tx.clone();

        struct GuiKeyboardState {
            held_mouse_key: Option<u8>,
            held_qwerty_keys: HashSet<egui::Key>,
            language: Language,
        }

        let initial_lang = if self.language.load(Ordering::Relaxed) == 1 {
            Language::SimplifiedChinese
        } else {
            Language::English
        };

        create_egui_editor(
            self.params.editor_state.clone(),
            GuiKeyboardState {
                held_mouse_key: None,
                held_qwerty_keys: HashSet::new(),
                language: initial_lang,
            },
            |egui_ctx, _gui_state| {
                setup_cjk_fonts(egui_ctx);
            },
            move |egui_ctx, setter, gui_state| {
                let lang = gui_state.language;
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE.fill(Color32::from_rgb(18, 19, 24)))
                    .show(egui_ctx, |ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(12.0, 10.0);

                        // Header Bar
                        ui.horizontal(|ui| {
                            ui.heading(
                                RichText::new(I18n::title(lang))
                                    .font(FontId::proportional(22.0))
                                    .color(Color32::from_rgb(255, 215, 120))
                                    .strong(),
                            );
                            ui.label(
                                RichText::new(I18n::subtitle(lang))
                                    .font(FontId::proportional(12.0))
                                    .color(Color32::from_rgb(150, 155, 170)),
                            );

                            // Language Switcher Toggle Button (persists choice)
                            let (btn_text, next_lang) = match lang {
                                Language::English => ("🌐 中文", Language::SimplifiedChinese),
                                Language::SimplifiedChinese => ("🌐 English", Language::English),
                            };
                            if ui
                                .button(
                                    RichText::new(btn_text)
                                        .font(FontId::proportional(12.0))
                                        .color(Color32::from_rgb(210, 220, 240)),
                                )
                                .clicked()
                            {
                                gui_state.language = next_lang;
                                language_arc.store(
                                    if next_lang == Language::SimplifiedChinese { 1 } else { 0 },
                                    Ordering::Relaxed,
                                );
                            }

                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let pl = peak_l.load(std::sync::atomic::Ordering::Relaxed);
                                let pr = peak_r.load(std::sync::atomic::Ordering::Relaxed);
                                render_vu_meter(ui, pl, pr, 32.0);

                                let ot = orbit_t_arc.read();
                                let op = orbit_p_arc.read();
                                render_lissajous_scope(ui, &ot, &op, 34.0);
                            });
                        });

                        ui.separator();

                        // Voicing, Mechanics & Spatial Parameter Rack
                        ui.group(|ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal_wrapped(|ui| {
                                // 1. Pedals & Micro-Mechanics (Tier 6)
                                ui.vertical(|ui| {
                                    ui.label(RichText::new(I18n::rack_pedals(lang)).strong().color(Color32::from_rgb(200, 205, 220)));
                                    ui.horizontal(|ui| {
                                        ui.vertical(|ui| {
                                            ui.label(I18n::sustain(lang));
                                            ui.add(nih_plug_egui::widgets::ParamSlider::for_param(&params.sustain_pedal, setter).with_width(85.0));
                                            let mut una = params.una_corda.value();
                                            if ui.checkbox(&mut una, I18n::una_corda(lang)).changed() {
                                                setter.begin_set_parameter(&params.una_corda);
                                                setter.set_parameter(&params.una_corda, una);
                                                setter.end_set_parameter(&params.una_corda);
                                            }
                                        });
                                        ui.vertical(|ui| {
                                            ui.label(I18n::key_action(lang));
                                            ui.add(nih_plug_egui::widgets::ParamSlider::for_param(&params.key_noise, setter).with_width(80.0));
                                            ui.label(I18n::damper_noise(lang));
                                            ui.add(nih_plug_egui::widgets::ParamSlider::for_param(&params.damper_noise, setter).with_width(80.0));
                                        });
                                        ui.vertical(|ui| {
                                            ui.label(I18n::pedal_shock(lang));
                                            ui.add(nih_plug_egui::widgets::ParamSlider::for_param(&params.pedal_noise, setter).with_width(80.0));
                                        });
                                    });
                                });

                                ui.separator();

                                // 2. String & Hammer Physics
                                ui.vertical(|ui| {
                                    ui.label(RichText::new(I18n::rack_string(lang)).strong().color(Color32::from_rgb(200, 205, 220)));
                                    ui.horizontal(|ui| {
                                        ui.vertical(|ui| {
                                            ui.label(I18n::inharmonicity(lang));
                                            ui.add(nih_plug_egui::widgets::ParamSlider::for_param(&params.inharmonicity_scale, setter).with_width(88.0));
                                            ui.label(I18n::hammer_hardness(lang));
                                            ui.add(nih_plug_egui::widgets::ParamSlider::for_param(&params.hammer_hardness, setter).with_width(88.0));
                                        });
                                        ui.vertical(|ui| {
                                            ui.label(I18n::unison_detune(lang));
                                            ui.add(nih_plug_egui::widgets::ParamSlider::for_param(&params.unison_detuning, setter).with_width(88.0));
                                            ui.label(I18n::phantom_partials(lang));
                                            ui.add(nih_plug_egui::widgets::ParamSlider::for_param(&params.phantom_gain, setter).with_width(88.0));
                                        });
                                    });
                                });

                                ui.separator();

                                // 3. Spatial Multi-Mic & Lid Baffle (Tier 7)
                                ui.vertical(|ui| {
                                    ui.label(RichText::new(I18n::rack_spatial(lang)).strong().color(Color32::from_rgb(200, 205, 220)));
                                    ui.horizontal(|ui| {
                                        ui.vertical(|ui| {
                                            ui.label(I18n::mic_close(lang));
                                            ui.add(nih_plug_egui::widgets::ParamSlider::for_param(&params.mic_close, setter).with_width(80.0));
                                            ui.label(I18n::mic_player(lang));
                                            ui.add(nih_plug_egui::widgets::ParamSlider::for_param(&params.mic_player, setter).with_width(80.0));
                                        });
                                        ui.vertical(|ui| {
                                            ui.label(I18n::mic_ambient(lang));
                                            ui.add(nih_plug_egui::widgets::ParamSlider::for_param(&params.mic_ambient, setter).with_width(80.0));
                                            ui.label(I18n::lid_angle(lang));
                                            ui.add(nih_plug_egui::widgets::ParamSlider::for_param(&params.lid_angle, setter).with_width(80.0));
                                        });
                                    });
                                });

                                ui.separator();

                                // 4. Master Output
                                ui.vertical(|ui| {
                                    ui.label(RichText::new(I18n::rack_output(lang)).strong().color(Color32::from_rgb(200, 205, 220)));
                                    ui.label(I18n::master_gain(lang));
                                    ui.add(nih_plug_egui::widgets::ParamSlider::for_param(&params.master_gain, setter).with_width(110.0));
                                });
                            });
                        });

                        ui.add_space(8.0);

                        // 88-Key Interactive Piano Keyboard (100% Lock-Free query)
                        ui.group(|ui| {
                            let low = keys_low_arc.load(Ordering::Relaxed);
                            let high = keys_high_arc.load(Ordering::Relaxed);
                            let is_active = move |k: u8| -> bool {
                                if (21..85).contains(&k) {
                                    (low & (1u64 << (k - 21))) != 0
                                } else if (85..=108).contains(&k) {
                                    (high & (1u64 << (k - 85))) != 0
                                } else {
                                    false
                                }
                            };

                            let mut kb = PianoKeyboardWidget::new(&is_active, &mut gui_state.held_mouse_key);

                            let avail_w = ui.available_width();
                            let kb_h = 135.0f32;
                            kb.show(ui, avail_w, kb_h);

                            // Send triggered notes from on-screen keyboard
                            for (note, vel) in kb.pressed_keys {
                                let _ = gui_tx.send(EngineEvent::NoteOn {
                                    time: 0,
                                    key: note,
                                    velocity: vel as f64,
                                });
                            }
                            for note in kb.released_keys {
                                let _ = gui_tx.send(EngineEvent::NoteOff {
                                    time: 0,
                                    key: note,
                                });
                            }
                        });

                        // Handle QWERTY laptop keyboard input (C4 to C5 octave)
                        // Uses stateful tracking (gui_state.held_qwerty_keys) to strictly suppress
                        // OS auto-repeat events when holding keys down.
                        const QWERTY_KEYS: &[(egui::Key, u8)] = &[
                            (egui::Key::A, 60), // C4
                            (egui::Key::W, 61), // C#4
                            (egui::Key::S, 62), // D4
                            (egui::Key::E, 63), // D#4
                            (egui::Key::D, 64), // E4
                            (egui::Key::F, 65), // F4
                            (egui::Key::T, 66), // F#4
                            (egui::Key::G, 67), // G4
                            (egui::Key::Y, 68), // G#4
                            (egui::Key::H, 69), // A4
                            (egui::Key::U, 70), // A#4
                            (egui::Key::J, 71), // B4
                            (egui::Key::K, 72), // C5
                        ];

                        egui_ctx.input(|i| {
                            for &(key, midi) in QWERTY_KEYS {
                                let is_down = i.key_down(key);
                                let was_down = gui_state.held_qwerty_keys.contains(&key);

                                if is_down && !was_down {
                                    gui_state.held_qwerty_keys.insert(key);
                                    let _ = gui_tx.send(EngineEvent::NoteOn {
                                        time: 0,
                                        key: midi,
                                        velocity: 0.85,
                                    });
                                } else if !is_down && was_down {
                                    gui_state.held_qwerty_keys.remove(&key);
                                    let _ = gui_tx.send(EngineEvent::NoteOff {
                                        time: 0,
                                        key: midi,
                                    });
                                }
                            }
                        });

                        // Footer with laptop QWERTY keyboard hints
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(I18n::keyboard_hint(lang))
                                    .font(FontId::proportional(11.0))
                                    .color(Color32::from_rgb(110, 115, 130)),
                            );
                        });
                    });

                // Request continuous repaint for smooth VU meters and active key animations
                egui_ctx.request_repaint();
            },
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

