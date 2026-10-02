//! Physical-modeling electric and acoustic bass instrument.
//!
//! The public engine is usable without a plugin host, which keeps the physical
//! model independently renderable and testable.  `nice_plugin` is only an
//! adapter around that engine.

pub mod acoustic;
pub mod core;
pub mod engine;
pub mod gui;
pub mod nice_plugin;
pub mod params;
pub mod presets;
pub mod string;

pub use acoustic::{AcousticBassBody, BassPickup, BassPickupType};
pub use engine::{
    BassEngine, BassEvent, BassFrame, BassMode, BassStringVoice,
    sanitize_floating_point_environment,
};
pub use params::BassStringParams;
pub use presets::factory_presets;
pub use string::{AlignedGrid, FdtdString, MAX_GRID_POINTS, PluckStyle};

nice_plug::nice_export_clap!(nice_plugin::PhysicsBass);
nice_plug::nice_export_vst3!(nice_plugin::PhysicsBass);
