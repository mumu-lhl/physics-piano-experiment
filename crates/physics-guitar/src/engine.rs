//! Real-time 6-string physical modeling guitar engine.
//! Zero allocation during audio loop, supports both acoustic body resonance and electric pickups.

use crate::params::{generate_guitar_string_set, GuitarStringSetType};
use crate::core::guitar_string::GuitarString;
use crate::core::pluck::{PluckExciter, PluckStyle};
use crate::core::fretboard::FretboardRouter;
use crate::core::pickup::{MagneticPickup, PickupType, PickupSelector};
use crate::core::body::AcousticGuitarBody;
use crate::core::amp_cab::GuitarAmpCab;
use crate::core::strummer::{SmartStrummer, StrumPluckEvent};
use crate::core::squeak::FingerSqueakGenerator;
use crate::core::groove::{GrooveEngine, GrooveAction};

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
}

impl GuitarEngine {
    pub fn new(sample_rate: f64, set_type: GuitarStringSetType, mode: GuitarInstrumentMode) -> Self {
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
            pickup: MagneticPickup::new(PickupType::SingleCoil, PickupSelector::Bridge, sample_rate),
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

        // Check if MPE channel (Channel 2..=7) or standard MIDI
        let loc = if (2..=7).contains(&channel) {
            self.router.allocate_mpe_note(channel, midi_note)
        } else {
            self.router.allocate_note(midi_note, &strings_held, &active_frets)
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

            if self.strummer.strum_speed_ms <= 1.0 {
                // Instant direct pluck
                let string = &mut self.strings[str_idx];
                string.set_fret(loc.fret);
                string.palm_mute_depth = self.palm_mute_depth;
                string.pluck(&self.exciter, self.pluck_pos_ratio, velocity);
            } else {
                // Route to smart strummer for chord strumming and picking delays
                self.strings[str_idx].set_fret(loc.fret);
                self.strings[str_idx].palm_mute_depth = self.palm_mute_depth;
                self.strummer.trigger_note(str_idx, loc.fret, velocity);
            }
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

    /// Handles Pitch Bend (e.g. string bending or vibrato).
    pub fn pitch_bend(&mut self, channel: u8, semitones: f64) {
        if (2..=7).contains(&channel) {
            let str_idx = (channel - 2) as usize;
            self.strings[str_idx].set_pitch_bend(semitones);
        } else {
            // Global pitch bend across currently sounding strings
            for s in &mut self.strings {
                if s.is_held {
                    s.set_pitch_bend(semitones);
                }
            }
        }
    }

    /// Sets palm mute depth across all strings [0.0 = ring open, 1.0 = heavy mute].
    pub fn set_palm_mute(&mut self, depth: f64) {
        self.palm_mute_depth = depth.clamp(0.0, 1.0);
        for s in &mut self.strings {
            s.palm_mute_depth = self.palm_mute_depth;
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
        self.exciter = PluckExciter::new(style);
    }

    /// Advances 1 audio sample in hard real-time (zero allocations).
    /// Returns stereo audio frame `(left, right)`.
    #[inline(always)]
    pub fn process_sample(&mut self) -> (f64, f64) {
        // Step groove accompaniment engine if active
        let groove_act = self.groove.step_sample();
        match groove_act {
            GrooveAction::None => {}
            GrooveAction::Strum { direction, velocity, palm_mute_override } => {
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
            GrooveAction::PluckString { string_rel_index, velocity } => {
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
                s.palm_mute_depth = self.palm_mute_depth;
                s.pluck(&self.exciter, self.pluck_pos_ratio, p.velocity);
            }
        }

        let mut total_bridge_t = 0.0;
        let mut pickup_mix = 0.0;

        for (i, s) in self.strings.iter_mut().enumerate() {
            let (f_t, _f_p) = s.step();
            total_bridge_t += f_t;

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

        let mono_out = match self.mode {
            GuitarInstrumentMode::Acoustic => self.body.process(total_bridge_t * 0.35) + squeak_sample * 0.5,
            GuitarInstrumentMode::Electric => {
                let pre_amp = pickup_mix * 2.5 + squeak_sample * 0.35;
                self.amp_cab.process(pre_amp)
            }
        };

        let sample = mono_out * self.master_volume;
        // Stereo output with slight natural room spread
        (sample, sample)
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
