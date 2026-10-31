use crate::core::action::KeyActionNoise;
use crate::core::bridge::BridgeSoundboard;
use crate::core::pedal::{DamperWhoosh, PlateShock, RestrikeBuzz};
use crate::core::voice::PianoVoice;
use crate::dsp::lid::LidBaffle;
use crate::dsp::simd::initialize_backend;
use crate::dsp::upols::{
    MultiPerspectiveUPOLS, UPOLSConvolver, generate_multi_perspective_soundboard_irs,
};
use crate::params::{KeyParams, generate_grand_piano_parameters};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub enum EngineEvent {
    NoteOn { time: usize, key: u8, velocity: f64 },
    NoteOff { time: usize, key: u8 },
    NoteTuning { time: usize, key: u8, cents: f64 },
    SustainPedal { time: usize, depth: f64 },
    SostenutoPedal { time: usize, down: bool },
    UnaCorda { time: usize, enabled: bool },
    PitchBend { time: usize, cents: f64 },
    Expression { time: usize, gain: f64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineOutEvent {
    NoteEnd { time: usize, key: u8 },
}

/// Fixed input abscissae of the velocity curve. The editor's curve points
/// sit at these x positions; only their y values move.
pub const VELOCITY_CURVE_X: [f64; 5] = [0.0, 0.25, 0.5, 0.75, 1.0];

/// Converts a legacy exponent-style touch curve (-1..1) to the five curve
/// points, so old presets keep their touch response at those points
/// (piecewise-linear in between): each point is the old formula
/// `v^(2^(curve*0.8))` evaluated at its abscissa.
pub fn velocity_points_from_exponent(curve: f64) -> [f64; 5] {
    let exponent = 2.0_f64.powf(curve.clamp(-1.0, 1.0) * 0.8);
    let mut points = [0.0; 5];
    for (i, x) in VELOCITY_CURVE_X.iter().enumerate() {
        points[i] = x.powf(exponent);
    }
    points
}

/// Piecewise-linear velocity mapping through five curve points (kept as a
/// free function so it can be tested without constructing an engine).
pub fn map_velocity_through_points(points: &[f64; 5], velocity: f64) -> f64 {
    let v = velocity.clamp(0.001, 1.0);
    let mut i = 0;
    while i < 4 && v > VELOCITY_CURVE_X[i + 1] {
        i += 1;
    }
    let x0 = VELOCITY_CURVE_X[i];
    let x1 = VELOCITY_CURVE_X[i + 1];
    let t = if x1 > x0 { (v - x0) / (x1 - x0) } else { 0.0 };
    (points[i] + t * (points[i + 1] - points[i])).clamp(0.001, 1.0)
}

pub struct PianoEngine {
    pub sample_rate: f64,
    pub dt: f64,
    pub num_modes: usize,

    pub key_params: HashMap<u8, KeyParams>,
    pub voices: HashMap<u8, PianoVoice>,
    pub active_keys: HashSet<u8>,
    pub depressed_keys: HashSet<u8>,

    pub bridge: BridgeSoundboard,
    pub upols: Option<UPOLSConvolver>,
    pub multi_upols: Option<MultiPerspectiveUPOLS>,
    pub lid_baffle: LidBaffle,
    pub radiation_mode: String,

    pub sustain_pedal: bool,
    pub pedal_depth: f64,
    pub prev_pedal_depth: f64,
    pub una_corda: bool,

    /// Sostenuto (middle pedal) state: keys physically held when the
    /// pedal went down keep sounding after release until it comes up.
    pub sostenuto_pedal: bool,
    pub sostenuto_notes: HashSet<u8>,

    // Core voicing parameters
    pub inharmonicity_scale: f64,
    pub hammer_hardness: f64,
    pub unison_detuning: f64,
    pub phantom_gain: f64,
    pub pitch_bend_cents: f64,
    pub note_tuning_cents: [Option<f64>; 88],
    pub expression_gain: f64,
    /// Velocity-curve output values at the five fixed input abscissae
    /// ([`VELOCITY_CURVE_X`]); the three interior points are user-draggable,
    /// the endpoints are always 0 and 1.
    pub velocity_curve_points: [f64; 5],

    // Tier 6: Micro-Mechanical Action Noise Generators
    pub action_noise: KeyActionNoise,
    pub damper_whoosh: DamperWhoosh,
    pub plate_shock: PlateShock,
    pub restrike_buzz: RestrikeBuzz,

    // Peak energy monitor for -96dB note lifecycle garbage collection
    pub note_energy_peak: HashMap<u8, f64>,

    // Reaction coupling forces from previous step
    pub f_react_t: f64,
    pub f_react_p: f64,

    pub max_active_voices: usize,
    pub active_keys_vec: Vec<u8>,
    pub keys_to_remove_scratch: Vec<u8>,
    /// Voices stolen from the active set, kept sounding under a fast
    /// damper release ramp instead of being reset mid-oscillation (a
    /// hard reset would drop the bridge force discontinuously and click).
    /// Each entry is `(key, remaining gain)`, ramping 1.0 -> 0.0 over
    /// [`RELEASE_RAMP_SECONDS`]; `process_block` keeps stepping these
    /// voices with their bridge contribution scaled by the gain.
    pub releasing_voices: Vec<(u8, f64)>,
}

pub const MAX_ACTIVE_VOICES: usize = 32;

/// Release-ramp duration for a stolen voice: the damper is seated and
/// the bridge-force contribution fades over ~30 ms before the voice is
/// reset.
pub const RELEASE_RAMP_SECONDS: f64 = 0.03;

impl PianoEngine {
    pub fn new(sample_rate: f64, num_modes: usize, stretch_tuning: bool) -> Self {
        initialize_backend();
        let dt = 1.0 / sample_rate;
        let all_params = generate_grand_piano_parameters(num_modes, stretch_tuning);
        let mut key_params = HashMap::with_capacity(88);
        for kp in all_params {
            key_params.insert(kp.midi_note, kp);
        }

        let action_noise = KeyActionNoise::new(sample_rate);
        let damper_whoosh = DamperWhoosh::new(sample_rate);
        let plate_shock = PlateShock::new(sample_rate);
        let restrike_buzz = RestrikeBuzz::new(sample_rate);
        let lid_baffle = LidBaffle::new(sample_rate);

        Self {
            sample_rate,
            dt,
            num_modes,
            key_params,
            voices: HashMap::with_capacity(88),
            active_keys: HashSet::with_capacity(32),
            depressed_keys: HashSet::with_capacity(32),
            bridge: BridgeSoundboard::new(sample_rate),
            upols: None,
            multi_upols: None,
            lid_baffle,
            radiation_mode: "modal".to_string(),
            sustain_pedal: false,
            pedal_depth: 0.0,
            prev_pedal_depth: 0.0,
            una_corda: false,
            sostenuto_pedal: false,
            sostenuto_notes: HashSet::with_capacity(32),
            inharmonicity_scale: 1.0,
            hammer_hardness: 1.0,
            unison_detuning: 1.0,
            phantom_gain: 1.0,
            pitch_bend_cents: 0.0,
            note_tuning_cents: [None; 88],
            expression_gain: 1.0,
            velocity_curve_points: [0.0, 0.25, 0.5, 0.75, 1.0],
            action_noise,
            damper_whoosh,
            plate_shock,
            restrike_buzz,
            note_energy_peak: HashMap::with_capacity(88),
            f_react_t: 0.0,
            f_react_p: 0.0,
            max_active_voices: MAX_ACTIVE_VOICES,
            active_keys_vec: Vec::with_capacity(32),
            keys_to_remove_scratch: Vec::with_capacity(32),
            // Pre-sized for all 88 piano keys (21..=108): entries are
            // deduplicated by key, so stealing can never grow this
            // Vec — and growing it inside process_block would trip
            // the no-allocation guard on the audio thread.
            releasing_voices: Vec::with_capacity(88),
        }
    }

    pub fn set_pitch_bend(&mut self, cents: f64) {
        self.pitch_bend_cents = cents;
        for &key in &self.active_keys {
            let composed = self.composed_pitch_offset(key);
            if let Some(voice) = self.voices.get_mut(&key) {
                voice.set_pitch_bend(composed);
            }
        }
    }

    /// Per-note tuning offset plus the global pitch bend, in cents.
    ///
    /// The two are stored separately (`note_tuning_cents` /
    /// `pitch_bend_cents`) and only composed when handed to a voice:
    /// `PianoVoice` keeps a single composed offset, so passing either
    /// component alone would erase the other. Keys outside 21..=108
    /// have no tuning entry and compose to the global bend alone.
    fn composed_pitch_offset(&self, key: u8) -> f64 {
        let tuning = key
            .checked_sub(21)
            .map(usize::from)
            .filter(|&i| i < 88)
            .and_then(|i| self.note_tuning_cents[i])
            .unwrap_or(0.0);
        tuning + self.pitch_bend_cents
    }

    pub fn set_expression_gain(&mut self, gain: f64) {
        self.expression_gain = gain.clamp(0.0, 2.0);
    }

    pub fn set_velocity_curve_points(&mut self, points: [f64; 5]) {
        for (i, p) in points.iter().enumerate() {
            self.velocity_curve_points[i] = p.clamp(0.0, 1.0);
        }
    }

    /// Maps an input velocity through the piecewise-linear curve points.
    pub fn map_velocity(&self, velocity: f64) -> f64 {
        map_velocity_through_points(&self.velocity_curve_points, velocity)
    }

    pub fn set_inharmonicity_scale(&mut self, scale: f64) {
        if (self.inharmonicity_scale - scale).abs() > 1e-4 {
            self.inharmonicity_scale = scale;
            for &key in &self.active_keys {
                if let Some(voice) = self.voices.get_mut(&key) {
                    voice.set_inharmonicity_scale(scale);
                }
            }
        }
    }

    pub fn set_hammer_hardness(&mut self, hardness: f64) {
        if (self.hammer_hardness - hardness).abs() > 1e-4 {
            self.hammer_hardness = hardness;
            for &key in &self.active_keys {
                if let Some(voice) = self.voices.get_mut(&key) {
                    voice.set_hammer_hardness(hardness);
                }
            }
        }
    }

    pub fn set_unison_detuning(&mut self, detune: f64) {
        if (self.unison_detuning - detune).abs() > 1e-4 {
            self.unison_detuning = detune;
            for &key in &self.active_keys {
                if let Some(voice) = self.voices.get_mut(&key) {
                    voice.set_unison_detuning(detune);
                }
            }
        }
    }

    pub fn set_phantom_gain(&mut self, gain: f64) {
        self.phantom_gain = gain;
    }

    pub fn set_radiation_mode(&mut self, mode: &str) {
        self.radiation_mode = mode.to_string();
        if mode == "multi_upols" && self.multi_upols.is_none() {
            let (close, player, ambient) =
                generate_multi_perspective_soundboard_irs(self.sample_rate, 0.6, 100);
            self.multi_upols = Some(MultiPerspectiveUPOLS::new(&close, &player, &ambient, 128));
        } else if mode == "upols" && self.upols.is_none() {
            let (ir_l, ir_r) =
                crate::dsp::upols::generate_orthotropic_soundboard_ir(self.sample_rate, 0.6, 100);
            self.upols = Some(UPOLSConvolver::new(&ir_l, &ir_r, 128));
        }
    }

    pub fn set_key_noise_gain(&mut self, gain: f64) {
        self.action_noise.gain = gain;
    }

    pub fn set_damper_noise_gain(&mut self, gain: f64) {
        self.damper_whoosh.gain = gain;
        self.restrike_buzz.gain = gain;
    }

    pub fn set_pedal_noise_gain(&mut self, gain: f64) {
        self.plate_shock.gain = gain;
    }

    pub fn set_mic_gains(&mut self, close: f64, player: f64, ambient: f64) {
        self.bridge.set_mic_gains(close, player, ambient);
        if let Some(upols) = self.multi_upols.as_mut() {
            upols.close_gain = close;
            upols.player_gain = player;
            upols.ambient_gain = ambient;
        }
    }

    pub fn set_lid_angle(&mut self, angle_deg: f64) {
        self.lid_baffle.set_angle_deg(angle_deg);
    }

    /// Preconstructs the complete keyboard outside the audio callback.
    /// Standalone/plugin hosts should call this during initialization so the first NoteOn only
    /// activates an already allocated voice.
    pub fn prepare_voices(&mut self) {
        for key in 21..=108u8 {
            self.get_or_create_voice(key);
        }
    }

    /// Callers must pass a key in 21..=108: `key_params` only covers the
    /// 88-key range. `note_on` guards out-of-range host notes before
    /// calling here; `prepare_voices` only iterates the valid range.
    pub fn get_or_create_voice(&mut self, key: u8) -> &mut PianoVoice {
        let sr = self.sample_rate;
        let una_corda = self.una_corda;
        let kp = self
            .key_params
            .get(&key)
            .expect("Key out of 88-key piano range");
        let hardness = self.hammer_hardness;
        let detune = self.unison_detuning;
        let inharm = self.inharmonicity_scale;
        let pitch_bend = self.composed_pitch_offset(key);

        self.voices.entry(key).or_insert_with(|| {
            let mut v = PianoVoice::new(kp.clone(), sr);
            if una_corda {
                v.set_una_corda(true);
            }
            v.set_hammer_hardness(hardness);
            v.update_string_tunings(detune, pitch_bend);
            v.set_inharmonicity_scale(inharm);
            v
        })
    }

    pub fn get_voice(&self, key: u8) -> Option<&PianoVoice> {
        self.voices.get(&key)
    }

    pub fn get_voice_mut(&mut self, key: u8) -> Option<&mut PianoVoice> {
        self.voices.get_mut(&key)
    }

    #[inline]
    pub fn sync_active_keys_vec(&mut self) {
        self.active_keys_vec.clear();
        self.active_keys_vec
            .extend(self.active_keys.iter().copied());
    }

    pub fn steal_voice(&mut self) -> Option<u8> {
        if self.active_keys.is_empty() {
            return None;
        }

        // Candidate 1: Key is not physically held down, lowest vibrational energy
        let released_candidate = self
            .active_keys
            .iter()
            .filter(|k| !self.depressed_keys.contains(k))
            .min_by(|&&a, &&b| {
                let ea = self.voices.get(&a).map(|v| v.get_energy()).unwrap_or(0.0);
                let eb = self.voices.get(&b).map(|v| v.get_energy()).unwrap_or(0.0);
                ea.partial_cmp(&eb).unwrap_or(std::cmp::Ordering::Equal)
            })
            .copied();

        let candidate = released_candidate.or_else(|| {
            // Candidate 2: All active keys are physically held down, steal the lowest energy one
            self.active_keys
                .iter()
                .min_by(|&&a, &&b| {
                    let ea = self.voices.get(&a).map(|v| v.get_energy()).unwrap_or(0.0);
                    let eb = self.voices.get(&b).map(|v| v.get_energy()).unwrap_or(0.0);
                    ea.partial_cmp(&eb).unwrap_or(std::cmp::Ordering::Equal)
                })
                .copied()
        });

        if let Some(stolen_key) = candidate {
            self.active_keys.remove(&stolen_key);
            self.sostenuto_notes.remove(&stolen_key);
            if let Some(v) = self.voices.get_mut(&stolen_key) {
                // Release ramp, not a hard reset: seat the damper so the
                // string decays fast, and keep stepping the voice in
                // `process_block` with its bridge contribution scaled
                // down over ~30 ms. Resetting here would zero the modal
                // state mid-oscillation and drop the bridge force in a
                // single sample (an audible click).
                v.sostenuto_held = false;
                v.note_off(false, 0.0);
                self.releasing_voices.retain(|(k, _)| *k != stolen_key);
                self.releasing_voices.push((stolen_key, 1.0));
            }
            self.sync_active_keys_vec();
            Some(stolen_key)
        } else {
            None
        }
    }

    pub fn note_on(&mut self, key: u8, velocity: f64) {
        // Hosts deliver the full MIDI range, but only A0..C8 (21..=108)
        // have parameters and voices. Ignore anything outside the range
        // instead of indexing `note_tuning_cents` / `key_params` out of
        // bounds, which would panic on the audio thread.
        if !(21..=108).contains(&key) {
            return;
        }
        if self.active_keys.len() >= self.max_active_voices && !self.active_keys.contains(&key) {
            self.steal_voice();
        }
        // A fresh strike supersedes any sostenuto capture: notes started
        // after the pedal went down are not held by it.
        self.sostenuto_notes.remove(&key);
        // A re-strike of a voice on the release ramp takes it back into
        // the active set; it must stop being stepped as a releasing voice
        // or it would be advanced twice per sample.
        self.releasing_voices.retain(|(k, _)| *k != key);
        let effective_vel = self.map_velocity(velocity);

        self.action_noise.trigger_note_on(key, effective_vel);
        let unison_detuning = self.unison_detuning;
        let pitch_bend_cents = self.composed_pitch_offset(key);
        let inharmonicity_scale = self.inharmonicity_scale;
        let hammer_hardness = self.hammer_hardness;
        let una_corda = self.una_corda;
        let cur_energy = {
            let voice = self.get_or_create_voice(key);
            if (voice.unison_scale - unison_detuning).abs() > 1e-4
                || (voice.pitch_bend_cents - pitch_bend_cents).abs() > 1e-4
            {
                voice.update_string_tunings(unison_detuning, pitch_bend_cents);
            }
            if (voice.inharmonicity_scale - inharmonicity_scale).abs() > 1e-4 {
                voice.set_inharmonicity_scale(inharmonicity_scale);
            }
            if (voice.hammer_hardness - hammer_hardness).abs() > 1e-4 {
                voice.set_hammer_hardness(hammer_hardness);
            }
            voice.set_una_corda(una_corda);
            voice.sostenuto_held = false;
            voice.note_on(effective_vel);
            voice.get_energy()
        };
        self.active_keys.insert(key);
        self.depressed_keys.insert(key);
        self.note_energy_peak.insert(key, cur_energy.max(1e-6));
        self.sync_active_keys_vec();
    }

    pub fn note_off(&mut self, key: u8) {
        self.action_noise.trigger_note_off(key, 0.5);
        if let Some(v) = self.voices.get(&key) {
            let cur_energy = v.get_energy();
            self.restrike_buzz.trigger(key, cur_energy);
        }
        self.depressed_keys.remove(&key);
        if let Some(v) = self.voices.get_mut(&key) {
            v.note_off(self.sustain_pedal, self.pedal_depth);
        }
    }

    pub fn set_note_tuning(&mut self, key: u8, cents: f64) {
        if let Some(index) = key.checked_sub(21).map(usize::from).filter(|&i| i < 88) {
            self.note_tuning_cents[index] = Some(cents);
        }
        // The voice stores a single composed offset, so hand it the
        // tuning plus the current global bend rather than the tuning
        // alone, which would erase the bend for a sounding voice.
        let composed = self.composed_pitch_offset(key);
        if let Some(v) = self.voices.get_mut(&key) {
            v.set_pitch_bend(composed);
        }
    }

    pub fn set_sustain_pedal(&mut self, pedal_down: bool, depth: f64) {
        let diff = depth - self.prev_pedal_depth;
        if diff > 0.05 {
            self.damper_whoosh.trigger(diff * 6.0);
        }
        if diff.abs() > 0.25 {
            self.plate_shock.trigger(diff * 4.0);
        }
        self.prev_pedal_depth = depth;

        self.sustain_pedal = pedal_down;
        self.pedal_depth = depth.clamp(0.0, 1.0);
        for &key in &self.active_keys {
            if let Some(voice) = self.voices.get_mut(&key) {
                voice.set_sustain_pedal(self.sustain_pedal, self.pedal_depth);
            }
        }
    }

    /// Sostenuto (middle pedal, CC66): pressing captures the keys held
    /// at that moment; releasing re-applies their pending note-offs
    /// (honoring the sustain pedal) for keys no longer held down.
    pub fn set_sostenuto_pedal(&mut self, down: bool) {
        if down == self.sostenuto_pedal {
            return;
        }
        self.sostenuto_pedal = down;
        if down {
            self.sostenuto_notes = self.depressed_keys.clone();
            for &key in &self.sostenuto_notes {
                if let Some(voice) = self.voices.get_mut(&key) {
                    voice.sostenuto_held = true;
                }
            }
        } else {
            let captured = std::mem::take(&mut self.sostenuto_notes);
            for key in captured {
                if let Some(voice) = self.voices.get_mut(&key) {
                    voice.sostenuto_held = false;
                }
                if !self.depressed_keys.contains(&key) {
                    if let Some(voice) = self.voices.get_mut(&key) {
                        voice.note_off(self.sustain_pedal, self.pedal_depth);
                    }
                }
            }
        }
    }

    pub fn set_una_corda(&mut self, enabled: bool) {
        self.una_corda = enabled;
        for &key in &self.active_keys {
            if let Some(voice) = self.voices.get_mut(&key) {
                voice.set_una_corda(enabled);
            }
        }
    }

    /// Process a block of samples through the 4-stage pipeline with sample-accurate events.
    pub fn process_block(
        &mut self,
        num_samples: usize,
        events: &[EngineEvent],
        out_events: &mut Vec<EngineOutEvent>,
        out_left: &mut [f64],
        out_right: &mut [f64],
    ) {
        assert_eq!(out_left.len(), num_samples);
        assert_eq!(out_right.len(), num_samples);

        if self.active_keys_vec.len() != self.active_keys.len() {
            self.sync_active_keys_vec();
        }

        let mut event_idx = 0;
        let use_multi_upols = self.radiation_mode == "multi_upols" && self.multi_upols.is_some();
        let use_upols = self.radiation_mode == "upols" && self.upols.is_some();

        for s in 0..num_samples {
            // Stage 1: Sample-accurate event dispatch
            while event_idx < events.len() {
                let ev = &events[event_idx];
                let ev_time = match ev {
                    EngineEvent::NoteOn { time, .. } => *time,
                    EngineEvent::NoteOff { time, .. } => *time,
                    EngineEvent::NoteTuning { time, .. } => *time,
                    EngineEvent::SustainPedal { time, .. } => *time,
                    EngineEvent::SostenutoPedal { time, .. } => *time,
                    EngineEvent::UnaCorda { time, .. } => *time,
                    EngineEvent::PitchBend { time, .. } => *time,
                    EngineEvent::Expression { time, .. } => *time,
                };
                if ev_time <= s {
                    match ev {
                        EngineEvent::NoteOn { key, velocity, .. } => self.note_on(*key, *velocity),
                        EngineEvent::NoteOff { key, .. } => self.note_off(*key),
                        EngineEvent::NoteTuning { key, cents, .. } => {
                            self.set_note_tuning(*key, *cents)
                        }
                        EngineEvent::SustainPedal { depth, .. } => {
                            let down = *depth > 0.05;
                            self.set_sustain_pedal(down, *depth);
                        }
                        EngineEvent::SostenutoPedal { down, .. } => {
                            self.set_sostenuto_pedal(*down)
                        }
                        EngineEvent::UnaCorda { enabled, .. } => self.set_una_corda(*enabled),
                        EngineEvent::PitchBend { cents, .. } => self.set_pitch_bend(*cents),
                        EngineEvent::Expression { gain, .. } => self.set_expression_gain(*gain),
                    }
                    event_idx += 1;
                } else {
                    break;
                }
            }

            // Stage 2: Voice & modal oscillator bank stepping
            let mut total_bridge_t = 0.0;
            let mut total_bridge_p = 0.0;
            let mut total_bridge_l = 0.0;

            let mut f_sb_l = 0.0;
            let mut f_sb_r = 0.0;

            let num_active = self.active_keys_vec.len();
            let coupling_t = self.f_react_t / num_active.max(1) as f64;
            let coupling_p = self.f_react_p / num_active.max(1) as f64;
            let phantom_scale = 0.28 * self.phantom_gain;

            for &key in &self.active_keys_vec {
                if let Some(v) = self.voices.get_mut(&key) {
                    let (fb_t, fb_p, fb_l) = v.step(coupling_t, coupling_p);
                    total_bridge_t += fb_t;
                    total_bridge_p += fb_p;
                    total_bridge_l += fb_l;

                    let f_k = 0.82 * fb_t + 0.15 * fb_p + phantom_scale * fb_l;
                    f_sb_l += f_k * v.pan_l;
                    f_sb_r += f_k * v.pan_r;
                }
            }

            // Releasing (stolen) voices: step with the same coupling,
            // scaling the bridge contribution by the remaining release
            // gain, which ramps 1.0 -> 0.0 over ~30 ms. The gain is
            // applied per sample, so there is no single-sample drop.
            let release_step = 1.0 / (RELEASE_RAMP_SECONDS * self.sample_rate).max(1.0);
            for entry in self.releasing_voices.iter_mut() {
                let gain = entry.1;
                if gain <= 0.0 {
                    continue;
                }
                let key = entry.0;
                if let Some(v) = self.voices.get_mut(&key) {
                    let (fb_t, fb_p, fb_l) = v.step(coupling_t, coupling_p);
                    total_bridge_t += fb_t * gain;
                    total_bridge_p += fb_p * gain;
                    total_bridge_l += fb_l * gain;

                    let f_k = 0.82 * fb_t + 0.15 * fb_p + phantom_scale * fb_l;
                    f_sb_l += f_k * gain * v.pan_l;
                    f_sb_r += f_k * gain * v.pan_r;
                }
                entry.1 = (gain - release_step).max(0.0);
            }

            // Stage 3: Bridge reaction force reduction
            let (react_t, react_p, f_sb) = self.bridge.calculate_coupling_forces(
                total_bridge_t,
                total_bridge_p,
                total_bridge_l,
            );
            self.f_react_t = react_t;
            self.f_react_p = react_p;

            // Stage 4: Soundboard radiation, lid acoustic baffle & mechanical action noise
            let (sb_l, sb_r) = if use_multi_upols {
                self.multi_upols.as_mut().unwrap().process_sample(f_sb)
            } else if use_upols {
                self.upols.as_mut().unwrap().process_sample(f_sb)
            } else {
                self.bridge.step_soundboard_stereo(f_sb_l, f_sb_r)
            };

            // Apply continuous lid acoustic baffle
            let (rad_l, rad_r) = self.lid_baffle.process(sb_l, sb_r);

            // Tier 6: Micro-mechanical action noise
            let (act_l, act_r) = self.action_noise.step();
            let (whoosh_l, whoosh_r) = self.damper_whoosh.step();
            let (shock_l, shock_r) = self.plate_shock.step();
            let (buzz_l, buzz_r) = self.restrike_buzz.step();

            let expr = self.expression_gain;
            out_left[s] = (rad_l + act_l + whoosh_l + shock_l + buzz_l) * expr;
            out_right[s] = (rad_r + act_r + whoosh_r + shock_r + buzz_r) * expr;
        }

        // Stage 5: Voice lifecycle & polyphony management at block boundary
        self.keys_to_remove_scratch.clear();
        for &key in &self.active_keys_vec {
            if let Some(v) = self.voices.get(&key) {
                let cur_energy = v.get_energy();
                let peak_e = self
                    .note_energy_peak
                    .get(&key)
                    .copied()
                    .unwrap_or(1e-6)
                    .max(cur_energy);
                self.note_energy_peak.insert(key, peak_e);

                if !self.depressed_keys.contains(&key) {
                    let ratio = cur_energy / peak_e.max(1e-12);
                    // Retire at -70 dB of peak energy whether damped or
                    // sustained: a longer sustain tail was considered
                    // here, but both branches of the old conditional were
                    // the same 1e-7, so there is a single threshold.
                    let thresh = 1e-7;
                    if ratio < thresh || cur_energy < 1e-10 {
                        self.keys_to_remove_scratch.push(key);
                    }
                }
            }
        }

        for &key in &self.keys_to_remove_scratch {
            self.active_keys.remove(&key);
            if let Some(v) = self.voices.get_mut(&key) {
                v.reset();
            }
            out_events.push(EngineOutEvent::NoteEnd {
                time: num_samples,
                key,
            });
        }

        // Releasing voices whose ramp has run out are inaudible now;
        // reset them so the next strike starts from silence.
        // Stack scratch, not a Vec: collecting into a Vec here would
        // heap-allocate inside process_block, tripping the
        // no-allocation guard. At most one entry per piano key exists
        // (entries are deduplicated by key on push), so 88 slots —
        // keys 21..=108 — always suffice.
        let mut finished_releasing = [0u8; 88];
        let mut finished_count = 0usize;
        self.releasing_voices.retain(|(key, gain)| {
            if *gain <= 0.0 {
                if finished_count < finished_releasing.len() {
                    finished_releasing[finished_count] = *key;
                    finished_count += 1;
                }
                false
            } else {
                true
            }
        });
        for key in &finished_releasing[..finished_count] {
            if let Some(v) = self.voices.get_mut(&key) {
                v.reset();
            }
        }

        // Voice Stealing: enforce max polyphony
        while self.active_keys.len() > self.max_active_voices {
            if let Some(stolen_key) = self.steal_voice() {
                out_events.push(EngineOutEvent::NoteEnd {
                    time: num_samples,
                    key: stolen_key,
                });
            } else {
                break;
            }
        }

        self.sync_active_keys_vec();
    }

    /// Render audio for a given duration in seconds.
    pub fn render(&mut self, duration: f64) -> (Vec<f64>, Vec<f64>) {
        let num_samples = (duration * self.sample_rate) as usize;
        let mut out_left = vec![0.0; num_samples];
        let mut out_right = vec![0.0; num_samples];
        let mut out_events = Vec::new();

        self.process_block(
            num_samples,
            &[],
            &mut out_events,
            &mut out_left,
            &mut out_right,
        );

        (out_left, out_right)
    }

    /// Full DSP and voice state reset on transport stop / seek / restart.
    pub fn reset(&mut self) {
        for voice in self.voices.values_mut() {
            voice.reset();
        }
        self.active_keys.clear();
        self.depressed_keys.clear();
        self.releasing_voices.clear();
        self.sostenuto_pedal = false;
        self.sostenuto_notes.clear();
        self.active_keys_vec.clear();
        self.note_energy_peak.clear();
        self.pitch_bend_cents = 0.0;
        self.note_tuning_cents.fill(None);
        self.expression_gain = 1.0;
        self.f_react_t = 0.0;
        self.f_react_p = 0.0;
        self.bridge = BridgeSoundboard::new(self.sample_rate);
        // Reset the lid baffle in place: rebuilding it heap-allocates
        // its sample-rate-sized delay Vecs, and reset() runs under
        // nice-plug's no-allocation guard. reset() keeps the current
        // lid angle (re-applied per block by process()); only the
        // filter state and delay lines are cleared, as a rebuild would.
        self.lid_baffle.reset();
        self.action_noise = KeyActionNoise::new(self.sample_rate);
        self.damper_whoosh = DamperWhoosh::new(self.sample_rate);
        self.plate_shock = PlateShock::new(self.sample_rate);
        self.restrike_buzz = RestrikeBuzz::new(self.sample_rate);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_curve_is_identity() {
        let points = [0.0, 0.25, 0.5, 0.75, 1.0];
        for v in [0.1, 0.25, 0.4, 0.5, 0.625, 0.9] {
            assert!(
                (map_velocity_through_points(&points, v) - v).abs() < 1e-9,
                "v={v}"
            );
        }
    }

    #[test]
    fn piecewise_segments_interpolate() {
        let points = [0.0, 0.5, 0.5, 0.5, 1.0];
        assert!((map_velocity_through_points(&points, 0.125) - 0.25).abs() < 1e-9);
        assert!((map_velocity_through_points(&points, 0.375) - 0.5).abs() < 1e-9);
        assert!((map_velocity_through_points(&points, 0.875) - 0.75).abs() < 1e-9);
    }

    #[test]
    fn velocity_mapping_clamps() {
        let points = [0.0, 0.25, 0.5, 0.75, 1.0];
        assert!((map_velocity_through_points(&points, 0.0) - 0.001).abs() < 1e-9);
        assert!((map_velocity_through_points(&points, 2.0) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn exponent_conversion_preserves_old_formula() {
        // Legacy exponent 0 is linear.
        assert_eq!(
            velocity_points_from_exponent(0.0),
            [0.0, 0.25, 0.5, 0.75, 1.0]
        );
        // A soft (negative) curve lifts low velocities; endpoints stay 0/1.
        let soft = velocity_points_from_exponent(-0.3);
        assert_eq!(soft[0], 0.0);
        assert_eq!(soft[4], 1.0);
        assert!(soft[1] > 0.25 && soft[1] < 0.5);
        assert!((soft[1] - 0.3097).abs() < 0.005);
        // A hard (positive) curve suppresses low velocities.
        let hard = velocity_points_from_exponent(0.3);
        assert!(hard[1] < 0.25 && hard[1] > 0.0);
        // Points stay strictly increasing, so the curve is a valid function.
        for pts in [soft, hard] {
            for w in pts.windows(2) {
                assert!(w[0] < w[1]);
            }
        }
    }

    #[test]
    fn out_of_range_note_on_through_process_block_is_ignored() {
        let mut engine = PianoEngine::new(48_000.0, 15, false);
        let block_size = 64;
        let mut out_l = vec![0.0; block_size];
        let mut out_r = vec![0.0; block_size];
        let mut out_events = Vec::new();

        // Hosts deliver the full MIDI range; keys 0 and 127 used to
        // panic on the audio thread via out-of-bounds indexing.
        let events = vec![
            EngineEvent::NoteOn {
                time: 0,
                key: 0,
                velocity: 0.8,
            },
            EngineEvent::NoteOn {
                time: 0,
                key: 127,
                velocity: 0.8,
            },
            EngineEvent::NoteOff { time: 0, key: 0 },
            EngineEvent::NoteOff {
                time: 0,
                key: 127,
            },
            EngineEvent::NoteOn {
                time: 0,
                key: 60,
                velocity: 0.8,
            },
        ];
        engine.process_block(
            block_size,
            &events,
            &mut out_events,
            &mut out_l,
            &mut out_r,
        );

        assert!(engine.get_voice(0).is_none());
        assert!(engine.get_voice(127).is_none());
        assert!(!engine.active_keys.contains(&0));
        assert!(!engine.active_keys.contains(&127));
        assert!(!engine.note_energy_peak.contains_key(&0));
        assert!(!engine.note_energy_peak.contains_key(&127));
        assert!(engine.note_tuning_cents.iter().all(|t| t.is_none()));
        assert!(engine.get_voice(60).is_some());
        assert!(engine.active_keys.contains(&60));

        // Direct calls, including the keys just outside the range, are
        // ignored too and leave no state behind.
        engine.note_on(0, 0.8);
        engine.note_on(20, 0.8);
        engine.note_on(109, 0.8);
        engine.note_on(127, 0.8);
        assert!(engine.get_voice(0).is_none());
        assert!(engine.get_voice(20).is_none());
        assert!(engine.get_voice(109).is_none());
        assert!(engine.get_voice(127).is_none());

        // The range edges themselves still sound.
        engine.note_on(21, 0.8);
        engine.note_on(108, 0.8);
        assert!(engine.get_voice(21).is_some());
        assert!(engine.get_voice(108).is_some());
    }

    #[test]
    fn pitch_bend_preserves_per_note_tuning_and_composes() {
        let mut engine = PianoEngine::new(44_100.0, 15, false);
        engine.note_on(60, 0.8);
        engine.note_on(62, 0.8);
        let baseline = engine.get_voice(60).unwrap().strings[0].tuning_offset_cents;

        // Per-note tuning applies to the sounding voice.
        engine.set_note_tuning(60, 50.0);
        assert_eq!(engine.note_tuning_cents[60 - 21], Some(50.0));
        assert!((engine.get_voice(60).unwrap().pitch_bend_cents - 50.0).abs() < 1e-6);
        let tuned = engine.get_voice(60).unwrap().strings[0].tuning_offset_cents;
        assert!((tuned - baseline - 50.0).abs() < 1e-6);

        // Pitch bend must not erase the tuning: the sounding voice and
        // the stored tuning both keep it, composed with the bend.
        engine.set_pitch_bend(100.0);
        assert_eq!(engine.note_tuning_cents[60 - 21], Some(50.0));
        assert!((engine.get_voice(60).unwrap().pitch_bend_cents - 150.0).abs() < 1e-6);
        let bent = engine.get_voice(60).unwrap().strings[0].tuning_offset_cents;
        assert!((bent - baseline - 150.0).abs() < 1e-6);
        // No-tuning case is unchanged: the untuned voice hears bend only.
        assert!((engine.get_voice(62).unwrap().pitch_bend_cents - 100.0).abs() < 1e-6);

        // Bend back to zero restores the tuned pitch.
        engine.set_pitch_bend(0.0);
        assert_eq!(engine.note_tuning_cents[60 - 21], Some(50.0));
        assert!((engine.get_voice(60).unwrap().pitch_bend_cents - 50.0).abs() < 1e-6);
        let restored = engine.get_voice(60).unwrap().strings[0].tuning_offset_cents;
        assert!((restored - baseline - 50.0).abs() < 1e-6);

        // A retriggered tuned note keeps its tuning under bend.
        engine.set_pitch_bend(100.0);
        engine.note_on(60, 0.8);
        assert!((engine.get_voice(60).unwrap().pitch_bend_cents - 150.0).abs() < 1e-6);

        // Tuning set before the first strike composes with the bend too.
        engine.set_note_tuning(64, -25.0);
        engine.note_on(64, 0.8);
        assert!((engine.get_voice(64).unwrap().pitch_bend_cents - 75.0).abs() < 1e-6);
    }

    #[test]
    fn stolen_voice_releases_over_a_ramp_instead_of_resetting() {
        let mut engine = PianoEngine::new(48_000.0, 15, false);
        engine.max_active_voices = 2;
        let block = 64usize;
        let mut out_l = vec![0.0; block];
        let mut out_r = vec![0.0; block];
        let mut out_events = Vec::new();
        let run_block = |engine: &mut PianoEngine,
                         out_l: &mut [f64],
                         out_r: &mut [f64],
                         out_events: &mut Vec<EngineOutEvent>| {
            engine.process_block(block, &[], out_events, out_l, out_r);
        };

        engine.note_on(60, 0.9);
        engine.note_on(64, 0.9);
        run_block(&mut engine, &mut out_l, &mut out_r, &mut out_events);
        engine.note_off(60);
        engine.note_off(64);
        // A third strike steals the lowest-energy released voice. Which
        // of the two is stolen depends on their energies, so identify
        // the victim from the resulting state instead of assuming it.
        engine.note_on(67, 0.9);
        let victim = [60u8, 64]
            .into_iter()
            .find(|k| !engine.active_keys.contains(k));
        let victim = victim.expect("one released voice must have been stolen");
        let survivor = if victim == 60 { 64 } else { 60 };
        assert!(engine.active_keys.contains(&67));
        assert!(engine.active_keys.contains(&survivor));
        assert!(engine.note_energy_peak.contains_key(&victim));

        // The stolen voice was NOT reset: it is on the release ramp at
        // full gain, still holding energy, with its damper seated.
        let (key, gain) = engine
            .releasing_voices
            .iter()
            .find(|(k, _)| *k == victim)
            .copied()
            .expect("stolen voice must be releasing");
        assert_eq!(key, victim);
        assert_eq!(gain, 1.0);
        let energy_at_steal = engine.get_voice(victim).unwrap().get_energy();
        assert!(
            energy_at_steal > 0.0,
            "stolen voice must keep its energy, got {energy_at_steal}"
        );
        assert!(engine.get_voice(victim).unwrap().is_sounding);

        // One block in, the gain has ramped down by exactly one block's
        // worth of the 30 ms ramp — a gradual fade, not a one-sample
        // drop — and the voice still holds energy.
        run_block(&mut engine, &mut out_l, &mut out_r, &mut out_events);
        let ramp_samples = RELEASE_RAMP_SECONDS * 48_000.0;
        let gain_after_block = engine
            .releasing_voices
            .iter()
            .find(|(k, _)| *k == victim)
            .map(|(_, g)| *g)
            .expect("stolen voice still releasing after one block");
        assert!(
            (gain_after_block - (1.0 - block as f64 / ramp_samples)).abs() < 1e-9,
            "gain must ramp linearly, got {gain_after_block}"
        );
        assert!(engine.get_voice(victim).unwrap().get_energy() > 0.0);

        // Past the ramp (24 blocks > 1440 samples), the voice is reset
        // and gone from the releasing list.
        for _ in 0..24 {
            run_block(&mut engine, &mut out_l, &mut out_r, &mut out_events);
        }
        assert!(
            !engine.releasing_voices.iter().any(|(k, _)| *k == victim),
            "release ramp must finish"
        );
        assert_eq!(engine.get_voice(victim).unwrap().get_energy(), 0.0);
        assert!(!engine.get_voice(victim).unwrap().is_sounding);

        // A re-strike during the ramp returns the voice to the active
        // set instead of double-stepping it (fresh engine, clean state;
        // the limit is raised after the steal so the re-strike itself
        // steals nothing).
        let mut engine = PianoEngine::new(48_000.0, 15, false);
        engine.max_active_voices = 2;
        engine.note_on(60, 0.9);
        engine.note_on(64, 0.9);
        engine.note_off(60);
        engine.note_off(64);
        engine.note_on(67, 0.9);
        assert_eq!(engine.releasing_voices.len(), 1);
        let victim2 = engine.releasing_voices[0].0;
        engine.max_active_voices = 3;
        engine.note_on(victim2, 0.9);
        assert!(engine.releasing_voices.is_empty());
        assert!(engine.active_keys.contains(&victim2));
        engine.process_block(block, &[], &mut out_events, &mut out_l, &mut out_r);
        assert!(engine.get_voice(victim2).unwrap().get_energy() > 0.0);
    }
}

#[cfg(all(test, debug_assertions))]
mod no_alloc_regression_tests {
    // Regression tests for the debug-standalone startup abort: reset()
    // runs under nice-plug's no-allocation guard, so any heap
    // (de)allocation there aborts the process (the LidBaffle delay
    // Vecs rebuilt by reset() did exactly that). In test builds the
    // guard only counts violations (warn_debug), so these tests
    // assert the count stays flat across the guarded section.
    use super::*;
    use nice_assert_no_alloc::{assert_no_alloc, violation_count};

    #[test]
    fn reset_does_not_allocate() {
        let mut engine = PianoEngine::new(48_000.0, 15, false);
        let before = violation_count();
        assert_no_alloc(|| engine.reset());
        assert_eq!(violation_count(), before);
    }

    #[test]
    fn voice_steal_release_ramp_and_prune_do_not_allocate() {
        let mut engine = PianoEngine::new(48_000.0, 15, false);
        engine.max_active_voices = 2;
        // Mirror activate(): voices are pre-created outside the guard
        // (prepare_voices), so striking a key must not allocate.
        engine.prepare_voices();
        let mut out_events = Vec::with_capacity(128);
        let mut out_l = vec![0.0; 64];
        let mut out_r = vec![0.0; 64];
        let before = violation_count();
        assert_no_alloc(|| {
            // Stealing pushes to releasing_voices, and the release-ramp
            // prune collects finished voices — both run here, inside
            // the guard, as they do on the audio thread.
            engine.note_on(60, 0.9);
            engine.note_on(64, 0.9);
            engine.note_off(60);
            engine.note_off(64);
            engine.note_on(67, 0.9);
            assert_eq!(engine.releasing_voices.len(), 1);
            // 25 blocks of 64 samples > 1440-sample (30 ms) ramp.
            for _ in 0..25 {
                engine.process_block(64, &[], &mut out_events, &mut out_l, &mut out_r);
            }
        });
        assert_eq!(violation_count(), before);
        assert!(engine.releasing_voices.is_empty());
    }
}
