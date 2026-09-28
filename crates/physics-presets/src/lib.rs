//! Universal Preset and Undo/Redo System for Physics Piano and Physics Guitar.

pub mod manager;
pub mod preset;
pub mod undo;

pub use manager::PresetManager;
pub use preset::Preset;
pub use undo::{ParamTransition, UndoAction, UndoManager};

/// Load Piano Factory Presets from embedded JSON resources
pub fn piano_factory_presets() -> Vec<Preset> {
    const PIANO_PRESETS_JSON: &[&str] = &[
        include_str!("../../../assets/presets/piano/steinway_concert_d.json"),
        include_str!("../../../assets/presets/piano/bright_pop_grand.json"),
        include_str!("../../../assets/presets/piano/warm_intimate_chamber.json"),
        include_str!("../../../assets/presets/piano/vintage_upright.json"),
        include_str!("../../../assets/presets/piano/cinematic_dream.json"),
        include_str!("../../../assets/presets/piano/classical_pure_solo.json"),
    ];

    PIANO_PRESETS_JSON
        .iter()
        .filter_map(|json| Preset::from_json(json).ok())
        .collect()
}

/// Load Guitar Factory Presets from embedded JSON resources
pub fn guitar_factory_presets() -> Vec<Preset> {
    const GUITAR_PRESETS_JSON: &[&str] = &[
        include_str!("../../../assets/presets/guitar/strat_clean_chime.json"),
        include_str!("../../../assets/presets/guitar/dreadnought_acoustic_fingerstyle.json"),
        include_str!("../../../assets/presets/guitar/blues_overdrive_crunch.json"),
        include_str!("../../../assets/presets/guitar/warm_jazz_archtop.json"),
        include_str!("../../../assets/presets/guitar/heavy_palm_mute_metal.json"),
        include_str!("../../../assets/presets/guitar/ambient_dreamy_pluck.json"),
    ];

    GUITAR_PRESETS_JSON
        .iter()
        .filter_map(|json| Preset::from_json(json).ok())
        .collect()
}
