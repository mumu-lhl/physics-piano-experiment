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
        let pan = (midi_note as f64 - 21.0) / (108.0 - 21.0) * 0.8 + 0.1;

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

    pub fn note_off(&mut self, sustain_pedal: bool) {
        self.is_key_down = false;
        if !sustain_pedal {
            for s in &mut self.strings {
                s.set_damper(true, 1.0);
            }
        }
    }

    pub fn set_sustain_pedal(&mut self, pedal_down: bool, depth: f64) {
        if pedal_down {
            if depth >= 0.99 {
                for s in &mut self.strings {
                    s.set_damper(false, 0.0);
                }
            } else {
                let effective_damping = 1.0 - depth;
                for s in &mut self.strings {
                    s.set_damper(true, effective_damping);
                }
            }
        } else if !self.is_key_down {
            for s in &mut self.strings {
                s.set_damper(true, 1.0);
            }
        }
    }

    #[inline]
    pub fn step(&mut self, f_coupling_t: f64, f_coupling_p: f64) -> (f64, f64, f64) {
        let num_str = self.strings.len() as f64;

        let f_hammer = if self.hammer.is_active {
            let mut u_avg = 0.0;
            let mut v_avg = 0.0;
            for s in &self.strings {
                let (u, v) = s.get_strike_displacement_and_velocity();
                u_avg += u;
                v_avg += v;
            }
            u_avg /= num_str;
            v_avg /= num_str;

            let f_h = self.hammer.compute_force(u_avg, v_avg);
            self.hammer.advance(f_h);
            f_h
        } else {
            0.0
        };

        let f_hammer_per_string = f_hammer / num_str;
        let coupling_per_string_t = f_coupling_t / num_str;
        let coupling_per_string_p = f_coupling_p / num_str;

        let mut total_bridge_t = 0.0;
        let mut total_bridge_p = 0.0;
        let mut total_bridge_l = 0.0;

        for s in &mut self.strings {
            let (fb_t, fb_p, fb_l) = s.step(f_hammer_per_string, coupling_per_string_t, coupling_per_string_p);
            total_bridge_t += fb_t;
            total_bridge_p += fb_p;
            total_bridge_l += fb_l;
        }

        (total_bridge_t, total_bridge_p, total_bridge_l)
    }
}
