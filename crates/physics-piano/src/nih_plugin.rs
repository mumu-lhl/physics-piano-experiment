//! Native CLAP Plugin & Standalone Synthesizer built with nih-plug and egui.

use nih_plug::prelude::*;
use nih_plug_egui::{
    create_egui_editor,
    egui::{self, Color32, FontId, RichText, Vec2},
    EguiState,
};
use std::collections::HashSet;
use std::num::NonZeroU32;
use std::sync::Arc;

use crate::engine::{EngineEvent, EngineOutEvent, PianoEngine};
use crate::gui::controls::{render_lissajous_scope, render_vu_meter};
use crate::gui::keyboard::PianoKeyboardWidget;

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

    /// Master Volume Gain [-30 dB ~ +6 dB]
    #[id = "gain"]
    pub master_gain: FloatParam,
}

impl Default for PhysicsPianoParams {
    fn default() -> Self {
        Self {
            editor_state: EguiState::from_size(980, 480),

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

    // Real-time visualization state shared with GUI
    peak_l: Arc<AtomicF32>,
    peak_r: Arc<AtomicF32>,
    active_keys: Arc<parking_lot::RwLock<HashSet<u8>>>,
    recent_orbit_t: Arc<parking_lot::RwLock<Vec<f32>>>,
    recent_orbit_p: Arc<parking_lot::RwLock<Vec<f32>>>,

    // Scratch buffers
    events_scratch: Vec<EngineEvent>,
    out_events_scratch: Vec<EngineOutEvent>,
    scratch_l: Vec<f64>,
    scratch_r: Vec<f64>,
}

impl Default for PhysicsPiano {
    fn default() -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        Self {
            params: Arc::new(PhysicsPianoParams::default()),
            engine: PianoEngine::new(48000.0, 35, true),
            gui_event_tx: tx,
            gui_event_rx: rx,
            peak_l: Arc::new(AtomicF32::new(0.0)),
            peak_r: Arc::new(AtomicF32::new(0.0)),
            active_keys: Arc::new(parking_lot::RwLock::new(HashSet::with_capacity(32))),
            recent_orbit_t: Arc::new(parking_lot::RwLock::new(vec![0.0; 64])),
            recent_orbit_p: Arc::new(parking_lot::RwLock::new(vec![0.0; 64])),
            events_scratch: Vec::with_capacity(64),
            out_events_scratch: Vec::with_capacity(64),
            scratch_l: vec![0.0; 512],
            scratch_r: vec![0.0; 512],
        }
    }
}

impl Plugin for PhysicsPiano {
    const NAME: &'static str = "Physics Piano";
    const VENDOR: &'static str = "mumu-lhl";
    const URL: &'static str = "https://github.com/mumu-lhl/physics-piano-experiment";
    const EMAIL: &'static str = "info@example.com";
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
        true
    }

    fn reset(&mut self) {
        self.engine.active_keys.clear();
        self.engine.voices.clear();
        self.active_keys.write().clear();
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

        // Sync parameter changes
        let sustain_val = self.params.sustain_pedal.value();
        self.engine.set_sustain_pedal(sustain_val > 0.01, sustain_val as f64);
        self.engine.set_una_corda(self.params.una_corda.value());

        // 3. Step Physical Simulation
        self.engine.process_block(
            num_samples,
            &self.events_scratch,
            &mut self.out_events_scratch,
            &mut self.scratch_l[..num_samples],
            &mut self.scratch_r[..num_samples],
        );

        // 4. Mix to output buffer with master gain
        let gain = self.params.master_gain.value();
        let mut max_l = 0.0f32;
        let mut max_r = 0.0f32;

        let channel_slices = buffer.as_slice();
        let (ch_left, ch_right) = channel_slices.split_at_mut(1);
        let left_out = &mut ch_left[0];
        let right_out = &mut ch_right[0];

        for s in 0..num_samples {
            let out_l = (self.scratch_l[s] as f32) * gain;
            let out_r = (self.scratch_r[s] as f32) * gain;
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

        // Sync physically depressed keys for GUI keyboard visualization
        {
            let mut keys = self.active_keys.write();
            keys.clear();
            for &k in &self.engine.depressed_keys {
                keys.insert(k);
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
        let params = self.params.clone();
        let peak_l = self.peak_l.clone();
        let peak_r = self.peak_r.clone();
        let active_keys_arc = self.active_keys.clone();
        let orbit_t_arc = self.recent_orbit_t.clone();
        let orbit_p_arc = self.recent_orbit_p.clone();
        let gui_tx = self.gui_event_tx.clone();

        create_egui_editor(
            self.params.editor_state.clone(),
            Option::<u8>::None,
            |_, _| {},
            move |egui_ctx, setter, held_mouse_key| {
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE.fill(Color32::from_rgb(18, 19, 24)))
                    .show(egui_ctx, |ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(12.0, 10.0);

                        // Header Bar
                        ui.horizontal(|ui| {
                            ui.heading(
                                RichText::new("PHYSICS PIANO")
                                    .font(FontId::proportional(22.0))
                                    .color(Color32::from_rgb(255, 215, 120))
                                    .strong(),
                            );
                            ui.label(
                                RichText::new("Acoustic Grand Physical Modeling Synthesizer (Rust Engine)")
                                    .font(FontId::proportional(12.0))
                                    .color(Color32::from_rgb(150, 155, 170)),
                            );

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

                        // Voicing & Physical Parameter Rack
                        ui.group(|ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal_wrapped(|ui| {
                                ui.vertical(|ui| {
                                    ui.label(RichText::new("PEDALS").strong().color(Color32::from_rgb(200, 205, 220)));
                                    ui.add(
                                        nih_plug_egui::widgets::ParamSlider::for_param(&params.sustain_pedal, setter)
                                            .with_width(120.0),
                                    );
                                    let mut una = params.una_corda.value();
                                    if ui.checkbox(&mut una, "Una Corda (Soft)").changed() {
                                        setter.set_parameter(&params.una_corda, una);
                                    }
                                });

                                ui.separator();

                                ui.vertical(|ui| {
                                    ui.label(RichText::new("STRING & HAMMER").strong().color(Color32::from_rgb(200, 205, 220)));
                                    ui.horizontal(|ui| {
                                        ui.vertical(|ui| {
                                            ui.label("Inharmonicity B:");
                                            ui.add(nih_plug_egui::widgets::ParamSlider::for_param(&params.inharmonicity_scale, setter).with_width(100.0));
                                        });
                                        ui.vertical(|ui| {
                                            ui.label("Hammer Hardness:");
                                            ui.add(nih_plug_egui::widgets::ParamSlider::for_param(&params.hammer_hardness, setter).with_width(100.0));
                                        });
                                    });
                                });

                                ui.separator();

                                ui.vertical(|ui| {
                                    ui.label(RichText::new("ACOUSTICS").strong().color(Color32::from_rgb(200, 205, 220)));
                                    ui.horizontal(|ui| {
                                        ui.vertical(|ui| {
                                            ui.label("Unison Detune:");
                                            ui.add(nih_plug_egui::widgets::ParamSlider::for_param(&params.unison_detuning, setter).with_width(100.0));
                                        });
                                        ui.vertical(|ui| {
                                            ui.label("Phantom Partials:");
                                            ui.add(nih_plug_egui::widgets::ParamSlider::for_param(&params.phantom_gain, setter).with_width(100.0));
                                        });
                                    });
                                });

                                ui.separator();

                                ui.vertical(|ui| {
                                    ui.label(RichText::new("OUTPUT").strong().color(Color32::from_rgb(200, 205, 220)));
                                    ui.add(nih_plug_egui::widgets::ParamSlider::for_param(&params.master_gain, setter).with_width(120.0));
                                });
                            });
                        });

                        ui.add_space(8.0);

                        // 88-Key Interactive Piano Keyboard
                        ui.group(|ui| {
                            let keys_read = active_keys_arc.read();
                            let mut kb = PianoKeyboardWidget::new(&*keys_read, held_mouse_key);

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
                                if i.key_pressed(key) {
                                    let _ = gui_tx.send(EngineEvent::NoteOn {
                                        time: 0,
                                        key: midi,
                                        velocity: 0.85,
                                    });
                                }
                                if i.key_released(key) {
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
                                RichText::new("Tip: Click or drag on keys to play (vertical position = velocity). QWERTY keys: A, W, S, E, D, F, T, G, Y, H, U, J, K")
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
    const CLAP_ID: &'static str = "com.mumu.physics-piano";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("First-principles physical modeling acoustic piano synthesizer");
    const CLAP_MANUAL_URL: Option<&'static str> =
        Some("https://github.com/mumu-lhl/physics-piano-experiment");
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] =
        &[ClapFeature::Instrument, ClapFeature::Synthesizer];
}
