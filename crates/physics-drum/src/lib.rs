//! Hybrid physical-modeling drum kit.

pub mod contact;
pub mod core;
pub mod engine;
pub mod gui;
pub mod membrane;
pub mod nice_plugin;
pub mod presets;
pub mod voices;

pub use contact::HuntCrossleyExciter;
pub use engine::{
    DrumEngine, DrumEvent, DrumVoice, sanitize_floating_point_environment, voice_for_note,
};
pub use membrane::{HEAD_MODE_COUNT, MembraneHead};
pub use presets::factory_presets;
pub use voices::{CymbalVoice, DoubleHeadVoice, KickVoice, SnareVoice, TomVoice};

nice_plug::nice_export_clap!(nice_plugin::PhysicsDrum);
nice_plug::nice_export_vst3!(nice_plugin::PhysicsDrum);
