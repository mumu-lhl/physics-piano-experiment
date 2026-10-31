//! Real-time 6-string physical modeling guitar engine.
//! Zero allocation during audio loop, supports both acoustic body resonance and electric pickups.

use crate::core::amp_cab::GuitarAmpCab;
use crate::core::body::AcousticGuitarBody;
use crate::core::chords::ChordShape;
use crate::core::fretboard::FretboardRouter;
use crate::core::groove::{GrooveAction, GrooveEngine};
use crate::core::guitar_string::GuitarString;
use crate::core::pickup::{MagneticPickup, PickupSelector, PickupType};
use crate::core::pluck::{PluckExciter, PluckStyle};
use crate::core::squeak::FingerSqueakGenerator;
use crate::core::strummer::{SmartStrummer, StrumDirection, StrumPluckEvent};
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
    /// Sostenuto (middle pedal, CC66) state: strings sounding when the
    /// pedal went down keep ringing after note-off until it comes up.
    /// `sostenuto_deferred` marks captured strings whose note-off already
    /// arrived while the pedal was down.
    pub sostenuto_down: bool,
    pub sostenuto_strings: [bool; 6],
    pub sostenuto_deferred: [bool; 6],

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
            sostenuto_down: false,
            sostenuto_strings: [false; 6],
            sostenuto_deferred: [false; 6],
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
                    // A mode change re-voices every string from the
                    // global bend; no per-string MPE bend survives it.
                    self.strings[i].mpe_pitch_bend_semitones = None;
                }
            }
        }
    }

    /// Clears all sounding and sequencing state (transport stop /
    /// seek / restart): strings, pending strummer plucks, groove
    /// position, sostenuto, pitch bends, feedback delay line and
    /// amp/cabinet/body/pickup/squeak DSP state. Playing settings
    /// (mode, volumes, knob values, pickup selection, cabinet model)
    /// are preserved.
    pub fn reset(&mut self) {
        for string in &mut self.strings {
            string.reset();
        }
        self.ready_plucks.clear();
        self.strummer.pending_plucks.clear();
        self.strummer.accum_buffer.clear();
        self.strummer.accum_timer = 0;
        self.groove.reset();
        self.active_notes_on_string = [None; 6];
        self.global_pitch_bend = 0.0;
        self.sostenuto_down = false;
        self.sostenuto_strings = [false; 6];
        self.sostenuto_deferred = [false; 6];
        self.feedback_buffer = [0.0; 2048];
        self.feedback_write_idx = 0;
        self.amp_cab.reset();
        // Reset the body in place: rebuilding it heap-allocates the
        // diffusion/dispersion buffers, which the no-allocation guard
        // on the audio thread forbids (this aborted the debug
        // standalone at startup). reset() preserves the settings.
        self.body.reset();
        // The pickup and squeak generator have no reset method:
        // rebuild them fresh, preserving their settings. Neither
        // constructor heap-allocates, so this stays inside the guard.
        let pickup_type = self.pickup.pickup_type;
        let selector = self.pickup.selector;
        let pole_width = self.pickup.pole_width;
        let humbucker_spacing = self.pickup.humbucker_spacing;
        let stratitis_strength = self.pickup.stratitis_strength;
        let rwrp_quack = self.pickup.rwrp_quack;
        let tone_knob = self.pickup.tone_circuit.tone_knob;
        self.pickup = MagneticPickup::new(pickup_type, selector, self.sample_rate);
        self.pickup.pole_width = pole_width;
        self.pickup.humbucker_spacing = humbucker_spacing;
        self.pickup.stratitis_strength = stratitis_strength;
        self.pickup.rwrp_quack = rwrp_quack;
        self.pickup.tone_circuit.set_tone(tone_knob);
        let squeak_level = self.squeak.squeak_level;
        self.squeak = FingerSqueakGenerator::new(self.sample_rate);
        self.squeak.squeak_level = squeak_level;
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
        } else if FretboardRouter::mpe_member_string_index(channel).is_some() {
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
            self.clear_sostenuto_capture(str_idx);

            let is_mpe = FretboardRouter::mpe_member_string_index(channel).is_some();
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
                    string.mpe_pitch_bend_semitones = None;
                }
            } else {
                // Route to smart strummer for chord strumming and picking delays
                self.strings[str_idx].set_fret(loc.fret);
                if !is_mpe {
                    self.strings[str_idx].set_pitch_bend(self.global_pitch_bend);
                    self.strings[str_idx].mpe_pitch_bend_semitones = None;
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

    /// Sostenuto (middle pedal, CC66): pressing captures the strings
    /// sounding at that moment; releasing applies their deferred
    /// note-offs. Explicit `release_string` calls bypass it.
    pub fn set_sostenuto_pedal(&mut self, down: bool) {
        if down == self.sostenuto_down {
            return;
        }
        self.sostenuto_down = down;
        if down {
            for i in 0..6 {
                self.sostenuto_strings[i] = self.active_notes_on_string[i].is_some();
                self.sostenuto_deferred[i] = false;
            }
        } else {
            for i in 0..6 {
                if self.sostenuto_deferred[i] {
                    self.strings[i].release();
                    self.active_notes_on_string[i] = None;
                }
                self.sostenuto_strings[i] = false;
                self.sostenuto_deferred[i] = false;
            }
        }
    }

    /// A fresh strike on a string supersedes any sostenuto capture:
    /// notes started after the pedal went down are not held by it.
    /// Every excitation path (MIDI note-on, strummer drain, groove
    /// pluck, GUI strike) must clear the capture for the struck
    /// string, or pedal-up later releases a freshly sounding string.
    pub(crate) fn clear_sostenuto_capture(&mut self, str_idx: usize) {
        self.sostenuto_strings[str_idx] = false;
        self.sostenuto_deferred[str_idx] = false;
    }

    /// Release a string, deferring to the sostenuto pedal when it holds
    /// this string: the note-off is remembered and applied on pedal-up.
    fn release_or_defer(&mut self, str_idx: usize) {
        if self.sostenuto_down && self.sostenuto_strings[str_idx] {
            self.sostenuto_deferred[str_idx] = true;
        } else {
            self.strings[str_idx].release();
            self.active_notes_on_string[str_idx] = None;
        }
    }

    /// Handles MIDI Note Off event.
    pub fn note_off(&mut self, channel: u8, midi_note: u8) {
        if let Some(str_idx) = FretboardRouter::mpe_member_string_index(channel) {
            self.release_or_defer(str_idx);
        } else {
            for i in 0..6 {
                if self.active_notes_on_string[i] == Some(midi_note) {
                    self.release_or_defer(i);
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

    /// Strums a chord-library shape. The direction applies to this strum
    /// only: it is restored afterwards, like the groove player does.
    /// Muted strings are untouched. Allocation-free: the notes are built
    /// in a fixed array, not via [`ChordShape::engine_notes`].
    pub fn strum_chord_shape(
        &mut self,
        shape: &ChordShape,
        velocity: f64,
        direction: StrumDirection,
    ) {
        // Engine order is high → low (index 0 = high E); shapes are stored
        // low → high, the same reversal as `ChordShape::engine_notes`.
        let mut notes = [(0usize, 0u8, 0.0f64); 6];
        let mut count = 0;
        for engine_idx in 0..6 {
            if let Some(fret) = shape.frets[5 - engine_idx] {
                notes[count] = (engine_idx, fret, velocity);
                count += 1;
            }
        }
        let prev_direction = self.strummer.direction;
        self.strummer.direction = direction;
        self.strummer.trigger_chord(&notes[..count]);
        self.strummer.direction = prev_direction;
    }

    /// Handles Pitch Bend (e.g. string bending, whammy bar, or vibrato).
    /// Supports both per-string MPE (0-based member channels 1..=6)
    /// and standard single-channel MIDI keyboards (master channel 0,
    /// or any channel with no string: 7..=15, treated as global).
    /// A member-channel bend is remembered per string (see
    /// `mpe_pitch_bend_semitones`) so the strummer drain can
    /// re-apply it instead of the global bend.
    pub fn pitch_bend(&mut self, channel: u8, semitones: f64) {
        // Clamped to ±24 semitones like bass; the string layer clamps
        // too, but the stored global/MPE values must be clamped here
        // or the strummer drain would keep re-applying the raw value.
        let semitones = semitones.clamp(-24.0, 24.0);
        if let Some(str_idx) = FretboardRouter::mpe_member_string_index(channel) {
            let string = &mut self.strings[str_idx];
            string.set_pitch_bend(semitones);
            string.mpe_pitch_bend_semitones = Some(semitones);
        } else {
            // Standard single-channel MIDI mode: apply to all strings and store global state
            self.global_pitch_bend = semitones;
            for s in &mut self.strings {
                s.set_pitch_bend(semitones);
                s.mpe_pitch_bend_semitones = None;
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
                    self.clear_sostenuto_capture(target_idx);
                    let s = &mut self.strings[target_idx];
                    s.palm_mute_depth = self.palm_mute_depth;
                    s.pluck(&self.exciter, self.pluck_pos_ratio, velocity);
                }
            }
        }

        // Step smart strummer and pluck any ready strings
        self.strummer.step_into(&mut self.ready_plucks);
        let global_bend = self.global_pitch_bend;
        for p in self.ready_plucks.drain(..) {
            if p.string_index < 6 {
                // Same as `clear_sostenuto_capture`, as direct field
                // writes: `drain(..)` borrows `self` for the loop,
                // so the helper (which takes `&mut self`) cannot be
                // called here.
                self.sostenuto_strings[p.string_index] = false;
                self.sostenuto_deferred[p.string_index] = false;
                let s = &mut self.strings[p.string_index];
                s.set_fret(p.fret);
                // Re-apply this string's own MPE bend when it has one:
                // a strummed note must not fall back to the global
                // bend. Skip the (modal-operator-rebuilding) call when
                // the string already carries the desired bend.
                let bend = s.mpe_pitch_bend_semitones.unwrap_or(global_bend);
                if s.pitch_bend_semitones != bend {
                    s.set_pitch_bend(bend);
                }
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
                    // Bound the closed feedback loop. The pressure is
                    // read post tube-amp + cabinet, where peaking
                    // filters can exceed unity, so clamp it to a
                    // finite range (a non-finite value becomes
                    // silence) and attenuate the coupling: with
                    // `feedback_gain` in [0, 1] (see
                    // `set_acoustic_feedback`) and the amp saturating
                    // in tanh stages, the loop gain stays below unity
                    // in the stable region (feedback_gain <= 1,
                    // drive <= 1). The clamp bounds the worst case
                    // instead of letting high feedback + high drive
                    // howl or run away.
                    let delayed_pressure = if delayed_pressure.is_finite() {
                        delayed_pressure.clamp(-2.0, 2.0)
                    } else {
                        0.0
                    };
                    self.feedback_write_idx = (self.feedback_write_idx + 1) % 2048;

                    let fb_coupling = self.feedback_gain * 0.04;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::amp_cab::CabinetModel;
    use crate::core::convolution::CabinetIrProfile;
    use crate::core::groove::{GrooveAction, GroovePattern};

    fn new_engine() -> GuitarEngine {
        GuitarEngine::new(
            44100.0,
            GuitarStringSetType::Electric010,
            GuitarInstrumentMode::Electric,
        )
    }

    #[test]
    fn mpe_channels_route_to_correct_strings() {
        let mut engine = new_engine();
        engine.strummer.set_strum_speed_ms(0.0);

        // Member channels 1..=6 address strings 1..=6.
        engine.note_on(1, 64, 0.9);
        assert_eq!(engine.active_notes_on_string[0], Some(64));
        engine.note_on(6, 45, 0.9);
        assert_eq!(engine.active_notes_on_string[5], Some(45));
        assert_eq!(engine.strings[5].current_fret, 5);

        // Member-channel note-off releases only its own string.
        engine.note_off(6, 45);
        assert_eq!(engine.active_notes_on_string[5], None);
        assert!(!engine.strings[5].is_held);
        assert_eq!(engine.active_notes_on_string[0], Some(64));

        // Global note-off (master channel) releases by note number.
        engine.note_off(0, 64);
        assert_eq!(engine.active_notes_on_string[0], None);

        // Member-channel bend affects only its string.
        engine.pitch_bend(1, 3.0);
        assert_eq!(engine.strings[0].pitch_bend_semitones, 3.0);
        assert!(engine.strings[1..]
            .iter()
            .all(|s| s.pitch_bend_semitones == 0.0));

        // Master-channel bend applies to all strings.
        engine.pitch_bend(0, 1.0);
        assert!(engine
            .strings
            .iter()
            .all(|s| s.pitch_bend_semitones == 1.0));
        assert_eq!(engine.global_pitch_bend, 1.0);

        // Channel 7 has no string: its bend is global, not string 6's.
        engine.pitch_bend(7, 2.0);
        assert_eq!(engine.global_pitch_bend, 2.0);
        assert!(engine
            .strings
            .iter()
            .all(|s| s.pitch_bend_semitones == 2.0));
    }

    #[test]
    fn set_cabinet_model_through_engine_changes_cabinet() {
        let mut engine = new_engine();
        engine.set_cabinet_model(CabinetModel::Greenback);
        assert_eq!(engine.amp_cab.cabinet.model, CabinetModel::Greenback);
        assert_eq!(
            engine.amp_cab.ir_engine.profile,
            CabinetIrProfile::MarshallGreenback
        );
    }

    #[test]
    fn strummer_drain_preserves_mpe_pitch_bend() {
        let mut engine = new_engine();
        engine.pitch_bend(0, 0.5);
        engine.pitch_bend(2, 2.0);
        assert_eq!(engine.strings[1].pitch_bend_semitones, 2.0);
        assert_eq!(engine.strings[1].mpe_pitch_bend_semitones, Some(2.0));

        // MPE note on channel 2 -> string 2, via the strummer.
        engine.note_on(2, 59, 0.9);
        assert_eq!(engine.strings[1].pitch_bend_semitones, 2.0);

        // Run until the strummer drains the pluck.
        let mut drained = false;
        for _ in 0..2000 {
            engine.process_sample();
            if engine.strings[1].is_held {
                drained = true;
                break;
            }
        }
        assert!(drained, "strummer never drained the pluck");
        assert_eq!(
            engine.strings[1].pitch_bend_semitones, 2.0,
            "strummer drain overwrote the MPE bend with the global bend"
        );
        assert_eq!(engine.strings[1].mpe_pitch_bend_semitones, Some(2.0));
    }

    #[test]
    fn non_mpe_note_on_clears_mpe_pitch_bend() {
        let mut engine = new_engine();
        engine.pitch_bend(0, 0.5);
        engine.note_on(2, 59, 0.9);
        engine.pitch_bend(2, 2.0);
        // Re-striking the same note from the master channel reuses the
        // string as a non-MPE note: the global bend must win.
        engine.note_on(0, 59, 0.9);
        assert_eq!(engine.strings[1].pitch_bend_semitones, 0.5);
        assert_eq!(engine.strings[1].mpe_pitch_bend_semitones, None);

        let mut drained = false;
        for _ in 0..2000 {
            engine.process_sample();
            if engine.strings[1].is_held {
                drained = true;
                break;
            }
        }
        assert!(drained, "strummer never drained the pluck");
        assert_eq!(engine.strings[1].pitch_bend_semitones, 0.5);
    }

    #[test]
    fn engine_reset_clears_sound_groove_and_sostenuto() {
        let mut engine = new_engine();
        engine.strummer.set_strum_speed_ms(0.0);
        // Channel 6 -> string 6, whose open note is E2 (40).
        engine.note_on(6, 40, 0.9);
        for _ in 0..100 {
            engine.process_sample();
        }
        assert!(engine.strings.iter().any(|s| s.total_energy() > 0.0));
        engine.set_sostenuto_pedal(true);
        assert!(engine.sostenuto_down);

        // Advance the groove past its first step.
        engine.groove.set_pattern(GroovePattern::FolkStrum44);
        let mut actions = 0;
        for _ in 0..200_000 {
            if !matches!(engine.groove.step_sample(), GrooveAction::None) {
                actions += 1;
                if actions == 2 {
                    break;
                }
            }
        }
        assert_eq!(actions, 2, "groove did not advance");

        // State the reset must also clear: an MPE bend, a queued
        // strummer pluck and a filled feedback delay line.
        engine.pitch_bend(2, 2.0);
        engine.strummer.set_strum_speed_ms(18.0);
        engine.note_on(0, 64, 0.9);
        engine.set_acoustic_feedback(0.5, 4.5);
        for _ in 0..3000 {
            engine.process_sample();
        }
        assert!(!engine.strummer.accum_buffer.is_empty()
            || !engine.strummer.pending_plucks.is_empty()
            || !engine.ready_plucks.is_empty()
            || engine.strings.iter().any(|s| s.is_held));

        engine.reset();

        assert!(engine.strings.iter().all(|s| s.total_energy() == 0.0));
        assert!(engine.strings.iter().all(|s| !s.is_held));
        assert_eq!(engine.active_notes_on_string, [None; 6]);
        assert!(!engine.sostenuto_down);
        assert_eq!(engine.sostenuto_strings, [false; 6]);
        assert_eq!(engine.sostenuto_deferred, [false; 6]);
        assert_eq!(engine.global_pitch_bend, 0.0);
        assert!(engine
            .strings
            .iter()
            .all(|s| s.pitch_bend_semitones == 0.0
                && s.mpe_pitch_bend_semitones.is_none()));
        assert!(engine.strummer.pending_plucks.is_empty());
        assert!(engine.strummer.accum_buffer.is_empty());
        assert_eq!(engine.strummer.accum_timer, 0);
        assert!(engine.ready_plucks.is_empty());
        assert!(engine.feedback_buffer.iter().all(|&v| v == 0.0));
        assert_eq!(engine.feedback_write_idx, 0);

        // The groove restarts from step 0: the first action is the
        // down-strum at velocity 0.95.
        for _ in 0..200_000 {
            match engine.groove.step_sample() {
                GrooveAction::None => {}
                GrooveAction::Strum {
                    direction,
                    velocity,
                    ..
                } => {
                    assert_eq!(direction, crate::core::strummer::StrumDirection::Down);
                    assert!((velocity - 0.95).abs() < 1e-9);
                    break;
                }
                other => panic!("unexpected first groove action after reset: {other:?}"),
            }
        }

        // Silence: no held strings, cleared DSP state.
        for _ in 0..512 {
            let (l, r) = engine.process_sample();
            assert_eq!(l, 0.0);
            assert_eq!(r, 0.0);
        }
    }

    #[test]
    fn sostenuto_capture_cleared_by_strummer_drain() {
        let mut engine = new_engine();
        engine.note_on(0, 64, 0.9);
        let str_idx = engine
            .active_notes_on_string
            .iter()
            .position(|&n| n == Some(64))
            .expect("note allocated to a string");
        for _ in 0..3000 {
            engine.process_sample();
            if engine.strings[str_idx].is_held {
                break;
            }
        }
        assert!(engine.strings[str_idx].is_held);

        engine.set_sostenuto_pedal(true);
        engine.note_off(0, 64);
        assert!(engine.sostenuto_deferred[str_idx]);
        assert!(engine.strings[str_idx].is_held);

        // Re-strike through the strummer: the drain must clear the
        // sostenuto capture, or pedal-up releases this fresh note.
        engine.note_on(0, 64, 0.9);
        let mut cleared = false;
        for _ in 0..4000 {
            engine.process_sample();
            if !engine.sostenuto_strings[str_idx] && !engine.sostenuto_deferred[str_idx] {
                cleared = true;
                break;
            }
        }
        assert!(cleared, "strummer drain did not clear the sostenuto capture");

        engine.set_sostenuto_pedal(false);
        for _ in 0..100 {
            engine.process_sample();
        }
        assert!(
            engine.strings[str_idx].is_held,
            "re-struck string was released on pedal-up"
        );
        assert_eq!(engine.active_notes_on_string[str_idx], Some(64));
        assert!(engine.strings[str_idx].total_energy() > 1e-6);
    }

    #[test]
    fn sostenuto_capture_cleared_by_groove_pluck() {
        let mut engine = new_engine();
        engine.strummer.set_strum_speed_ms(0.0);
        engine.note_on(0, 64, 0.9);
        let str_idx = engine
            .active_notes_on_string
            .iter()
            .position(|&n| n == Some(64))
            .expect("note allocated to a string");
        for _ in 0..100 {
            engine.process_sample();
        }
        assert!(engine.strings[str_idx].is_held);

        engine.set_sostenuto_pedal(true);
        engine.note_off(0, 64);
        assert!(engine.sostenuto_deferred[str_idx]);

        // BalladArp68's first step is a PluckString on a held string.
        engine.groove.set_pattern(GroovePattern::BalladArp68);
        engine.groove.set_bpm(240.0);
        let mut cleared = false;
        for _ in 0..6000 {
            engine.process_sample();
            if !engine.sostenuto_strings[str_idx] && !engine.sostenuto_deferred[str_idx] {
                cleared = true;
                break;
            }
        }
        assert!(cleared, "groove pluck did not clear the sostenuto capture");

        engine.set_sostenuto_pedal(false);
        for _ in 0..100 {
            engine.process_sample();
        }
        assert!(
            engine.strings[str_idx].is_held,
            "groove-plucked string was released on pedal-up"
        );
        assert_eq!(engine.active_notes_on_string[str_idx], Some(64));
    }

    #[test]
    fn feedback_loop_stays_bounded_at_max_gain_and_drive() {
        let mut engine = new_engine();
        engine.strummer.set_strum_speed_ms(0.0);
        engine.set_acoustic_feedback(1.0, 4.5);
        engine.amp_cab.drive = 1.0;
        engine.note_on(6, 40, 1.0);
        for n in 0..88_200 {
            let (l, r) = engine.process_sample();
            assert!(l.is_finite() && r.is_finite(), "non-finite output at {n}");
            assert!(l.abs() < 1.0 && r.abs() < 1.0, "output exceeded soft limit at {n}");
            if n % 1024 == 0 {
                assert!(
                    engine
                        .strings
                        .iter()
                        .flat_map(|s| s
                            .state_t
                            .iter()
                            .chain(s.state_p.iter())
                            .flat_map(|st| [st.q, st.v]))
                        .all(|v| v.is_finite()),
                    "non-finite modal state at {n}"
                );
            }
        }
        let energy = engine.strings.iter().map(|s| s.total_energy()).sum::<f64>();
        assert!(energy.is_finite() && energy < 1e12, "feedback energy ran away: {energy}");
        assert!(engine.feedback_buffer.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn pitch_bend_clamped_to_plus_minus_24_semitones() {
        let mut engine = new_engine();
        engine.pitch_bend(0, 100.0);
        assert_eq!(engine.global_pitch_bend, 24.0);
        assert!(engine
            .strings
            .iter()
            .all(|s| s.pitch_bend_semitones == 24.0));

        engine.pitch_bend(3, -100.0);
        assert_eq!(engine.strings[2].pitch_bend_semitones, -24.0);
        assert_eq!(engine.strings[2].mpe_pitch_bend_semitones, Some(-24.0));
        assert!(engine
            .strings
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != 2)
            .all(|(_, s)| s.pitch_bend_semitones == 24.0));
    }
}

#[cfg(all(test, debug_assertions))]
mod no_alloc_regression_tests {
    // Regression test for the debug-standalone startup abort:
    // GuitarEngine::reset() runs under nice-plug's no-allocation
    // guard, and rebuilding AcousticGuitarBody there heap-allocated
    // the diffusion/dispersion buffers, aborting the process. In
    // test builds the guard only counts violations (warn_debug).
    use super::*;
    use nice_assert_no_alloc::{assert_no_alloc, violation_count};

    #[test]
    fn reset_does_not_allocate() {
        let mut engine = GuitarEngine::new(
            44100.0,
            GuitarStringSetType::Electric010,
            GuitarInstrumentMode::Electric,
        );
        let before = violation_count();
        assert_no_alloc(|| engine.reset());
        assert_eq!(violation_count(), before);
    }
}
