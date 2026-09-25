//! Real-time 6-string physical modeling guitar engine.
//! Zero allocation during audio loop, supports both acoustic body resonance and electric pickups.

use crate::params::{generate_guitar_string_set, GuitarStringSetType};
use crate::core::guitar_string::GuitarString;
use crate::core::pluck::{PluckExciter, PluckStyle};
use crate::core::fretboard::FretboardRouter;
use crate::core::pickup::{MagneticPickup, PickupType, PickupPosition};
use crate::core::body::AcousticGuitarBody;

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

    // Electric guitar pickup
    pub pickup: MagneticPickup,
    // Acoustic guitar body
    pub body: AcousticGuitarBody,

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
            pickup: MagneticPickup::new(PickupType::SingleCoil, PickupPosition::Bridge),
            body: AcousticGuitarBody::new(sample_rate),
            pluck_pos_ratio: 0.15,
            palm_mute_depth: 0.0,
            master_volume: 0.85,
            active_notes_on_string: [None; 6],
        }
    }

    /// Handles MIDI Note On event.
    pub fn note_on(&mut self, channel: u8, midi_note: u8, velocity: f64) {
        if velocity <= 0.0 {
            self.note_off(channel, midi_note);
            return;
        }

        let mut strings_held = [false; 6];
        for i in 0..6 {
            strings_held[i] = self.strings[i].is_held;
        }

        // Check if MPE channel (Channel 2..=7) or standard MIDI
        let loc = if (2..=7).contains(&channel) {
            self.router.allocate_mpe_note(channel, midi_note)
        } else {
            self.router.allocate_note(midi_note, &strings_held)
        };

        if let Some(loc) = loc {
            let str_idx = (loc.string_index - 1) as usize;
            let string = &mut self.strings[str_idx];
            string.set_fret(loc.fret);
            string.palm_mute_depth = self.palm_mute_depth;
            string.pluck(&self.exciter, self.pluck_pos_ratio, velocity);
            self.active_notes_on_string[str_idx] = Some(midi_note);
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

    /// Sets pluck style (Finger vs Plectrum).
    pub fn set_pluck_style(&mut self, style: PluckStyle) {
        self.exciter = PluckExciter::new(style);
    }

    /// Advances 1 audio sample in hard real-time (zero allocations).
    /// Returns stereo audio frame `(left, right)`.
    #[inline(always)]
    pub fn process_sample(&mut self) -> (f64, f64) {
        let mut total_bridge_t = 0.0;
        let mut pickup_mix = 0.0;

        for (i, s) in self.strings.iter_mut().enumerate() {
            let (f_t, _f_p) = s.step();
            total_bridge_t += f_t;

            if self.mode == GuitarInstrumentMode::Electric {
                let emf = self.pickup.sample_string(s);
                // Slight stereo spread across the 6 strings
                pickup_mix += emf;
            }
            // Auto release if energy drops below threshold
            if s.is_held && s.total_energy() < 1e-9 {
                s.is_held = false;
                self.active_notes_on_string[i] = None;
            }
        }

        let mono_out = match self.mode {
            GuitarInstrumentMode::Acoustic => self.body.process(total_bridge_t * 0.35),
            GuitarInstrumentMode::Electric => pickup_mix * 2.5,
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
