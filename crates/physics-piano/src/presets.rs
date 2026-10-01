//! Factory Presets for Physics Piano.
//!
//! Provides signature acoustic grand and upright presets covering classical,
//! pop/jazz, intimate chamber, vintage upright, and cinematic ambient aesthetics.

use physics_ui::{Language, translate};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FactoryPreset {
    SteinwayConcertD = 0,
    BrightPopGrand = 1,
    WarmIntimateChamber = 2,
    VintageUpright = 3,
    CinematicDream = 4,
    ClassicalPureSolo = 5,
}

#[derive(Debug, Clone, Copy)]
pub struct PresetValues {
    pub inharmonicity_scale: f32,
    pub hammer_hardness: f32,
    pub unison_detuning: f32,
    pub phantom_gain: f32,
    pub key_noise: f32,
    pub damper_noise: f32,
    pub pedal_noise: f32,
    pub mic_close: f32,
    pub mic_player: f32,
    pub mic_ambient: f32,
    pub lid_angle: f32,
    pub velocity_curve: f32,
}

impl FactoryPreset {
    pub const ALL: [FactoryPreset; 6] = [
        FactoryPreset::SteinwayConcertD,
        FactoryPreset::BrightPopGrand,
        FactoryPreset::WarmIntimateChamber,
        FactoryPreset::VintageUpright,
        FactoryPreset::CinematicDream,
        FactoryPreset::ClassicalPureSolo,
    ];

    pub fn all() -> &'static [FactoryPreset] {
        &Self::ALL
    }

    pub fn from_index(idx: usize) -> Self {
        match idx {
            1 => FactoryPreset::BrightPopGrand,
            2 => FactoryPreset::WarmIntimateChamber,
            3 => FactoryPreset::VintageUpright,
            4 => FactoryPreset::CinematicDream,
            5 => FactoryPreset::ClassicalPureSolo,
            _ => FactoryPreset::SteinwayConcertD,
        }
    }

    pub fn name(&self, lang: Language) -> &'static str {
        match self {
            FactoryPreset::SteinwayConcertD => {
                translate(lang, "piano.factory-preset.0", "Steinway D Concert Grand")
            }
            FactoryPreset::BrightPopGrand => {
                translate(lang, "piano.factory-preset.1", "Bright Pop & Jazz Grand")
            }
            FactoryPreset::WarmIntimateChamber => {
                translate(lang, "piano.factory-preset.2", "Warm Intimate Chamber")
            }
            FactoryPreset::VintageUpright => {
                translate(lang, "piano.factory-preset.3", "Vintage Honky-Tonk Upright")
            }
            FactoryPreset::CinematicDream => {
                translate(lang, "piano.factory-preset.4", "Cinematic Ambient Dream")
            }
            FactoryPreset::ClassicalPureSolo => {
                translate(lang, "piano.factory-preset.5", "Classical Pure Solo")
            }
        }
    }

    pub fn values(&self) -> PresetValues {
        match self {
            FactoryPreset::SteinwayConcertD => PresetValues {
                inharmonicity_scale: 1.0,
                hammer_hardness: 1.05,
                unison_detuning: 1.0,
                phantom_gain: 1.0,
                key_noise: 1.0,
                damper_noise: 1.0,
                pedal_noise: 1.0,
                mic_close: 0.0,
                mic_player: -3.0,
                mic_ambient: -6.0,
                lid_angle: 45.0,
                velocity_curve: 0.0,
            },
            FactoryPreset::BrightPopGrand => PresetValues {
                inharmonicity_scale: 0.95,
                hammer_hardness: 1.55,
                unison_detuning: 1.25,
                phantom_gain: 1.2,
                key_noise: 1.2,
                damper_noise: 0.8,
                pedal_noise: 0.9,
                mic_close: 2.0,
                mic_player: 0.0,
                mic_ambient: -12.0,
                lid_angle: 60.0,
                velocity_curve: -0.2,
            },
            FactoryPreset::WarmIntimateChamber => PresetValues {
                inharmonicity_scale: 1.05,
                hammer_hardness: 0.75,
                unison_detuning: 0.85,
                phantom_gain: 0.8,
                key_noise: 0.7,
                damper_noise: 1.1,
                pedal_noise: 0.8,
                mic_close: -2.0,
                mic_player: 1.0,
                mic_ambient: -4.0,
                lid_angle: 30.0,
                velocity_curve: 0.1,
            },
            FactoryPreset::VintageUpright => PresetValues {
                inharmonicity_scale: 1.65,
                hammer_hardness: 1.35,
                unison_detuning: 2.10,
                phantom_gain: 0.6,
                key_noise: 1.7,
                damper_noise: 1.6,
                pedal_noise: 1.5,
                mic_close: 1.0,
                mic_player: -1.0,
                mic_ambient: -18.0,
                lid_angle: 15.0,
                velocity_curve: 0.0,
            },
            FactoryPreset::CinematicDream => PresetValues {
                inharmonicity_scale: 1.10,
                hammer_hardness: 0.65,
                unison_detuning: 1.40,
                phantom_gain: 1.5,
                key_noise: 0.5,
                damper_noise: 1.3,
                pedal_noise: 1.2,
                mic_close: -12.0,
                mic_player: -2.0,
                mic_ambient: 3.0,
                lid_angle: 45.0,
                velocity_curve: -0.3,
            },
            FactoryPreset::ClassicalPureSolo => PresetValues {
                inharmonicity_scale: 1.00,
                hammer_hardness: 0.95,
                unison_detuning: 0.70,
                phantom_gain: 0.9,
                key_noise: 0.6,
                damper_noise: 0.7,
                pedal_noise: 0.7,
                mic_close: -4.0,
                mic_player: 2.0,
                mic_ambient: -3.0,
                lid_angle: 45.0,
                velocity_curve: 0.0,
            },
        }
    }
}
