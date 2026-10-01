//! Factory Presets for Physics Guitar.
//!
//! Provides signature acoustic and electric tones modeled after iconic
//! instruments, amps, and playing styles.

use physics_ui::{Language, translate};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FactoryPreset {
    MartinD28Fingerstyle = 0,
    DreadnoughtStrummer = 1,
    StratCleanChime = 2,
    TexasBluesBreakup = 3,
    LesPaulWarmJazz = 4,
    HeavyMetalChug = 5,
}

#[derive(Debug, Clone, Copy)]
pub struct PresetValues {
    pub mode: i32,        // 0 = Electric, 1 = Acoustic
    pub pluck_style: i32, // 0 = Plectrum, 1 = Finger
    pub pickup_pos: i32,  // 0 = Bridge, 1 = Mid, 2 = Neck, 3 = B+N, 4 = B+M
    pub pickup_type: i32, // 0 = Single, 1 = Humbucker
    pub tone: f32,        // 0.0 ~ 1.0
    pub amp_drive: f32,   // 0.0 ~ 1.0
    pub cab_enabled: bool,
    pub palm_mute: f32,      // 0.0 ~ 1.0
    pub strum_speed: f32,    // ms
    pub fret_buzz: f32,      // 0.0 ~ 1.0
    pub pluck_pos: f32,      // 0.05 ~ 0.35
    pub finger_squeak: f32,  // 0.0 ~ 1.0
    pub groove_pattern: i32, // 0 = Off, 1 = Folk, 2 = Ballad, 3 = Funk, 4 = Rock
}

impl FactoryPreset {
    pub const ALL: [FactoryPreset; 6] = [
        FactoryPreset::MartinD28Fingerstyle,
        FactoryPreset::DreadnoughtStrummer,
        FactoryPreset::StratCleanChime,
        FactoryPreset::TexasBluesBreakup,
        FactoryPreset::LesPaulWarmJazz,
        FactoryPreset::HeavyMetalChug,
    ];

    pub fn all() -> &'static [FactoryPreset] {
        &Self::ALL
    }

    pub fn from_index(idx: usize) -> Self {
        match idx {
            1 => FactoryPreset::DreadnoughtStrummer,
            2 => FactoryPreset::StratCleanChime,
            3 => FactoryPreset::TexasBluesBreakup,
            4 => FactoryPreset::LesPaulWarmJazz,
            5 => FactoryPreset::HeavyMetalChug,
            _ => FactoryPreset::MartinD28Fingerstyle,
        }
    }

    pub fn name(&self, lang: Language) -> &'static str {
        match self {
            FactoryPreset::MartinD28Fingerstyle => {
                translate(lang, "guitar.factory-preset.0", "Martin D-28 Fingerstyle")
            }
            FactoryPreset::DreadnoughtStrummer => {
                translate(lang, "guitar.factory-preset.1", "Dreadnought Strummer")
            }
            FactoryPreset::StratCleanChime => {
                translate(lang, "guitar.factory-preset.2", "Strat Clean Chime")
            }
            FactoryPreset::TexasBluesBreakup => {
                translate(lang, "guitar.factory-preset.3", "Texas Blues Breakup")
            }
            FactoryPreset::LesPaulWarmJazz => {
                translate(lang, "guitar.factory-preset.4", "Les Paul Warm Jazz")
            }
            FactoryPreset::HeavyMetalChug => {
                translate(lang, "guitar.factory-preset.5", "Heavy Metal Chug")
            }
        }
    }

    pub fn values(&self) -> PresetValues {
        match self {
            FactoryPreset::MartinD28Fingerstyle => PresetValues {
                mode: 1,        // Acoustic
                pluck_style: 1, // Finger
                pickup_pos: 0,
                pickup_type: 0,
                tone: 1.0,
                amp_drive: 0.0,
                cab_enabled: false,
                palm_mute: 0.0,
                strum_speed: 14.0,
                fret_buzz: 0.25,
                pluck_pos: 0.22,
                finger_squeak: 0.45,
                groove_pattern: 0,
            },
            FactoryPreset::DreadnoughtStrummer => PresetValues {
                mode: 1,        // Acoustic
                pluck_style: 0, // Plectrum
                pickup_pos: 0,
                pickup_type: 0,
                tone: 0.90,
                amp_drive: 0.0,
                cab_enabled: false,
                palm_mute: 0.0,
                strum_speed: 26.0,
                fret_buzz: 0.38,
                pluck_pos: 0.15,
                finger_squeak: 0.40,
                groove_pattern: 1, // Folk 4/4 Basic
            },
            FactoryPreset::StratCleanChime => PresetValues {
                mode: 0,        // Electric
                pluck_style: 0, // Plectrum
                pickup_pos: 4,  // Bridge + Middle
                pickup_type: 0, // Single Coil
                tone: 0.95,
                amp_drive: 0.15,
                cab_enabled: true,
                palm_mute: 0.0,
                strum_speed: 6.0,
                fret_buzz: 0.30,
                pluck_pos: 0.16,
                finger_squeak: 0.25,
                groove_pattern: 0,
            },
            FactoryPreset::TexasBluesBreakup => PresetValues {
                mode: 0,        // Electric
                pluck_style: 1, // Finger
                pickup_pos: 2,  // Neck
                pickup_type: 0, // Single Coil
                tone: 0.70,
                amp_drive: 0.58,
                cab_enabled: true,
                palm_mute: 0.08,
                strum_speed: 10.0,
                fret_buzz: 0.45,
                pluck_pos: 0.18,
                finger_squeak: 0.35,
                groove_pattern: 0,
            },
            FactoryPreset::LesPaulWarmJazz => PresetValues {
                mode: 0,        // Electric
                pluck_style: 1, // Finger
                pickup_pos: 2,  // Neck
                pickup_type: 1, // Humbucker
                tone: 0.35,
                amp_drive: 0.08,
                cab_enabled: true,
                palm_mute: 0.0,
                strum_speed: 8.0,
                fret_buzz: 0.15,
                pluck_pos: 0.26,
                finger_squeak: 0.15,
                groove_pattern: 0,
            },
            FactoryPreset::HeavyMetalChug => PresetValues {
                mode: 0,        // Electric
                pluck_style: 0, // Plectrum
                pickup_pos: 0,  // Bridge
                pickup_type: 1, // Humbucker
                tone: 1.0,
                amp_drive: 0.88,
                cab_enabled: true,
                palm_mute: 0.65,
                strum_speed: 4.0,
                fret_buzz: 0.50,
                pluck_pos: 0.10,
                finger_squeak: 0.20,
                groove_pattern: 4, // Rock 8th Chug
            },
        }
    }
}
