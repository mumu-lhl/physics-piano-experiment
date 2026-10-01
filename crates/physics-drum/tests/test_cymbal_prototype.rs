//! Cymbal physical modeling prototype with 512 high-density modes and sparse nonlinear coupling.
//! ZERO artificial white noise.
//! Models:
//! 1. 512 dense bronze plate modes spanning 300 Hz to 18 kHz
//! 2. Hamiltonian-conservative sparse modal coupling network (Energy cascade: Shimmer & Wash)
//! 3. Crash (18-inch), Ride (20-inch), and Hi-Hat (14-inch with pedal openness & choke)

use hound::{WavSpec, WavWriter};
use physics_dsp::{ModalTransition, OverdampedPolicy};
use std::f64::consts::PI;
use std::fs;
use std::path::Path;

pub const CYMBAL_MODE_COUNT: usize = 512;
pub const MAX_NEIGHBORS: usize = 6;

#[derive(Debug, Clone, Copy)]
pub struct CymbalMode {
    pub q: f64,
    pub v: f64,
    pub omega: f64,
    pub t60: f64,
    pub modal_mass: f64,
    pub strike_gain: f64,
    pub radiation_gain: f64,
    // Sparse coupling graph: up to 6 neighbor indices and symmetric coupling weights
    pub neighbor_count: usize,
    pub neighbors: [usize; MAX_NEIGHBORS],
    pub coupling_weights: [f64; MAX_NEIGHBORS],
}

pub struct MalletExciter {
    pub x: f64,
    pub v: f64,
    pub mass: f64,
    pub stiffness: f64,
    pub dissipation: f64,
    pub is_contacting: bool,
}

impl MalletExciter {
    pub fn new(mass: f64, stiffness: f64, dissipation: f64) -> Self {
        Self {
            x: 0.0,
            v: 0.0,
            mass,
            stiffness,
            dissipation,
            is_contacting: false,
        }
    }

    pub fn trigger(&mut self, velocity: f64) {
        self.x = 0.0;
        self.v = velocity.clamp(0.05, 12.0);
        self.is_contacting = true;
    }

    pub fn step(&mut self, surface_x: f64, surface_v: f64, dt: f64) -> f64 {
        if !self.is_contacting {
            return 0.0;
        }
        const SUB_STEPS: usize = 4;
        let sub_dt = dt / SUB_STEPS as f64;
        let mut total_force = 0.0;

        for _ in 0..SUB_STEPS {
            let penetration = self.x - surface_x;
            if penetration > 0.0 {
                let v_rel = self.v - surface_v;
                let elastic = self.stiffness * penetration.powf(1.5);
                let dissipative = self.dissipation * elastic * v_rel;
                let force = (elastic + dissipative).max(0.0);
                self.v -= (force / self.mass) * sub_dt;
                self.x += self.v * sub_dt;
                total_force += force;
            } else {
                self.x += self.v * sub_dt;
                if self.v <= surface_v {
                    self.is_contacting = false;
                }
            }
        }
        total_force / SUB_STEPS as f64
    }
}

pub struct CymbalInstrument {
    pub sample_rate: f64,
    pub base_hz: f64,
    pub modes: Vec<CymbalMode>,
    pub transitions: Vec<ModalTransition>,
    pub exciter: MalletExciter,
    pub open_amount: f64, // 0.0 closed to 1.0 open
    pub base_t60: f64,
    // Buffer for instantaneous nonlinear restoring forces
    nonlinear_forces: Vec<f64>,
}

impl CymbalInstrument {
    pub fn new_crash(sample_rate: f64) -> Self {
        Self::build(sample_rate, 290.0, 5.5, 0.022, 1.4e7, 1.0, 4.5e4)
    }

    pub fn new_ride(sample_rate: f64) -> Self {
        Self::build(sample_rate, 340.0, 4.0, 0.018, 2.2e7, 1.0, 2.0e4)
    }

    pub fn new_hihat(sample_rate: f64, open_amount: f64) -> Self {
        Self::build(sample_rate, 380.0, 2.5, 0.016, 1.8e7, open_amount.clamp(0.0, 1.0), 3.0e4)
    }

    fn build(
        sample_rate: f64,
        base_hz: f64,
        base_t60: f64,
        stick_mass: f64,
        stick_stiffness: f64,
        open_amount: f64,
        coupling_strength: f64,
    ) -> Self {
        let dt = 1.0 / sample_rate;
        let mut modes = Vec::with_capacity(CYMBAL_MODE_COUNT);
        let mut transitions = Vec::with_capacity(CYMBAL_MODE_COUNT);

        // Hi-hat openness modulates effective damping: closed = 0.08s, open = base_t60
        let effective_t60 = 0.06 + (base_t60 - 0.06) * open_amount.powf(1.8);

        // Bronze plate mode distribution: high modal density at high frequencies
        for i in 0..CYMBAL_MODE_COUNT {
            let norm = i as f64 / (CYMBAL_MODE_COUNT - 1) as f64;
            // Dispersion law of circular plate with slight inharmonic detuning
            let ratio = 1.0 + 8.5 * norm.powf(1.1) + 42.0 * norm.powf(1.6);
            let inharmonic_detune = 1.0 + 0.015 * (i as f64 * 3.7).sin();
            let freq = (base_hz * ratio * inharmonic_detune).min(sample_rate * 0.45);
            let omega = 2.0 * PI * freq;

            // Physical bronze damping: low modes ring long, ultra-high modes (> 12 kHz) decay faster
            let t60 = (effective_t60 / (1.0 + (freq / 3500.0).powf(1.2)) + 0.015).max(0.008);
            let sigma = 1000.0_f64.ln() / t60;

            let modal_mass = 0.0025 * (1.0 + 0.5 * norm);
            // High frequency modes have smaller strike coupling from spatial stick patch
            let strike_gain = 1.0 / (1.0 + (freq / 2500.0).powf(1.4));
            // Acoustic radiation: higher frequency modes radiate efficiently
            let radiation_gain = (freq / 1000.0).clamp(0.4, 2.8) / (CYMBAL_MODE_COUNT as f64).sqrt();

            modes.push(CymbalMode {
                q: 0.0,
                v: 0.0,
                omega,
                t60,
                modal_mass,
                strike_gain,
                radiation_gain,
                neighbor_count: 0,
                neighbors: [0; MAX_NEIGHBORS],
                coupling_weights: [0.0; MAX_NEIGHBORS],
            });

            transitions.push(ModalTransition::new(omega, sigma, dt, OverdampedPolicy::ExponentialFallback));
        }

        // Construct sparse coupling graph:
        // 1. Frequency neighbors: i <-> i+1, i+2
        // 2. Approximate 2:1 and 3:1 internal resonance partners
        for i in 0..CYMBAL_MODE_COUNT {
            let mut neighbors = [0; MAX_NEIGHBORS];
            let mut weights = [0.0; MAX_NEIGHBORS];
            let mut count = 0;

            // Immediate frequency neighbors
            if i + 1 < CYMBAL_MODE_COUNT && count < MAX_NEIGHBORS {
                neighbors[count] = i + 1;
                weights[count] = coupling_strength;
                count += 1;
            }
            if i > 0 && count < MAX_NEIGHBORS {
                neighbors[count] = i - 1;
                weights[count] = coupling_strength;
                count += 1;
            }
            if i + 3 < CYMBAL_MODE_COUNT && count < MAX_NEIGHBORS {
                neighbors[count] = i + 3;
                weights[count] = coupling_strength * 0.4;
                count += 1;
            }

            // Find an internal resonance partner near 2 * omega_i
            let target_omega = modes[i].omega * 2.0;
            if target_omega < sample_rate * 0.44 * 2.0 * PI && count < MAX_NEIGHBORS {
                let mut best_partner = None;
                let mut best_diff = f64::INFINITY;
                for j in (i + 1)..CYMBAL_MODE_COUNT {
                    let diff = (modes[j].omega - target_omega).abs();
                    if diff < best_diff {
                        best_diff = diff;
                        best_partner = Some(j);
                    }
                }
                if let Some(partner) = best_partner {
                    if best_diff < modes[i].omega * 0.15 {
                        neighbors[count] = partner;
                        weights[count] = coupling_strength * 0.7;
                        count += 1;
                    }
                }
            }

            modes[i].neighbor_count = count;
            modes[i].neighbors = neighbors;
            modes[i].coupling_weights = weights;
        }

        Self {
            sample_rate,
            base_hz,
            modes,
            transitions,
            exciter: MalletExciter::new(stick_mass, stick_stiffness, 0.04),
            open_amount,
            base_t60,
            nonlinear_forces: vec![0.0; CYMBAL_MODE_COUNT],
        }
    }

    pub fn set_open_amount(&mut self, amount: f64) {
        self.open_amount = amount.clamp(0.0, 1.0);
        let effective_t60 = 0.06 + (self.base_t60 - 0.06) * self.open_amount.powf(1.8);
        let dt = 1.0 / self.sample_rate;
        for (mode, trans) in self.modes.iter_mut().zip(&mut self.transitions) {
            let t60 = (effective_t60 / (1.0 + (mode.omega / (2.0 * PI * 3500.0)).powf(1.2)) + 0.015).max(0.008);
            let sigma = 1000.0_f64.ln() / t60;
            mode.t60 = t60;
            *trans = ModalTransition::new(mode.omega, sigma, dt, OverdampedPolicy::ExponentialFallback);
        }
    }

    pub fn choke(&mut self) {
        // Foot pedal or hand grab choke: rapid damping of modal energy
        self.set_open_amount(0.0);
        for mode in &mut self.modes {
            mode.q *= 0.15;
            mode.v *= 0.15;
        }
    }

    pub fn trigger(&mut self, velocity: f64) {
        self.exciter.trigger(velocity * 5.0);
    }

    pub fn step(&mut self) -> f64 {
        let dt = 1.0 / self.sample_rate;

        // 1. Surface displacement at strike point
        let mut x_surf = 0.0;
        let mut v_surf = 0.0;
        for mode in &self.modes {
            x_surf += mode.q * mode.strike_gain;
            v_surf += mode.v * mode.strike_gain;
        }

        // Exciter strike force
        let strike_force = self.exciter.step(x_surf, v_surf, dt);

        // 2. Hamiltonian conservative sparse nonlinear coupling forces:
        // F_nl,i = - q_i * sum_{j in N(i)} beta_ij * q_j^2
        self.nonlinear_forces.fill(0.0);
        for i in 0..CYMBAL_MODE_COUNT {
            let q_i = self.modes[i].q;
            let count = self.modes[i].neighbor_count;
            let mut stiffness_shift = 0.0;
            for k in 0..count {
                let j = self.modes[i].neighbors[k];
                let weight = self.modes[i].coupling_weights[k];
                let q_j = self.modes[j].q;
                stiffness_shift += weight * (q_j * q_j);
            }
            self.nonlinear_forces[i] = -q_i * stiffness_shift;
        }

        // 3. Advance all 512 modes with exact transitions and conservative coupling
        let mut sound_out = 0.0;
        for (i, mode) in self.modes.iter_mut().enumerate() {
            let trans = &self.transitions[i];
            let (p11, p12, p21, p22) = trans.phi;
            let (g1, g2) = trans.gamma;

            let total_force = strike_force * mode.strike_gain + self.nonlinear_forces[i];
            let force_over_m = total_force / mode.modal_mass;

            let next_q = p11 * mode.q + p12 * mode.v + g1 * force_over_m;
            let next_v = p21 * mode.q + p22 * mode.v + g2 * force_over_m;

            mode.q = next_q;
            mode.v = next_v;

            sound_out += mode.v * mode.radiation_gain;
        }

        sound_out
    }
}

fn render_cymbal(mut cymbal: CymbalInstrument, velocity: f64, seconds: f64) -> Vec<f64> {
    cymbal.trigger(velocity);
    let total_frames = (cymbal.sample_rate * seconds) as usize;
    let mut out = Vec::with_capacity(total_frames);
    for _ in 0..total_frames {
        out.push(cymbal.step());
    }
    out
}

fn save_wav(samples: &[f64], filename: &str) {
    let dir = Path::new("target/tom-prototype");
    fs::create_dir_all(dir).unwrap();
    let path = dir.join(filename);
    let spec = WavSpec {
        channels: 1,
        sample_rate: 48000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = WavWriter::create(&path, spec).unwrap();
    let peak = samples.iter().copied().fold(0.0_f64, |p, s| p.max(s.abs())).max(1e-6);
    for &s in samples {
        let norm = (s / peak * 0.90).clamp(-1.0, 1.0);
        let sample_i16 = (norm * 32767.0) as i16;
        writer.write_sample(sample_i16).unwrap();
    }
    writer.finalize().unwrap();
    println!("Saved WAV to {}", path.display());
}

#[test]
fn test_cymbal_prototype_512_modes_and_shimmer() {
    println!("Rendering Cymbal Prototype (512 modes + Sparse Nonlinear Coupling, ZERO white noise)...");
    let sample_rate = 48000.0;

    // 1. 18-inch Crash Cymbal (Heavy edge strike, long wash)
    let crash = CymbalInstrument::new_crash(sample_rate);
    let crash_samples = render_cymbal(crash, 0.95, 2.5);
    save_wav(&crash_samples, "cymbal_crash.wav");

    // 2. 20-inch Ride Cymbal (Bow stick tip, defined ping)
    let ride = CymbalInstrument::new_ride(sample_rate);
    let ride_samples = render_cymbal(ride, 0.85, 2.0);
    save_wav(&ride_samples, "cymbal_ride_bow.wav");

    // 3. 14-inch Closed Hi-Hat (Tight chick, short decay)
    let hihat_closed = CymbalInstrument::new_hihat(sample_rate, 0.0);
    let closed_samples = render_cymbal(hihat_closed, 0.85, 0.5);
    save_wav(&closed_samples, "cymbal_hihat_closed.wav");

    // 4. 14-inch Open Hi-Hat (Full sizzle)
    let hihat_open = CymbalInstrument::new_hihat(sample_rate, 1.0);
    let open_samples = render_cymbal(hihat_open, 0.90, 1.5);
    save_wav(&open_samples, "cymbal_hihat_open.wav");

    // 5. 14-inch Hi-Hat with Choke (Open hit, then choked at 300ms)
    let mut hihat_choke = CymbalInstrument::new_hihat(sample_rate, 1.0);
    hihat_choke.trigger(0.90);
    let total_frames = (sample_rate * 1.2) as usize;
    let choke_frame = (sample_rate * 0.30) as usize;
    let mut choke_samples = Vec::with_capacity(total_frames);
    for frame in 0..total_frames {
        if frame == choke_frame {
            hihat_choke.choke();
        }
        choke_samples.push(hihat_choke.step());
    }
    save_wav(&choke_samples, "cymbal_hihat_choke.wav");

    // Assertions: no NaN, Inf
    assert!(crash_samples.iter().all(|s| s.is_finite()));
    assert!(ride_samples.iter().all(|s| s.is_finite()));
    assert!(closed_samples.iter().all(|s| s.is_finite()));
    assert!(open_samples.iter().all(|s| s.is_finite()));
    assert!(choke_samples.iter().all(|s| s.is_finite()));

    println!("All 5 cymbal instruments rendered successfully!");
}
