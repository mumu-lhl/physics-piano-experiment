//! Native CLAP Plugin & Standalone Synthesizer built with nih-plug and egui for Physics Guitar.

use nih_plug::prelude::*;
use nih_plug_egui::{
    create_egui_editor,
    egui::{self, Color32, FontId, RichText, Vec2},
    EguiState,
};
use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};
use std::sync::Arc;
use parking_lot::Mutex;

use crate::engine::{GuitarEngine, GuitarInstrumentMode};
use crate::params::GuitarStringSetType;
use crate::core::pluck::PluckStyle;
use crate::core::pickup::{PickupType, PickupPosition};
use crate::gui::GuitarFretboardWidget;

#[derive(Params)]
pub struct PhysicsGuitarParams {
    #[persist = "editor-state"]
    pub editor_state: Arc<EguiState>,

    /// Instrument Mode: 0 = Electric, 1 = Acoustic
    #[id = "mode"]
    pub mode: IntParam,

    /// Pluck Exciter Style: 0 = Plectrum / Pick, 1 = Finger Flesh
    #[id = "pluck_style"]
    pub pluck_style: IntParam,

    /// Electric Guitar Pickup Position: 0 = Bridge, 1 = Middle, 2 = Neck
    #[id = "pickup_pos"]
    pub pickup_pos: IntParam,

    /// Electric Guitar Pickup Type: 0 = Single-Coil, 1 = Humbucker
    #[id = "pickup_type"]
    pub pickup_type: IntParam,

    /// Palm Mute Depth [0.0 ~ 1.0] (0% = Open ring, 100% = Tight chug)
    #[id = "palmmute"]
    pub palm_mute: FloatParam,

    /// Pluck Position along string [0.05 ~ 0.35] (Sul Ponticello to Sul Tasto)
    #[id = "pluckpos"]
    pub pluck_pos: FloatParam,

    /// Master Output Gain [-30 dB ~ +6 dB]
    #[id = "gain"]
    pub master_gain: FloatParam,
}

impl Default for PhysicsGuitarParams {
    fn default() -> Self {
        Self {
            editor_state: EguiState::from_size(960, 480),

            mode: IntParam::new("Instrument Mode", 0, IntRange::Linear { min: 0, max: 1 }),
            pluck_style: IntParam::new("Pluck Style", 0, IntRange::Linear { min: 0, max: 1 }),
            pickup_pos: IntParam::new("Pickup Position", 0, IntRange::Linear { min: 0, max: 2 }),
            pickup_type: IntParam::new("Pickup Type", 1, IntRange::Linear { min: 0, max: 1 }), // default Humbucker

            palm_mute: FloatParam::new(
                "Palm Mute",
                0.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_unit(" %")
            .with_value_to_string(formatters::v2s_f32_percentage(0))
            .with_string_to_value(formatters::s2v_f32_percentage()),

            pluck_pos: FloatParam::new(
                "Pluck Position",
                0.15,
                FloatRange::Linear { min: 0.05, max: 0.35 },
            )
            .with_unit(" L")
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
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_gain_to_db(1))
            .with_string_to_value(formatters::s2v_f32_gain_to_db()),
        }
    }
}

pub struct PhysicsGuitar {
    pub params: Arc<PhysicsGuitarParams>,
    pub engine: GuitarEngine,
    pub sample_rate: f32,

    // Shared state for GUI animation (atomic / lock-free)
    pub active_frets_shared: Arc<[AtomicU8; 6]>,
    pub string_energies_shared: Arc<[AtomicU32; 6]>,
    // Thread-safe trigger queue from GUI clicks into audio engine
    pub gui_trigger_queue: Arc<Mutex<Vec<(u8, u8)>>>, // (string 1..=6, fret 0..=24)
}

impl Default for PhysicsGuitar {
    fn default() -> Self {
        let sample_rate = 44100.0;
        let engine = GuitarEngine::new(
            sample_rate,
            GuitarStringSetType::Electric010,
            GuitarInstrumentMode::Electric,
        );

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

        Self {
            params: Arc::new(PhysicsGuitarParams::default()),
            engine,
            sample_rate: sample_rate as f32,
            active_frets_shared,
            string_energies_shared,
            gui_trigger_queue: Arc::new(Mutex::new(Vec::with_capacity(16))),
        }
    }
}

impl Plugin for PhysicsGuitar {
    const NAME: &'static str = "Physics Guitar";
    const VENDOR: &'static str = "Mumulhl";
    const URL: &'static str = "https://github.com/mumu-lhl/physics-piano-experiment";
    const EMAIL: &'static str = "mumulhl@example.com";
    const VERSION: &'static str = "0.1.0";

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
        self.engine = GuitarEngine::new(
            self.sample_rate as f64,
            GuitarStringSetType::Electric010,
            GuitarInstrumentMode::Electric,
        );
        true
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // 1. Process GUI manual fret clicks from queue
        {
            let mut queue = self.gui_trigger_queue.lock();
            for &(str_idx, fret) in queue.iter() {
                if (1..=6).contains(&str_idx) {
                    let s_i = (str_idx - 1) as usize;
                    let open_note = self.engine.router.open_notes[s_i];
                    let midi_note = open_note + fret;
                    self.engine.strings[s_i].set_fret(fret);
                    self.engine.strings[s_i].pluck(&self.engine.exciter, self.engine.pluck_pos_ratio, 0.85);
                    self.engine.active_notes_on_string[s_i] = Some(midi_note);
                }
            }
            queue.clear();
        }

        // 2. Sync plugin parameters to engine
        let mode_val = self.params.mode.value();
        self.engine.mode = if mode_val == 1 {
            GuitarInstrumentMode::Acoustic
        } else {
            GuitarInstrumentMode::Electric
        };

        let style_val = self.params.pluck_style.value();
        self.engine.set_pluck_style(if style_val == 1 {
            PluckStyle::FingerFlesh
        } else {
            PluckStyle::Plectrum
        });

        let pos_val = self.params.pickup_pos.value();
        self.engine.pickup.position = match pos_val {
            1 => PickupPosition::Middle,
            2 => PickupPosition::Neck,
            _ => PickupPosition::Bridge,
        };

        let type_val = self.params.pickup_type.value();
        self.engine.pickup.pickup_type = if type_val == 1 {
            PickupType::Humbucker
        } else {
            PickupType::SingleCoil
        };

        self.engine.set_palm_mute(self.params.palm_mute.value() as f64);
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
                    NoteEvent::NoteOn { channel, note, velocity, .. } => {
                        self.engine.note_on(channel, note, velocity as f64);
                    }
                    NoteEvent::NoteOff { channel, note, .. } => {
                        self.engine.note_off(channel, note);
                    }
                    NoteEvent::MidiPitchBend { channel, value, .. } => {
                        // 14-bit pitch bend: 8192 is center, map to +/- 12 semitones
                        let bend_semitones = ((value as f64 - 8192.0) / 8192.0) * 12.0;
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
            let fret_val = if string.is_held { string.current_fret } else { 255 };
            self.active_frets_shared[i].store(fret_val, Ordering::Relaxed);

            let energy = (string.total_energy() * 1000.0).clamp(0.0, 1.0) as f32;
            self.string_energies_shared[i].store(energy.to_bits(), Ordering::Relaxed);
        }

        ProcessStatus::Normal
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        let params = self.params.clone();
        let active_frets_shared = self.active_frets_shared.clone();
        let string_energies_shared = self.string_energies_shared.clone();
        let gui_trigger_queue = self.gui_trigger_queue.clone();

        create_egui_editor(
            self.params.editor_state.clone(),
            (),
            |_, _| {},
            move |egui_ctx, setter, _state| {
                egui::CentralPanel::default().show(egui_ctx, |ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(8.0, 8.0);

                    // Top Banner Header
                    ui.horizontal(|ui| {
                        ui.heading(
                            RichText::new("PHYSICS GUITAR")
                                .font(FontId::proportional(22.0))
                                .color(Color32::from_rgb(255, 195, 80))
                                .strong(),
                        );
                        ui.label(
                            RichText::new("FIRST-PRINCIPLES PHYSICAL MODELING VIRTUAL INSTRUMENT")
                                .font(FontId::proportional(12.0))
                                .color(Color32::from_rgb(170, 175, 190)),
                        );
                    });

                    ui.separator();

                    // Mode & Performance Controls Rack
                    ui.horizontal(|ui| {
                        ui.group(|ui| {
                            ui.label(RichText::new("Instrument").strong());
                            let mut mode_idx = params.mode.value();
                            ui.radio_value(&mut mode_idx, 0, "Electric Guitar");
                            ui.radio_value(&mut mode_idx, 1, "Acoustic Guitar");
                            if mode_idx != params.mode.value() {
                                setter.begin_set_parameter(&params.mode);
                                setter.set_parameter(&params.mode, mode_idx);
                                setter.end_set_parameter(&params.mode);
                            }
                        });

                        ui.group(|ui| {
                            ui.label(RichText::new("Pickup Selector").strong());
                            let mut pos_idx = params.pickup_pos.value();
                            ui.horizontal(|ui| {
                                ui.radio_value(&mut pos_idx, 0, "Bridge");
                                ui.radio_value(&mut pos_idx, 1, "Middle");
                                ui.radio_value(&mut pos_idx, 2, "Neck");
                            });
                            if pos_idx != params.pickup_pos.value() {
                                setter.begin_set_parameter(&params.pickup_pos);
                                setter.set_parameter(&params.pickup_pos, pos_idx);
                                setter.end_set_parameter(&params.pickup_pos);
                            }

                            let mut type_idx = params.pickup_type.value();
                            ui.horizontal(|ui| {
                                ui.radio_value(&mut type_idx, 0, "Single-Coil");
                                ui.radio_value(&mut type_idx, 1, "Humbucker");
                            });
                            if type_idx != params.pickup_type.value() {
                                setter.begin_set_parameter(&params.pickup_type);
                                setter.set_parameter(&params.pickup_type, type_idx);
                                setter.end_set_parameter(&params.pickup_type);
                            }
                        });

                        ui.group(|ui| {
                            ui.label(RichText::new("Pluck & Tone").strong());
                            let mut style_idx = params.pluck_style.value();
                            ui.horizontal(|ui| {
                                ui.radio_value(&mut style_idx, 0, "Plectrum");
                                ui.radio_value(&mut style_idx, 1, "Finger");
                            });
                            if style_idx != params.pluck_style.value() {
                                setter.begin_set_parameter(&params.pluck_style);
                                setter.set_parameter(&params.pluck_style, style_idx);
                                setter.end_set_parameter(&params.pluck_style);
                            }

                            let mut mute_val = params.palm_mute.value();
                            if ui.add(egui::Slider::new(&mut mute_val, 0.0..=1.0).text("Palm Mute")).changed() {
                                setter.begin_set_parameter(&params.palm_mute);
                                setter.set_parameter(&params.palm_mute, mute_val);
                                setter.end_set_parameter(&params.palm_mute);
                            }
                        });

                        ui.group(|ui| {
                            ui.label(RichText::new("Master Output").strong());
                            let mut gain_db = util::gain_to_db(params.master_gain.value());
                            if ui.add(egui::Slider::new(&mut gain_db, -30.0..=6.0).text("Gain (dB)")).changed() {
                                let new_gain = util::db_to_gain(gain_db);
                                setter.begin_set_parameter(&params.master_gain);
                                setter.set_parameter(&params.master_gain, new_gain);
                                setter.end_set_parameter(&params.master_gain);
                            }
                        });
                    });

                    ui.add_space(8.0);

                    // Read shared fret states and string vibration energies
                    let mut active_frets = [None; 6];
                    let mut string_energies = [0.0f32; 6];
                    for i in 0..6 {
                        let f = active_frets_shared[i].load(Ordering::Relaxed);
                        if f != 255 {
                            active_frets[i] = Some(f);
                        }
                        string_energies[i] = f32::from_bits(string_energies_shared[i].load(Ordering::Relaxed));
                    }

                    // Interactive Guitar Fretboard Widget
                    ui.label(RichText::new("Interactive 6-String Fretboard (Click or Drag frets to play):").color(Color32::from_rgb(180, 185, 200)));
                    let fretboard_size = Vec2::new(ui.available_width(), 200.0);

                    let mut click_handler = |str_clicked: u8, fret_clicked: u8| {
                        gui_trigger_queue.lock().push((str_clicked, fret_clicked));
                    };

                    GuitarFretboardWidget::new(&active_frets, &string_energies)
                        .with_callback(&mut click_handler)
                        .show(ui, fretboard_size);
                });
            },
        )
    }
}

impl ClapPlugin for PhysicsGuitar {
    const CLAP_ID: &'static str = "org.mumulhl.physics-guitar";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("Physical modeling acoustic & electric guitar synthesizer");
    const CLAP_MANUAL_URL: Option<&'static str> = Some("https://github.com/mumu-lhl/physics-piano-experiment");
    const CLAP_SUPPORT_URL: Option<&'static str> = Some("https://github.com/mumu-lhl/physics-piano-experiment");
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::Instrument,
        ClapFeature::Synthesizer,
        ClapFeature::Stereo,
    ];
}

impl Vst3Plugin for PhysicsGuitar {
    const VST3_CLASS_ID: [u8; 16] = *b"PhysicsGuitarSyn";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[
        Vst3SubCategory::Instrument,
        Vst3SubCategory::Synth,
    ];
}

nih_export_clap!(PhysicsGuitar);
nih_export_vst3!(PhysicsGuitar);
