//! First-principles physical parameters for 6-string acoustic and electric guitars.
//! Based on standard mechanical constants and data from Guitar Physical Modeling Synthesis research.

/// Physical string specification for a single guitar string.
#[derive(Debug, Clone)]
pub struct GuitarStringParams {
    /// String index: 1 (high E) to 6 (low E)
    pub string_index: u8,
    /// Open string MIDI note number (e.g. 64 for E4, 40 for E2)
    pub open_midi_note: u8,
    /// Open string fundamental frequency f0 (Hz)
    pub open_f0: f64,
    /// Nominal scale length L (meters, standard 0.648m = 25.5")
    pub scale_length: f64,
    /// Wire diameter d (meters)
    pub diameter: f64,
    /// Linear mass density mu = rho * A (kg/m)
    pub linear_density: f64,
    /// Effective Young's modulus E (Pascals)
    pub youngs_modulus: f64,
    /// Nominal static tension T0 (Newtons)
    pub tension: f64,
    /// Cross-sectional area moment of inertia I (m^4)
    pub moment_of_inertia: f64,
    /// Inharmonicity coefficient B
    pub inharmonicity_b: f64,
    /// Air loss damping sigma_0 (s^-1)
    pub sigma0: f64,
    /// Internal viscoelastic friction damping sigma_1 (m^2/s)
    pub sigma1: f64,
    /// Number of active modal oscillators
    pub num_modes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuitarStringSetType {
    /// Nickel-plated steel .010-.046 set (Electric Guitar)
    Electric010,
    /// Phosphor Bronze .012-.053 set (Acoustic Folk Guitar)
    Acoustic012,
    /// Standard Tension Clear Nylon (Classical Guitar)
    ClassicalNylon,
}

impl GuitarStringParams {
    /// Computes effective vibrating length when fretted at fret `k` (0 to 24).
    /// L_eff = L * 2^(-k / 12)
    pub fn effective_length_at_fret(&self, fret: u8) -> f64 {
        self.scale_length * 2.0f64.powf(-(fret as f64) / 12.0)
    }

    /// Computes fundamental frequency at fret `k`.
    pub fn frequency_at_fret(&self, fret: u8) -> f64 {
        self.open_f0 * 2.0f64.powf((fret as f64) / 12.0)
    }

    /// Computes distance from nut to fret `k` (meters).
    /// x_fret = L * (1 - 2^(-k / 12))
    pub fn fret_position_from_nut(&self, fret: u8) -> f64 {
        self.scale_length * (1.0 - 2.0f64.powf(-(fret as f64) / 12.0))
    }
}

/// Generates the standard 6-string physical parameters based on set type.
pub fn generate_guitar_string_set(set_type: GuitarStringSetType, num_modes: usize) -> Vec<GuitarStringParams> {
    let scale_length = 0.648; // 25.5 inches standard

    match set_type {
        GuitarStringSetType::Electric010 => vec![
            // 1st string (E4, 329.63 Hz, .010")
            GuitarStringParams {
                string_index: 1,
                open_midi_note: 64,
                open_f0: 329.63,
                scale_length,
                diameter: 0.000254,
                linear_density: 4.05e-4,
                youngs_modulus: 200.0e9,
                tension: 71.8,
                moment_of_inertia: 2.04e-16,
                inharmonicity_b: 1.34e-4,
                sigma0: 0.52,
                sigma1: 1.2e-5,
                num_modes,
            },
            // 2nd string (B3, 246.94 Hz, .013")
            GuitarStringParams {
                string_index: 2,
                open_midi_note: 59,
                open_f0: 246.94,
                scale_length,
                diameter: 0.000330,
                linear_density: 6.85e-4,
                youngs_modulus: 200.0e9,
                tension: 68.4,
                moment_of_inertia: 5.83e-16,
                inharmonicity_b: 4.01e-4,
                sigma0: 0.65,
                sigma1: 1.5e-5,
                num_modes,
            },
            // 3rd string (G3, 196.00 Hz, .017")
            GuitarStringParams {
                string_index: 3,
                open_midi_note: 55,
                open_f0: 196.00,
                scale_length,
                diameter: 0.000432,
                linear_density: 1.17e-3,
                youngs_modulus: 200.0e9,
                tension: 73.9,
                moment_of_inertia: 1.71e-15,
                inharmonicity_b: 1.09e-3,
                sigma0: 0.85,
                sigma1: 2.0e-5,
                num_modes,
            },
            // 4th string (D3, 146.83 Hz, .026" wound)
            GuitarStringParams {
                string_index: 4,
                open_midi_note: 50,
                open_f0: 146.83,
                scale_length,
                diameter: 0.000660,
                linear_density: 2.38e-3,
                youngs_modulus: 105.0e9,
                tension: 83.2,
                moment_of_inertia: 2.20e-15,
                inharmonicity_b: 3.12e-4,
                sigma0: 1.15,
                sigma1: 3.2e-5,
                num_modes,
            },
            // 5th string (A2, 110.00 Hz, .036" wound)
            GuitarStringParams {
                string_index: 5,
                open_midi_note: 45,
                open_f0: 110.00,
                scale_length,
                diameter: 0.000914,
                linear_density: 4.45e-3,
                youngs_modulus: 95.0e9,
                tension: 85.5,
                moment_of_inertia: 5.80e-15,
                inharmonicity_b: 2.85e-4,
                sigma0: 1.45,
                sigma1: 4.5e-5,
                num_modes,
            },
            // 6th string (E2, 82.41 Hz, .046" wound)
            GuitarStringParams {
                string_index: 6,
                open_midi_note: 40,
                open_f0: 82.41,
                scale_length,
                diameter: 0.001168,
                linear_density: 7.15e-3,
                youngs_modulus: 90.0e9,
                tension: 77.8,
                moment_of_inertia: 1.25e-14,
                inharmonicity_b: 3.18e-4,
                sigma0: 1.80,
                sigma1: 6.0e-5,
                num_modes,
            },
        ],
        GuitarStringSetType::Acoustic012 => vec![
            // Phosphor Bronze .012 - .053
            GuitarStringParams {
                string_index: 1,
                open_midi_note: 64,
                open_f0: 329.63,
                scale_length,
                diameter: 0.000305,
                linear_density: 5.82e-4,
                youngs_modulus: 210.0e9,
                tension: 103.6,
                moment_of_inertia: 4.25e-16,
                inharmonicity_b: 2.03e-4,
                sigma0: 0.90,
                sigma1: 3.0e-5,
                num_modes,
            },
            GuitarStringParams {
                string_index: 2,
                open_midi_note: 59,
                open_f0: 246.94,
                scale_length,
                diameter: 0.000406,
                linear_density: 1.03e-3,
                youngs_modulus: 210.0e9,
                tension: 105.0,
                moment_of_inertia: 1.33e-15,
                inharmonicity_b: 4.80e-4,
                sigma0: 1.05,
                sigma1: 3.5e-5,
                num_modes,
            },
            GuitarStringParams {
                string_index: 3,
                open_midi_note: 55,
                open_f0: 196.00,
                scale_length,
                diameter: 0.000610,
                linear_density: 2.10e-3,
                youngs_modulus: 120.0e9,
                tension: 120.5,
                moment_of_inertia: 1.80e-15,
                inharmonicity_b: 2.60e-4,
                sigma0: 1.25,
                sigma1: 4.2e-5,
                num_modes,
            },
            GuitarStringParams {
                string_index: 4,
                open_midi_note: 50,
                open_f0: 146.83,
                scale_length,
                diameter: 0.000813,
                linear_density: 3.65e-3,
                youngs_modulus: 110.0e9,
                tension: 122.0,
                moment_of_inertia: 4.50e-15,
                inharmonicity_b: 2.80e-4,
                sigma0: 1.45,
                sigma1: 5.0e-5,
                num_modes,
            },
            GuitarStringParams {
                string_index: 5,
                open_midi_note: 45,
                open_f0: 110.00,
                scale_length,
                diameter: 0.001067,
                linear_density: 6.20e-3,
                youngs_modulus: 105.0e9,
                tension: 118.0,
                moment_of_inertia: 1.10e-14,
                inharmonicity_b: 3.10e-4,
                sigma0: 1.70,
                sigma1: 6.2e-5,
                num_modes,
            },
            GuitarStringParams {
                string_index: 6,
                open_midi_note: 40,
                open_f0: 82.41,
                scale_length,
                diameter: 0.001346,
                linear_density: 9.65e-3,
                youngs_modulus: 100.0e9,
                tension: 115.4,
                moment_of_inertia: 2.10e-14,
                inharmonicity_b: 4.27e-4,
                sigma0: 1.95,
                sigma1: 7.8e-5,
                num_modes,
            },
        ],
        GuitarStringSetType::ClassicalNylon => vec![
            // Classical Nylon Standard Tension
            GuitarStringParams {
                string_index: 1,
                open_midi_note: 64,
                open_f0: 329.63,
                scale_length: 0.650,
                diameter: 0.000711,
                linear_density: 4.38e-4,
                youngs_modulus: 5.2e9,
                tension: 73.1,
                moment_of_inertia: 1.26e-14,
                inharmonicity_b: 2.11e-5,
                sigma0: 0.65,
                sigma1: 2.0e-5,
                num_modes,
            },
            GuitarStringParams {
                string_index: 2,
                open_midi_note: 59,
                open_f0: 246.94,
                scale_length: 0.650,
                diameter: 0.000813,
                linear_density: 5.75e-4,
                youngs_modulus: 5.2e9,
                tension: 54.5,
                moment_of_inertia: 2.15e-14,
                inharmonicity_b: 3.40e-5,
                sigma0: 0.70,
                sigma1: 2.2e-5,
                num_modes,
            },
            GuitarStringParams {
                string_index: 3,
                open_midi_note: 55,
                open_f0: 196.00,
                scale_length: 0.650,
                diameter: 0.001016,
                linear_density: 8.95e-4,
                youngs_modulus: 5.2e9,
                tension: 52.8,
                moment_of_inertia: 5.20e-14,
                inharmonicity_b: 5.10e-5,
                sigma0: 0.85,
                sigma1: 2.6e-5,
                num_modes,
            },
            GuitarStringParams {
                string_index: 4,
                open_midi_note: 50,
                open_f0: 146.83,
                scale_length: 0.650,
                diameter: 0.000737,
                linear_density: 1.85e-3,
                youngs_modulus: 12.0e9,
                tension: 68.2,
                moment_of_inertia: 1.45e-14,
                inharmonicity_b: 8.50e-5,
                sigma0: 1.10,
                sigma1: 3.5e-5,
                num_modes,
            },
            GuitarStringParams {
                string_index: 5,
                open_midi_note: 45,
                open_f0: 110.00,
                scale_length: 0.650,
                diameter: 0.000889,
                linear_density: 2.80e-3,
                youngs_modulus: 10.5e9,
                tension: 64.0,
                moment_of_inertia: 3.05e-14,
                inharmonicity_b: 1.15e-4,
                sigma0: 1.30,
                sigma1: 4.5e-5,
                num_modes,
            },
            GuitarStringParams {
                string_index: 6,
                open_midi_note: 40,
                open_f0: 82.41,
                scale_length: 0.650,
                diameter: 0.001092,
                linear_density: 4.35e-3,
                youngs_modulus: 9.8e9,
                tension: 65.5,
                moment_of_inertia: 4.50e-14,
                inharmonicity_b: 1.58e-4,
                sigma0: 1.55,
                sigma1: 5.8e-5,
                num_modes,
            },
        ],
    }
}
