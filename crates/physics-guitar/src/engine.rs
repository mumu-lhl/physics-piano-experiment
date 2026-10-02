//! Real-time 6-string physical modeling guitar engine.
//! Zero allocation during audio loop, supports both acoustic body resonance and electric pickups.

use crate::core::amp_cab::GuitarAmpCab;
use crate::core::body::AcousticGuitarBody;
use crate::core::fretboard::FretboardRouter;
use crate::core::groove::{GrooveAction, GrooveEngine};
use crate::core::guitar_string::GuitarString;
use crate::core::pickup::{MagneticPickup, PickupSelector, PickupType};
use crate::core::pluck::{PluckExciter, PluckStyle};
use crate::core::squeak::FingerSqueakGenerator;
use crate::core::strummer::{SmartStrummer, StrumPluckEvent};
use crate::params::{GuitarStringSetType, generate_guitar_string_set};

const ELECTRIC_OUTPUT_TRIM: f64 = 0.5;
const ACOUSTIC_OUTPUT_TRIM: f64 = 0.05;
const SOFT_LIMIT_KNEE: f64 = 0.9;

#[inline]
fn soft_limit(sample: f64) -> f64 {
    let magnitude = sample.abs();
    if magnitude <= SOFT_LIMIT_KNEE {
        sample
    } else {
        sample.signum()
            * (SOFT_LIMIT_KNEE
                + (1.0 - SOFT_LIMIT_KNEE)
                    * ((magnitude - SOFT_LIMIT_KNEE) / (1.0 - SOFT_LIMIT_KNEE)).tanh())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuitarInstrumentMode {
    Acoustic,
    Electric,
}

#[derive(Debug, Clone)]
pub struct GuitarEngine {
    pub sample_rate: f64,
    pub dt: f64,
    pub mode: GuitarInstrumentMode,

    /// The 6 strings: index 0 = string 1 (high E), index 5 = string 6 (low E)
    pub strings: Vec<GuitarString>,
    pub exciter: PluckExciter,
    pub router: FretboardRouter,

    // Electric guitar pickup & passive RLC tone network
    pub pickup: MagneticPickup,
    // Tube amp and 12-inch cabinet
    pub amp_cab: GuitarAmpCab,
    // Acoustic guitar body
    pub body: AcousticGuitarBody,
    // Smart strummer
    pub strummer: SmartStrummer,
    ready_plucks: Vec<StrumPluckEvent>,

    // Physical acoustics: Finger squeak & Wound string friction
    pub squeak: FingerSqueakGenerator,
    // Accompaniment groove pattern engine
    pub groove: GrooveEngine,

    // Performance parameters
    pub pluck_pos_ratio: f64,
    pub palm_mute_depth: f64,
    pub master_volume: f64,

    // Tracking active notes for release
    pub active_notes_on_string: [Option<u8>; 6],
    /// Global pitch bend in semitones for standard single-channel MIDI keyboards
    pub global_pitch_bend: f64,

    // Acoustic feedback closed-loop
    pub feedback_gain: f64,
    pub feedback_delay_ms: f64,
    feedback_buffer: [f64; 2048],
    feedback_write_idx: usize,
}

impl GuitarEngine {
    pub fn new(
        sample_rate: f64,
        set_type: GuitarStringSetType,
        mode: GuitarInstrumentMode,
    ) -> Self {
        let dt = 1.0 / sample_rate;
        let string_params = generate_guitar_string_set(set_type, 30);
        let mut strings = Vec::with_capacity(6);

        for p in string_params {
            strings.push(GuitarString::new(p, sample_rate));
        }

        let exciter = match mode {
            GuitarInstrumentMode::Electric => PluckExciter::new(PluckStyle::Plectrum),
            GuitarInstrumentMode::Acoustic => PluckExciter::new(PluckStyle::FingerFlesh),
        };

        Self {
            sample_rate,
            dt,
            mode,
            strings,
            exciter,
            router: FretboardRouter::new(),
            pickup: MagneticPickup::new(
                PickupType::SingleCoil,
                PickupSelector::Bridge,
                sample_rate,
            ),
            amp_cab: GuitarAmpCab::new(sample_rate),
            body: AcousticGuitarBody::new(sample_rate),
            strummer: SmartStrummer::new(sample_rate),
            ready_plucks: Vec::with_capacity(16),
            squeak: FingerSqueakGenerator::new(sample_rate),
            groove: GrooveEngine::new(sample_rate),
            pluck_pos_ratio: 0.15,
            palm_mute_depth: 0.0,
            master_volume: 0.85,
            active_notes_on_string: [None; 6],
            global_pitch_bend: 0.0,
            feedback_gain: 0.0,
            feedback_delay_ms: 4.5,
            feedback_buffer: [0.0; 2048],
            feedback_write_idx: 0,
        }
    }

    /// Changes the instrument mode (Acoustic vs Electric) and dynamically reloads
    /// authentic physical string properties (Phosphor Bronze .012 vs Nickel Steel .010),
    /// preserving active fretting state.
    pub fn set_mode(&mut self, mode: GuitarInstrumentMode) {
        if self.mode != mode {
            self.mode = mode;
            let set_type = match mode {
                GuitarInstrumentMode::Acoustic => GuitarStringSetType::Acoustic012,
                GuitarInstrumentMode::Electric => GuitarStringSetType::Electric010,
            };
            let string_params = generate_guitar_string_set(set_type, 30);
            for (i, p) in string_params.into_iter().enumerate() {
                if i < self.strings.len() {
                    let fret = self.strings[i].current_fret;
                    self.strings[i].params = p;
                    self.strings[i].set_fret(fret);
                    self.strings[i].set_pitch_bend(self.global_pitch_bend);
                }
            }
        }
    }

    /// Handles MIDI Note On event.
    pub fn note_on(&mut self, channel: u8, midi_note: u8, velocity: f64) {
        if velocity <= 0.0 {
            self.note_off(channel, midi_note);
            return;
        }

        let mut strings_held = [false; 6];
        let mut active_frets = [None; 6];
        for i in 0..6 {
            strings_held[i] = self.strings[i].is_held;
            if self.strings[i].is_held {
                active_frets[i] = Some(self.strings[i].current_fret);
            }
        }

        // If this exact MIDI note is already sounding on a string, reuse that same string!
        // This avoids orphaning the currently sounding string and creating stuck/dangling notes.
        let existing_string = self
            .active_notes_on_string
            .iter()
            .position(|&opt| opt == Some(midi_note));

        let loc = if let Some(str_idx) = existing_string {
            Some(crate::core::fretboard::FretboardLocation {
                string_index: (str_idx + 1) as u8,
                fret: self.strings[str_idx].current_fret,
            })
        } else if (2..=7).contains(&channel) {
            self.router.allocate_mpe_note(channel, midi_note)
        } else {
            self.router
                .allocate_note(midi_note, &strings_held, &active_frets)
        };

        if let Some(loc) = loc {
            let str_idx = (loc.string_index - 1) as usize;
            let prev_fret = self.strings[str_idx].current_fret;
            let delta = (loc.fret as i16 - prev_fret as i16).abs() as u8;
            if delta >= 2 && str_idx >= 3 {
                // Wound strings (D, A, low E) trigger acoustic squeak during position shifts
                self.squeak.trigger_shift(str_idx, delta, 1.0);
            }
            self.active_notes_on_string[str_idx] = Some(midi_note);

            let is_mpe = (2..=7).contains(&channel);
            if self.strummer.strum_speed_ms <= 1.0 {
                // Instant direct pluck with physical legato if string is already ringing
                let string = &mut self.strings[str_idx];
                let is_legato = string.is_held && string.current_fret != loc.fret && string.total_energy() > 1e-6;
                if is_legato {
                    string.legato_fret(loc.fret, velocity);
                } else {
                    string.set_fret(loc.fret);
                    string.palm_mute_depth = self.palm_mute_depth;
                    string.pluck(&self.exciter, self.pluck_pos_ratio, velocity);
                }
                if !is_mpe {
                    string.set_pitch_bend(self.global_pitch_bend);
                }
            } else {
                // Route to smart strummer for chord strumming and picking delays
                self.strings[str_idx].set_fret(loc.fret);
                if !is_mpe {
                    self.strings[str_idx].set_pitch_bend(self.global_pitch_bend);
                }
                self.strings[str_idx].palm_mute_depth = self.palm_mute_depth;
                self.strummer.trigger_note(str_idx, loc.fret, velocity);
            }
        }
    }

    /// Performs a physical legato transition (hammer-on or pull-off) on a specific string.
    pub fn legato_to_fret(&mut self, string_index: usize, new_fret: u8, velocity: f64) {
        if string_index < 6 {
            let open_note = self.router.open_notes[string_index];
            self.strings[string_index].legato_fret(new_fret, velocity);
            self.active_notes_on_string[string_index] = Some(open_note + new_fret);
        }
    }

    /// Initiates a continuous legato slide on a string.
    pub fn slide_string(&mut self, string_index: usize, target_fret: u8, duration_ms: f64) {
        if string_index < 6 {
            let open_note = self.router.open_notes[string_index];
            self.strings[string_index].start_slide(target_fret, duration_ms);
            self.active_notes_on_string[string_index] = Some(open_note + target_fret);
        }
    }

    /// Plays a natural harmonic on a string (node 2 = 12th fret, node 3 = 7th fret, node 4 = 5th fret).
    pub fn play_natural_harmonic(&mut self, string_index: usize, node: u8, velocity: f64) {
        if string_index < 6 {
            self.strings[string_index].trigger_natural_harmonic(&self.exciter, node, velocity);
            let open_note = self.router.open_notes[string_index];
            self.active_notes_on_string[string_index] = Some(open_note);
        }
    }

    /// Plays a rock/metal pinch harmonic on a string (high overtone squeal).
    pub fn play_pinch_harmonic(&mut self, string_index: usize, node_ratio: f64, velocity: f64) {
        if string_index < 6 {
            self.strings[string_index].trigger_pinch_harmonic(&self.exciter, node_ratio, velocity);
            let open_note = self.router.open_notes[string_index];
            self.active_notes_on_string[string_index] =
                Some(open_note + self.strings[string_index].current_fret);
        }
    }

    /// Plays a tap harmonic at specified fret above fretted note.
    pub fn play_tap_harmonic(&mut self, string_index: usize, tap_node_fret: u8, velocity: f64) {
        if string_index < 6 {
            self.strings[string_index].trigger_tap_harmonic(tap_node_fret, velocity);
        }
    }

    /// Handles MIDI Note Off event.
    pub fn note_off(&mut self, channel: u8, midi_note: u8) {
        if (2..=7).contains(&channel) {
            let str_idx = (channel - 2) as usize;
            self.strings[str_idx].release();
            self.active_notes_on_string[str_idx] = None;
        } else {
            for i in 0..6 {
                if self.active_notes_on_string[i] == Some(midi_note) {
                    self.strings[i].release();
                    self.active_notes_on_string[i] = None;
                }
            }
        }
    }

    /// Releases a specific string (1..=6) directly.
    pub fn release_string(&mut self, string_index: u8) {
        if (1..=6).contains(&string_index) {
            let str_idx = (string_index - 1) as usize;
            self.strings[str_idx].release();
            self.active_notes_on_string[str_idx] = None;
        }
    }

    /// Handles Pitch Bend (e.g. string bending, whammy bar, or vibrato).
    /// Supports both per-string MPE (channels 2..=7) and standard single-channel
    /// MIDI keyboards (channel 0 or 1, or global broadcast).
    pub fn pitch_bend(&mut self, channel: u8, semitones: f64) {
        if (2..=7).contains(&channel) {
            let str_idx = (channel - 2) as usize;
            self.strings[str_idx].set_pitch_bend(semitones);
        } else {
            // Standard single-channel MIDI mode: apply to all strings and store global state
            self.global_pitch_bend = semitones;
            for s in &mut self.strings {
                s.set_pitch_bend(semitones);
            }
        }
    }

    /// Sets palm mute depth across all strings [0.0 = ring open, 1.0 = heavy mute].
    pub fn set_palm_mute(&mut self, depth: f64) {
        let depth = depth.clamp(0.0, 1.0);
        if (self.palm_mute_depth - depth).abs() <= 1e-4 {
            return;
        }
        self.palm_mute_depth = depth;
        for s in &mut self.strings {
            s.palm_mute_depth = depth;
            s.recalculate_modal_operators();
        }
    }

    /// Sets passive tone circuit knob [0.0 = dark, 1.0 = bright open].
    pub fn set_tone(&mut self, tone: f64) {
        self.pickup.set_tone(tone);
    }

    /// Sets fret buzz sensitivity [0.0 = clean, 1.0 = heavy buzz].
    pub fn set_fret_buzz(&mut self, sensitivity: f64) {
        let sens = sensitivity.clamp(0.0, 1.0);
        for s in &mut self.strings {
            s.fret_buzz_sensitivity = sens;
        }
    }

    /// Sets pluck style (Finger vs Plectrum).
    pub fn set_pluck_style(&mut self, style: PluckStyle) {
        if self.exciter.style != style {
            self.exciter = PluckExciter::new(style);
        }
    }

    /// Sets acoustic feedback gain and air-propagation delay line distance in ms.
    pub fn set_acoustic_feedback(&mut self, gain: f64, delay_ms: f64) {
        self.feedback_gain = gain.clamp(0.0, 1.0);
        self.feedback_delay_ms = delay_ms.clamp(0.5, 25.0);
    }

    /// Sets electric guitar speaker cabinet model.
    pub fn set_cabinet_model(&mut self, model: crate::core::amp_cab::CabinetModel) {
        self.amp_cab.set_cabinet_model(model);
    }

    /// Sets virtual microphone placement distance in cm (proximity effect).
    pub fn set_mic_distance(&mut self, distance_cm: f64) {
        self.amp_cab.set_mic_distance(distance_cm);
    }

    /// Enables or disables RWRP reverse-wound phase cancellation in 2 & 4 quack positions.
    pub fn set_rwrp_quack(&mut self, quack: bool) {
        self.pickup.set_rwrp_quack(quack);
    }

    /// Advances 1 audio sample in hard real-time (zero allocations).
    /// Returns stereo audio frame `(left, right)`.
    #[inline(always)]
    pub fn process_sample(&mut self) -> (f64, f64) {
        // Step groove accompaniment engine if active
        let groove_act = self.groove.step_sample();
        match groove_act {
            GrooveAction::None => {}
            GrooveAction::Strum {
                direction,
                velocity,
                palm_mute_override,
            } => {
                let mut count = 0;
                let mut notes = [(0usize, 0u8, 0.0f64); 6];
                for (i, s) in self.strings.iter().enumerate() {
                    if s.is_held {
                        notes[count] = (i, s.current_fret, velocity);
                        count += 1;
                    }
                }
                if count > 0 {
                    if let Some(pm) = palm_mute_override {
                        for s in &mut self.strings {
                            s.palm_mute_depth = pm;
                        }
                    } else {
                        for s in &mut self.strings {
                            s.palm_mute_depth = self.palm_mute_depth;
                        }
                    }
                    let prev_dir = self.strummer.direction;
                    self.strummer.direction = direction;
                    self.strummer.trigger_chord(&notes[..count]);
                    self.strummer.direction = prev_dir;
                }
            }
            GrooveAction::PluckString {
                string_rel_index,
                velocity,
            } => {
                let mut held = [0usize; 6];
                let mut count = 0;
                for (i, s) in self.strings.iter().enumerate() {
                    if s.is_held {
                        held[count] = i;
                        count += 1;
                    }
                }
                if count > 0 {
                    let target_idx = held[string_rel_index % count];
                    let s = &mut self.strings[target_idx];
                    s.palm_mute_depth = self.palm_mute_depth;
                    s.pluck(&self.exciter, self.pluck_pos_ratio, velocity);
                }
            }
        }

        // Step smart strummer and pluck any ready strings
        self.strummer.step_into(&mut self.ready_plucks);
        for p in self.ready_plucks.drain(..) {
            if p.string_index < 6 {
                let s = &mut self.strings[p.string_index];
                s.set_fret(p.fret);
                s.set_pitch_bend(self.global_pitch_bend);
                s.palm_mute_depth = self.palm_mute_depth;
                s.pluck(&self.exciter, self.pluck_pos_ratio, p.velocity);
            }
        }

        let mut total_bridge_t = 0.0;
        let mut total_bridge_p = 0.0;
        let mut pickup_mix = 0.0;

        for (i, s) in self.strings.iter_mut().enumerate() {
            let (f_t, f_p) = s.step();
            total_bridge_t += f_t;
            total_bridge_p += f_p;

            if self.mode == GuitarInstrumentMode::Electric {
                let emf = self.pickup.sample_string(s);
                pickup_mix += emf;
            }
            // Auto release if energy drops below threshold
            if s.is_held && s.total_energy() < 1e-6 {
                s.is_held = false;
                self.active_notes_on_string[i] = None;
            }
        }

        // Tactile finger squeak noise across wound strings
        let squeak_sample = self.squeak.process_sample();

        let (out_l, out_r) = match self.mode {
            GuitarInstrumentMode::Acoustic => {
                // Bridge rocking torque: horizontal shear couples into soundboard as ~18% effective normal force
                // (produces authentic two-stage decay: punchy vertical attack + lingering horizontal sustain)
                let effective_bridge_force = total_bridge_t + 0.18 * total_bridge_p;
                let (body_l, body_r) = self.body.process_stereo(effective_bridge_force * 0.35);

                // Inter-string sympathetic resonance: bridge velocity drives unmuted strings across soundboard
                let v_br = self.body.bridge_velocity();
                let sym_coupling = 0.00025;
                for s in &mut self.strings {
                    s.inject_bridge_motion(v_br, sym_coupling);
                }

                // Spatial stereo field: soundhole/body spread + neck squeak placed naturally towards left
                let squeak_l = squeak_sample * 0.6 * 0.5;
                let squeak_r = squeak_sample * 0.4 * 0.5;

                (
                    (body_l + squeak_l) * ACOUSTIC_OUTPUT_TRIM,
                    (body_r + squeak_r) * ACOUSTIC_OUTPUT_TRIM,
                )
            }
            GuitarInstrumentMode::Electric => {
                let pre_amp = pickup_mix * 2.5 + squeak_sample * 0.35;
                let amp_out = self.amp_cab.process(pre_amp);

                // Acoustic feedback closed loop: sound pressure radiates through air and drives strings
                if self.feedback_gain > 0.0 {
                    self.feedback_buffer[self.feedback_write_idx] = amp_out;
                    let delay_samples = (self.feedback_delay_ms * 0.001 * self.sample_rate).clamp(1.0, 2040.0);
                    let read_idx_float = (self.feedback_write_idx as f64 + 2048.0 - delay_samples) % 2048.0;
                    let idx0 = read_idx_float.floor() as usize % 2048;
                    let idx1 = (idx0 + 1) % 2048;
                    let frac = read_idx_float - read_idx_float.floor();
                    let delayed_pressure = self.feedback_buffer[idx0] * (1.0 - frac) + self.feedback_buffer[idx1] * frac;
                    self.feedback_write_idx = (self.feedback_write_idx + 1) % 2048;

                    let fb_coupling = self.feedback_gain * 0.08;
                    for s in &mut self.strings {
                        if s.is_held || s.total_energy() > 1e-7 {
                            s.inject_acoustic_pressure(delayed_pressure, fb_coupling);
                        }
                    }
                }

                (
                    amp_out * ELECTRIC_OUTPUT_TRIM,
                    amp_out * ELECTRIC_OUTPUT_TRIM,
                )
            }
        };

        (
            soft_limit(out_l * self.master_volume),
            soft_limit(out_r * self.master_volume),
        )
    }

    /// Renders an audio block into left and right channel slices.
    pub fn process_block(&mut self, left: &mut [f32], right: &mut [f32]) {
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            let (sl, sr) = self.process_sample();
            *l = sl as f32;
            *r = sr as f32;
        }
    }
}
