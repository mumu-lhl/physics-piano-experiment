//! First-principles physical modeling acoustic & electric guitar engine.

pub mod params;
pub mod core;
pub mod engine;
pub mod gui;
pub mod nih_plugin;

pub use params::*;
pub use core::*;
pub use engine::*;
pub use gui::*;
pub use nih_plugin::*;
