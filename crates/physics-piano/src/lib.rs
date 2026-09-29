pub mod clap_plugin;
pub mod core;
pub mod dsp;
pub mod engine;
pub mod gui;
pub mod nice_plugin;
pub mod params;
pub mod presets;

pub use clap_plugin::raw_clap_entry;
pub use engine::PianoEngine;
pub use nice_plugin::PhysicsPiano;

nice_plug::nice_export_clap!(nice_plugin::PhysicsPiano);
nice_plug::nice_export_vst3!(nice_plugin::PhysicsPiano);
