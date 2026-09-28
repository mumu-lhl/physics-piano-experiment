//! First-principles physical modeling acoustic & electric guitar engine.

pub mod core;
pub mod engine;
pub mod gui;
pub mod nih_plugin;
pub mod params;
pub mod presets;

pub use core::*;
pub use engine::*;
pub use gui::*;
pub use nih_plugin::*;
pub use params::*;
pub use presets::*;
