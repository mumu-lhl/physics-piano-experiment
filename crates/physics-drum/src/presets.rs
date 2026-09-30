//! Factory voicings for the physical drum kit.

use physics_presets::Preset;

/// Built-in physical drum-kit voicings loaded from embedded JSON resources.
pub fn factory_presets() -> Vec<Preset> {
    physics_presets::drum_factory_presets()
}
