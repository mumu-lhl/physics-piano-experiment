//! 88-Key Physical Parameter Generator for Concert Grand Piano in Rust.

use std::f64::consts::PI;

pub const PITCH_NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

#[derive(Debug, Clone)]
pub struct StringPhysicalParams {
    pub length: f64,
    pub radius: f64,
    pub density: f64,
    pub youngs_modulus: f64,
    pub tension: f64,
    pub sigma0: f64,
    pub sigma1: f64,
    pub strike_ratio: f64,
    pub num_modes: usize,
    pub polarization_mistuning: f64,
}

impl StringPhysicalParams {
    pub fn fundamental_hz(&self) -> f64 {
        let area = PI * self.radius.powi(2);
        let mu = self.density * area;
        (1.0 / (2.0 * self.length)) * (self.tension / mu).sqrt()
    }

    pub fn inharmonicity_b(&self) -> f64 {
        (PI.powi(3) * self.youngs_modulus * self.radius.powi(4))
            / (4.0 * self.tension * self.length.powi(2))
    }
}

#[derive(Debug, Clone)]
pub struct HammerPhysicalParams {
    pub mass: f64,
    pub stiffness: f64,
    pub exponent: f64,
    pub dissipation: f64,
    pub max_velocity: f64,
    pub velocity_gamma: f64,
}

#[derive(Debug, Clone)]
pub struct KeyParams {
    pub midi_note: u8,
    pub pitch_name: String,
    pub target_f0: f64,
    pub strings: Vec<StringPhysicalParams>,
    pub hammer: HammerPhysicalParams,
    pub num_unisons: usize,
    pub detuning_cents: Vec<f64>,
}

pub fn midi_to_pitch_name(midi_note: u8) -> String {
    let octave = (midi_note as i32 / 12) - 1;
    let name = PITCH_NAMES[(midi_note as usize) % 12];
    format!("{}{}", name, octave)
}

pub fn compute_railsback_cents(midi_note: u8) -> f64 {
    let note = midi_note as f64;
    if note < 60.0 {
        let norm = (60.0 - note) / 39.0;
        -32.0 * norm.powf(1.85)
    } else if note > 60.0 {
        let norm = (note - 60.0) / 48.0;
        35.0 * norm.powf(2.1)
    } else {
        0.0
    }
}

pub fn generate_grand_piano_parameters(num_modes: usize, stretch_tuning: bool) -> Vec<KeyParams> {
    let mut key_params = Vec::with_capacity(88);

    for midi in 21..=108u8 {
        let norm_key = (midi as f64 - 21.0) / (108.0 - 21.0);
        let stretch = if stretch_tuning { compute_railsback_cents(midi) } else { 0.0 };
        let f0 = 440.0 * 2.0f64.powf((midi as f64 - 69.0 + stretch / 100.0) / 12.0);
        let pitch_name = midi_to_pitch_name(midi);

        // 1. Discontinuous break points across keyboard
        let (num_unisons, detuning) = if midi <= 28 {
            // Single-wound copper string (A0-E1)
            (1, vec![0.0])
        } else if midi <= 34 {
            // Double-wound copper strings (F1-Bb2)
            (2, vec![-0.25, 0.25])
        } else {
            // Plain steel wire (B2-C8)
            (3, vec![-0.38, 0.0, 0.38])
        };

        // 2. Active string length
        let length = if midi <= 40 {
            1.85 - (midi as f64 - 21.0) * 0.045
        } else {
            0.065 + (0.95 - 0.065) * (1.0 - norm_key).powf(1.35)
        };

        // 3. String wire radius and density
        let (radius, density) = if midi <= 28 {
            (0.00072 - norm_key * 0.00012, 7850.0 * 3.2)
        } else if midi <= 34 {
            (0.00058 - norm_key * 0.00010, 7850.0 * 2.0)
        } else {
            (0.00048 - norm_key * 0.00015, 7850.0)
        };

        let youngs_modulus = 2.0e11;
        let area = PI * radius.powi(2);
        let mut mu = density * area;
        let mut tension = mu * (2.0 * length * f0).powi(2);

        // Tension bounds
        if tension < 450.0 {
            tension = 550.0;
            mu = tension / (2.0 * length * f0).powi(2);
        } else if tension > 1200.0 {
            tension = 1050.0;
            mu = tension / (2.0 * length * f0).powi(2);
        }
        let eff_density = mu / area;

        // Damping parameters
        let sigma0 = 0.8 - (0.4 * norm_key);
        let sigma1 = 5.0e-6 + (3.0e-5 * norm_key);
        let strike_ratio = 0.125 - (0.035 * norm_key);

        let string_params: Vec<StringPhysicalParams> = (0..num_unisons)
            .map(|_| StringPhysicalParams {
                length,
                radius,
                density: eff_density,
                youngs_modulus,
                tension,
                sigma0,
                sigma1,
                strike_ratio,
                num_modes,
                polarization_mistuning: 0.0012 + (0.0008 * norm_key),
            })
            .collect();

        // 4. Hammer felt parameters
        let hammer_mass = 0.0115 - (0.0063 * norm_key);
        let hammer_stiffness = 1.5e9 * 10.0f64.powf(norm_key * 2.8);
        let hammer_exponent = 2.0 + (1.1 * norm_key);
        let hammer_dissipation = 1.5e4 + (4.0e4 * norm_key);

        let hammer = HammerPhysicalParams {
            mass: hammer_mass,
            stiffness: hammer_stiffness,
            exponent: hammer_exponent,
            dissipation: hammer_dissipation,
            max_velocity: 5.2,
            velocity_gamma: 1.55,
        };

        key_params.push(KeyParams {
            midi_note: midi,
            pitch_name,
            target_f0: f0,
            strings: string_params,
            hammer,
            num_unisons,
            detuning_cents: detuning,
        });
    }

    key_params
}
