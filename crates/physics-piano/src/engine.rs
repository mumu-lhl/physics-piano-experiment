use crate::core::action::KeyActionNoise;
use crate::core::bridge::BridgeSoundboard;
use crate::core::pedal::{DamperWhoosh, PlateShock, RestrikeBuzz};
use crate::core::voice::PianoVoice;
use crate::dsp::lid::LidBaffle;
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
    UnaCorda { time: usize, enabled: bool },
    PitchBend { time: usize, cents: f64 },
    Expression { time: usize, gain: f64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineOutEvent {
    NoteEnd { time: usize, key: u8 },
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

    // Core voicing parameters
    pub inharmonicity_scale: f64,
    pub hammer_hardness: f64,
    pub unison_detuning: f64,
    pub phantom_gain: f64,
    pub pitch_bend_cents: f64,
    pub note_tuning_cents: [Option<f64>; 88],
    pub expression_gain: f64,
    pub velocity_curve: f64,

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
}

pub const MAX_ACTIVE_VOICES: usize = 32;

impl PianoEngine {
    pub fn new(sample_rate: f64, num_modes: usize, stretch_tuning: bool) -> Self {
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
            inharmonicity_scale: 1.0,
            hammer_hardness: 1.0,
            unison_detuning: 1.0,
            phantom_gain: 1.0,
            pitch_bend_cents: 0.0,
            note_tuning_cents: [None; 88],
            expression_gain: 1.0,
            velocity_curve: 0.0,
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
        }
    }

    pub fn set_pitch_bend(&mut self, cents: f64) {
        self.pitch_bend_cents = cents;
        self.note_tuning_cents.fill(None);
        for &key in &self.active_keys {
            if let Some(voice) = self.voices.get_mut(&key) {
                voice.set_pitch_bend(cents);
            }
        }
    }

    pub fn set_expression_gain(&mut self, gain: f64) {
        self.expression_gain = gain.clamp(0.0, 2.0);
    }

    pub fn set_velocity_curve(&mut self, curve: f64) {
        self.velocity_curve = curve.clamp(-1.0, 1.0);
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
        let pitch_bend =
            self.note_tuning_cents[(key - 21) as usize].unwrap_or(self.pitch_bend_cents);

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
            if let Some(v) = self.voices.get_mut(&stolen_key) {
                v.reset();
            }
            self.sync_active_keys_vec();
            Some(stolen_key)
        } else {
            None
        }
    }

    pub fn note_on(&mut self, key: u8, velocity: f64) {
        if self.active_keys.len() >= self.max_active_voices && !self.active_keys.contains(&key) {
            self.steal_voice();
        }
        let effective_vel = if self.velocity_curve.abs() > 1e-4 {
            let exponent = 2.0_f64.powf(self.velocity_curve * 0.8);
            velocity.clamp(0.001, 1.0).powf(exponent).clamp(0.001, 1.0)
        } else {
            velocity.clamp(0.001, 1.0)
        };

        self.action_noise.trigger_note_on(key, effective_vel);
        let unison_detuning = self.unison_detuning;
        let pitch_bend_cents =
            self.note_tuning_cents[(key - 21) as usize].unwrap_or(self.pitch_bend_cents);
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
        if let Some(v) = self.voices.get_mut(&key) {
            v.set_tuning_offset(cents);
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
                    // Damped or sustained: if decayed to inaudibility (-70 dB ~ -80 dB)
                    let thresh = if !self.sustain_pedal { 1e-7 } else { 1e-7 };
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
        self.active_keys_vec.clear();
        self.note_energy_peak.clear();
        self.pitch_bend_cents = 0.0;
        self.note_tuning_cents.fill(None);
        self.expression_gain = 1.0;
        self.f_react_t = 0.0;
        self.f_react_p = 0.0;
        self.bridge = BridgeSoundboard::new(self.sample_rate);
        self.lid_baffle = LidBaffle::new(self.sample_rate);
        self.action_noise = KeyActionNoise::new(self.sample_rate);
        self.damper_whoosh = DamperWhoosh::new(self.sample_rate);
        self.plate_shock = PlateShock::new(self.sample_rate);
        self.restrike_buzz = RestrikeBuzz::new(self.sample_rate);
    }
}
