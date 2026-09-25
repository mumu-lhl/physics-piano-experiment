pub mod core;
pub mod dsp;
pub mod params;
pub mod engine;
pub mod clap_plugin;
pub mod gui;
pub mod nih_plugin;

pub use engine::PianoEngine;
pub use clap_plugin::raw_clap_entry;
pub use nih_plugin::PhysicsPiano;

nih_plug::nih_export_clap!(nih_plugin::PhysicsPiano);
nih_plug::nih_export_vst3!(nih_plugin::PhysicsPiano);
