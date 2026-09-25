//! Piano Voice with Unison String Triplet, Micro-Detuning, and Damper Control in Rust.

use crate::params::KeyParams;
use crate::core::string::StiffStringModal;
use crate::core::hammer::HuntCrossleyHammer;

pub struct PianoVoice {
    pub key_params: KeyParams,
    pub midi_note: u8,
    pub pitch_name: String,
    pub target_f0: f64,
    pub pan: f64,
    pub pan_l: f64,
    pub pan_r: f64,

    pub strings: Vec<StiffStringModal>,
    pub hammer: HuntCrossleyHammer,

    pub is_key_down: bool,
    pub is_sounding: bool,
}

impl PianoVoice {
    pub fn new(key_params: KeyParams, sample_rate: f64) -> Self {
        let midi_note = key_params.midi_note;
        let pitch_name = key_params.pitch_name.clone();
        let target_f0 = key_params.target_f0;
        let pan = ((midi_note as f64 - 21.0) / (108.0 - 21.0) * 0.8 + 0.1).clamp(0.05, 0.95);
        let pan_l = ((1.0 - pan) * std::f64::consts::PI * 0.5).sin();
        let pan_r = (pan * std::f64::consts::PI * 0.5).sin();

        let has_damper = midi_note < 89;
        let mut strings = Vec::with_capacity(key_params.num_unisons);
        for (i, s_param) in key_params.strings.iter().enumerate() {
            let cents_detune = if i < key_params.detuning_cents.len() {
                key_params.detuning_cents[i]
            } else {
                0.0
            };
            let freq_ratio = 2.0f64.powf(cents_detune / 1200.0);
            let detuned_tension = s_param.tension * freq_ratio.powi(2);

            let mut detuned_param = s_param.clone();
            detuned_param.tension = detuned_tension;

            let mut s = StiffStringModal::new(detuned_param, sample_rate);
            s.has_damper = has_damper;
            if !has_damper {
                s.set_damper(false, 0.0);
                s.current_damper_depth = 0.0;
            }
            strings.push(s);
        }

        let hammer = HuntCrossleyHammer::new(key_params.hammer.clone(), sample_rate);

        Self {
            key_params,
            midi_note,
            pitch_name,
            target_f0,
            pan,
            pan_l,
            pan_r,
            strings,
            hammer,
            is_key_down: false,
            is_sounding: false,
        }
    }

    pub fn reset(&mut self) {
        self.is_sounding = false;
        self.is_key_down = false;
        self.hammer.reset();
        for s in &mut self.strings {
            s.reset();
        }
    }

    pub fn set_tuning_offset(&mut self, cents: f64) {
        for s in &mut self.strings {
            s.set_tuning_offset(cents);
        }
    }

    pub fn set_una_corda(&mut self, enabled: bool) {
        self.hammer.set_una_corda(enabled);
    }

    pub fn set_hammer_hardness(&mut self, scale: f64) {
        let s = scale.clamp(0.4, 3.0);
        self.hammer.k_h = self.key_params.hammer.stiffness * s.powf(2.0);
        self.hammer.p = (self.key_params.hammer.exponent * s.sqrt()).clamp(1.5, 3.5);
    }

    pub fn set_unison_detuning(&mut self, detune_scale: f64) {
        for (i, s) in self.strings.iter_mut().enumerate() {
            let base_cents = if i < self.key_params.detuning_cents.len() {
                self.key_params.detuning_cents[i]
            } else {
                0.0
            };
            s.set_tuning_offset(base_cents * detune_scale);
        }
    }

    pub fn set_inharmonicity_scale(&mut self, inharm_scale: f64) {
        for s in &mut self.strings {
            s.set_inharmonicity_scale(inharm_scale);
        }
    }

    pub fn set_damper_depth(&mut self, depth: f64) {
        let active = depth > 0.0;
        for s in &mut self.strings {
            s.set_damper(active, depth);
        }
    }

    pub fn get_energy(&self) -> f64 {
        let e_strings: f64 = self.strings.iter().map(|s| s.get_energy()).sum();
        let e_hammer = if self.hammer.is_active {
            0.5 * self.hammer.m_h * self.hammer.v_h.powi(2)
        } else {
            0.0
        };
        e_strings + e_hammer
    }

    pub fn note_on(&mut self, velocity: f64) {
        self.is_key_down = true;
        self.is_sounding = true;
        for s in &mut self.strings {
            s.set_damper(false, 0.0);
            s.current_damper_depth = 0.0;
        }

        let u_avg: f64 = self.strings.iter().map(|s| s.get_strike_displacement_and_velocity().0).sum::<f64>()
            / self.strings.len() as f64;
        self.hammer.strike(velocity, u_avg);
    }

    pub fn update_damper_state(&mut self, sustain_pedal: bool, pedal_depth: f64) {
        // 1. If key is physically held down by pianist's finger,
        // the whippen/damper lever keeps the damper 100% lifted off the string!
        if self.is_key_down {
            for s in &mut self.strings {
                s.set_damper(false, 0.0);
            }
            return;
        }

        // 2. Key is released: damper position is governed by sustain pedal
        // Acoustic Grand Piano damper lift curve:
        // - depth in [0.0, 0.20]: Lost motion (pedal clearance), dampers remain fully seated (effective damping = 1.0)
        // - depth in [0.20, 0.70]: Half-pedal zone, dampers gradually lift off string
        // - depth in [0.70, 1.00]: Dampers completely clear off string (effective damping = 0.0, fully sustained)
        if sustain_pedal && pedal_depth > 0.20 {
            if pedal_depth >= 0.70 {
                // Fully lifted: sustain
                for s in &mut self.strings {
                    s.set_damper(false, 0.0);
                }
            } else {
                // Half-pedal zone: progressive damping
                let norm = (pedal_depth - 0.20) / 0.50; // 0.0 to 1.0
                let effective_damping = (1.0 - norm).powi(2);
                if effective_damping < 0.02 {
                    for s in &mut self.strings {
                        s.set_damper(false, 0.0);
                    }
                } else {
                    for s in &mut self.strings {
                        s.set_damper(true, effective_damping);
                    }
                }
            }
        } else {
            // Pedal released: full damping
            for s in &mut self.strings {
                s.set_damper(true, 1.0);
            }
        }
    }

    pub fn note_off(&mut self, sustain_pedal: bool, pedal_depth: f64) {
        self.is_key_down = false;
        self.update_damper_state(sustain_pedal, pedal_depth);
    }

    pub fn set_sustain_pedal(&mut self, pedal_down: bool, depth: f64) {
        self.update_damper_state(pedal_down, depth);
    }

    #[inline]
    pub fn step(&mut self, f_coupling_t: f64, f_coupling_p: f64) -> (f64, f64, f64) {
        let num_str = self.strings.len() as f64;
        let is_una_corda = self.hammer.una_corda;
        // Physical shift: for multi-string unisons (triplets/bichords), shift drops one string
        let num_struck = if is_una_corda && self.strings.len() >= 2 {
            self.strings.len() - 1
        } else {
            self.strings.len()
        };

        let f_hammer = if self.hammer.is_active {
            let mut u_avg = 0.0;
            let mut v_avg = 0.0;
            for s in &self.strings[..num_struck] {
                let (u, v) = s.get_strike_displacement_and_velocity();
                u_avg += u;
                v_avg += v;
            }
            u_avg /= num_struck as f64;
            v_avg /= num_struck as f64;

            let f_h = self.hammer.compute_force(u_avg, v_avg);
            self.hammer.advance(f_h);
            f_h
        } else {
            0.0
        };

        let f_hammer_per_struck = f_hammer / num_struck as f64;
        let coupling_per_string_t = f_coupling_t / num_str;
        let coupling_per_string_p = f_coupling_p / num_str;

        let mut total_bridge_t = 0.0;
        let mut total_bridge_p = 0.0;
        let mut total_bridge_l = 0.0;

        for (i, s) in self.strings.iter_mut().enumerate() {
            let f_h_i = if i < num_struck { f_hammer_per_struck } else { 0.0 };
            let (fb_t, fb_p, fb_l) = s.step(f_h_i, coupling_per_string_t, coupling_per_string_p);
            total_bridge_t += fb_t;
            total_bridge_p += fb_p;
            total_bridge_l += fb_l;
        }

        (total_bridge_t, total_bridge_p, total_bridge_l)
    }
}
