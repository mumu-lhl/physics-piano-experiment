//! High-Performance Physical Piano Simulation Coordinator and 4-Stage Audio Pipeline in Rust.

use std::collections::{HashMap, HashSet};
use crate::params::{generate_grand_piano_parameters, KeyParams};
use crate::core::voice::PianoVoice;
use crate::core::bridge::BridgeSoundboard;
use crate::dsp::upols::UPOLSConvolver;

#[derive(Debug, Clone)]
pub enum EngineEvent {
    NoteOn { time: usize, key: u8, velocity: f64 },
    NoteOff { time: usize, key: u8 },
    NoteTuning { time: usize, key: u8, cents: f64 },
    SustainPedal { time: usize, depth: f64 },
    UnaCorda { time: usize, enabled: bool },
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
    pub radiation_mode: String,

    pub sustain_pedal: bool,
    pub pedal_depth: f64,
    pub una_corda: bool,

    // Peak energy monitor for -96dB note lifecycle garbage collection
    pub note_energy_peak: HashMap<u8, f64>,

    // Reaction coupling forces from previous step
    pub f_react_t: f64,
    pub f_react_p: f64,

    pub max_active_voices: usize,
    pub active_keys_vec: Vec<u8>,
    pub keys_to_remove_scratch: Vec<u8>,
}

pub const MAX_ACTIVE_VOICES: usize = 16;

impl PianoEngine {
    pub fn new(sample_rate: f64, num_modes: usize, stretch_tuning: bool) -> Self {
        let dt = 1.0 / sample_rate;
        let all_params = generate_grand_piano_parameters(num_modes, stretch_tuning);
        let mut key_params = HashMap::with_capacity(88);
        for kp in all_params {
            key_params.insert(kp.midi_note, kp);
        }

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
            radiation_mode: "modal".to_string(),
            sustain_pedal: false,
            pedal_depth: 1.0,
            una_corda: false,
            note_energy_peak: HashMap::with_capacity(88),
            f_react_t: 0.0,
            f_react_p: 0.0,
            max_active_voices: MAX_ACTIVE_VOICES,
            active_keys_vec: Vec::with_capacity(32),
            keys_to_remove_scratch: Vec::with_capacity(32),
        }
    }

    pub fn set_radiation_mode(&mut self, mode: &str) {
        self.radiation_mode = mode.to_string();
        if mode == "upols" && self.upols.is_none() {
            let (ir_l, ir_r) = crate::dsp::upols::generate_orthotropic_soundboard_ir(self.sample_rate, 0.6, 100);
            self.upols = Some(UPOLSConvolver::new(&ir_l, &ir_r, 128));
        }
    }

    pub fn get_or_create_voice(&mut self, key: u8) -> &mut PianoVoice {
        let sr = self.sample_rate;
        let una_corda = self.una_corda;
        let kp = self.key_params.get(&key).expect("Key out of 88-key piano range");
        self.voices.entry(key).or_insert_with(|| {
            let mut v = PianoVoice::new(kp.clone(), sr);
            if una_corda {
                v.set_una_corda(true);
            }
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
        self.active_keys_vec.extend(self.active_keys.iter().copied());
    }

    pub fn steal_voice(&mut self) -> Option<u8> {
        if self.active_keys.is_empty() {
            return None;
        }

        // Candidate 1: Key is not physically held down, lowest vibrational energy
        let released_candidate = self.active_keys.iter()
            .filter(|k| !self.depressed_keys.contains(k))
            .min_by(|&&a, &&b| {
                let ea = self.voices.get(&a).map(|v| v.get_energy()).unwrap_or(0.0);
                let eb = self.voices.get(&b).map(|v| v.get_energy()).unwrap_or(0.0);
                ea.partial_cmp(&eb).unwrap_or(std::cmp::Ordering::Equal)
            })
            .copied();

        let candidate = released_candidate.or_else(|| {
            // Candidate 2: All active keys are physically held down, steal the lowest energy one
            self.active_keys.iter()
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
        let cur_energy = {
            let v = self.get_or_create_voice(key);
            v.note_on(velocity);
            v.get_energy()
        };
        self.active_keys.insert(key);
        self.depressed_keys.insert(key);
        self.note_energy_peak.insert(key, cur_energy.max(1e-6));
        self.sync_active_keys_vec();
    }

    pub fn note_off(&mut self, key: u8) {
        self.depressed_keys.remove(&key);
        if let Some(v) = self.voices.get_mut(&key) {
            v.note_off(self.sustain_pedal);
        }
    }

    pub fn set_note_tuning(&mut self, key: u8, cents: f64) {
        if let Some(v) = self.voices.get_mut(&key) {
            v.set_tuning_offset(cents);
        }
    }

    pub fn set_sustain_pedal(&mut self, pedal_down: bool, depth: f64) {
        self.sustain_pedal = pedal_down;
        self.pedal_depth = depth.clamp(0.0, 1.0);
        for v in self.voices.values_mut() {
            v.set_sustain_pedal(self.sustain_pedal, self.pedal_depth);
        }
    }

    pub fn set_una_corda(&mut self, enabled: bool) {
        self.una_corda = enabled;
        for v in self.voices.values_mut() {
            v.set_una_corda(enabled);
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
                };
                if ev_time <= s {
                    match ev {
                        EngineEvent::NoteOn { key, velocity, .. } => self.note_on(*key, *velocity),
                        EngineEvent::NoteOff { key, .. } => self.note_off(*key),
                        EngineEvent::NoteTuning { key, cents, .. } => self.set_note_tuning(*key, *cents),
                        EngineEvent::SustainPedal { depth, .. } => {
                            let down = *depth > 0.05;
                            self.set_sustain_pedal(down, *depth);
                        }
                        EngineEvent::UnaCorda { enabled, .. } => self.set_una_corda(*enabled),
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

            let num_active = self.active_keys_vec.len();
            let coupling_t = self.f_react_t / num_active.max(1) as f64;
            let coupling_p = self.f_react_p / num_active.max(1) as f64;

            for &key in &self.active_keys_vec {
                if let Some(v) = self.voices.get_mut(&key) {
                    let (fb_t, fb_p, fb_l) = v.step(coupling_t, coupling_p);
                    total_bridge_t += fb_t;
                    total_bridge_p += fb_p;
                    total_bridge_l += fb_l;
                }
            }

            // Stage 3: Bridge reaction force reduction
            let (react_t, react_p, f_sb) = self.bridge.calculate_coupling_forces(
                total_bridge_t, total_bridge_p, total_bridge_l,
            );
            self.f_react_t = react_t;
            self.f_react_p = react_p;

            // Stage 4: Soundboard radiation & stereo output
            if self.radiation_mode == "upols" {
                if let Some(ref mut upols) = self.upols {
                    let (l, r) = upols.process_sample(f_sb);
                    out_left[s] = l;
                    out_right[s] = r;
                } else {
                    let (l, r) = self.bridge.step_soundboard(f_sb, 0.5);
                    out_left[s] = l;
                    out_right[s] = r;
                }
            } else {
                let (l, r) = self.bridge.step_soundboard(f_sb, 0.5);
                out_left[s] = l;
                out_right[s] = r;
            }
        }

        // Stage 5: Voice lifecycle & polyphony management at block boundary
        self.keys_to_remove_scratch.clear();
        for &key in &self.active_keys_vec {
            if let Some(v) = self.voices.get(&key) {
                let cur_energy = v.get_energy();
                let peak_e = self.note_energy_peak.get(&key).copied().unwrap_or(1e-6).max(cur_energy);
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
            out_events.push(EngineOutEvent::NoteEnd { time: num_samples, key });
        }

        // Voice Stealing: enforce max polyphony
        while self.active_keys.len() > self.max_active_voices {
            if let Some(stolen_key) = self.steal_voice() {
                out_events.push(EngineOutEvent::NoteEnd { time: num_samples, key: stolen_key });
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

        self.process_block(num_samples, &[], &mut out_events, &mut out_left, &mut out_right);

        (out_left, out_right)
    }
}
