//! Physical parameter sets for the bass-string model.

use std::f64::consts::PI;

/// Parameters shared by one idealized bass string.
#[derive(Debug, Clone, Copy)]
pub struct BassStringParams {
    /// Open-string MIDI note.
    pub open_midi: u8,
    /// Open-string frequency in Hz. This is the tuning target used by the model.
    pub open_f0: f64,
    /// Scale length in metres.
    pub scale_length: f64,
    /// Static tension in newtons.
    pub tension: f64,
    /// Linear mass density in kg/m.
    pub linear_density: f64,
    /// Effective Young's modulus in pascals.
    pub youngs_modulus: f64,
    /// Effective circular radius in metres.
    pub radius: f64,
    /// Frequency-independent damping in s^-1.
    pub sigma0: f64,
    /// Frequency-dependent damping in m^2/s.
    pub sigma1: f64,
    /// A multiplier for the prescribed bending stiffness.
    pub bending_stiffness: f64,
}

impl BassStringParams {
    /// Cross-sectional area used by the geometric tension term.
    #[inline]
    pub fn area(self) -> f64 {
        PI * self.radius * self.radius
    }

    /// Second moment of area for the effective circular string core.
    #[inline]
    pub fn second_moment(self) -> f64 {
        PI * self.radius.powi(4) * 0.25
    }

    /// Effective length after fretting at a semitone number.
    #[inline]
    pub fn length_at_fret(self, fret: u8) -> f64 {
        self.scale_length * 2.0_f64.powf(-(fret.min(24) as f64) / 12.0)
    }

    /// Nominal frequency after fretting. The stiffness model adds the small
    /// inharmonic shift in the string core, while this value keeps tuning exact.
    #[inline]
    pub fn frequency_at_fret(self, fret: u8) -> f64 {
        self.open_f0 * 2.0_f64.powf(fret.min(24) as f64 / 12.0)
    }

    /// Four-string electric-bass tuning, low to high: E1 A1 D2 G2.
    pub const fn electric_four() -> [Self; 4] {
        [
            Self {
                open_midi: 28,
                open_f0: 41.203_444,
                scale_length: 0.864,
                tension: 95.0,
                linear_density: 0.019213_04,
                youngs_modulus: 1.9e11,
                radius: 0.00105,
                sigma0: 0.80,
                sigma1: 0.0025,
                bending_stiffness: 1.15,
            },
            Self {
                open_midi: 33,
                open_f0: 55.0,
                scale_length: 0.864,
                tension: 105.0,
                linear_density: 0.011781_04,
                youngs_modulus: 1.9e11,
                radius: 0.00092,
                sigma0: 0.86,
                sigma1: 0.0024,
                bending_stiffness: 1.10,
            },
            Self {
                open_midi: 38,
                open_f0: 73.416_19,
                scale_length: 0.864,
                tension: 112.0,
                linear_density: 0.007004_38,
                youngs_modulus: 1.9e11,
                radius: 0.00078,
                sigma0: 0.92,
                sigma1: 0.0022,
                bending_stiffness: 1.06,
            },
            Self {
                open_midi: 43,
                open_f0: 97.998_86,
                scale_length: 0.864,
                tension: 120.0,
                linear_density: 0.004194_75,
                youngs_modulus: 1.9e11,
                radius: 0.00062,
                sigma0: 1.0,
                sigma1: 0.0020,
                bending_stiffness: 1.0,
            },
        ]
    }

    /// Five-string tuning with a low B0 string followed by E1 A1 D2 G2.
    pub const fn electric_five() -> [Self; 5] {
        [
            Self {
                open_midi: 23,
                open_f0: 30.867_706,
                scale_length: 0.889,
                tension: 115.0,
                linear_density: 0.039689_55,
                youngs_modulus: 1.9e11,
                radius: 0.00125,
                sigma0: 0.75,
                sigma1: 0.0028,
                bending_stiffness: 1.18,
            },
            Self {
                open_midi: 28,
                open_f0: 41.203_444,
                scale_length: 0.889,
                tension: 108.0,
                linear_density: 0.020545_08,
                youngs_modulus: 1.9e11,
                radius: 0.00105,
                sigma0: 0.80,
                sigma1: 0.0025,
                bending_stiffness: 1.15,
            },
            Self {
                open_midi: 33,
                open_f0: 55.0,
                scale_length: 0.889,
                tension: 112.0,
                linear_density: 0.011851_55,
                youngs_modulus: 1.9e11,
                radius: 0.00092,
                sigma0: 0.86,
                sigma1: 0.0024,
                bending_stiffness: 1.10,
            },
            Self {
                open_midi: 38,
                open_f0: 73.416_19,
                scale_length: 0.889,
                tension: 118.0,
                linear_density: 0.006965_72,
                youngs_modulus: 1.9e11,
                radius: 0.00078,
                sigma0: 0.92,
                sigma1: 0.0022,
                bending_stiffness: 1.06,
            },
            Self {
                open_midi: 43,
                open_f0: 97.998_86,
                scale_length: 0.889,
                tension: 124.0,
                linear_density: 0.004093_36,
                youngs_modulus: 1.9e11,
                radius: 0.00062,
                sigma0: 1.0,
                sigma1: 0.0020,
                bending_stiffness: 1.0,
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::BassStringParams;
    use std::f64::consts::PI;

    #[test]
    fn physical_string_parameters_match_open_tuning_including_stiffness() {
        let strings: Vec<_> = BassStringParams::electric_four()
            .into_iter()
            .chain(BassStringParams::electric_five())
            .collect();
        for string in strings {
            let wave_number = PI / string.scale_length;
            let omega_squared = (string.tension * wave_number.powi(2)
                + string.youngs_modulus * string.second_moment() * wave_number.powi(4))
                / string.linear_density;
            let predicted_hz = omega_squared.sqrt() / (2.0 * PI);
            assert!((predicted_hz - string.open_f0).abs() / string.open_f0 < 2e-5);
        }
    }
}
