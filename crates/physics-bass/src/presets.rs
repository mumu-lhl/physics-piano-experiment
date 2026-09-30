//! Factory voicings for the bass plugin.

use physics_presets::Preset;

/// Built-in bass sounds loaded from embedded, versioned JSON resources.
/// Parameter keys are the stable IDs declared by `PhysicsBassParams`.
pub fn factory_presets() -> Vec<Preset> {
    physics_presets::bass_factory_presets()
}
