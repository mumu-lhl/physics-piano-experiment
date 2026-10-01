//! GUI modules for interactive piano visualization and controls.

pub mod editor;
pub mod keyboard;
pub mod lid;
pub mod mics;
pub mod scope;
mod skia_compat;

pub use editor::{create_vizia_piano_editor, default_vizia_state};
pub use keyboard::PianoKeyboardWidget;
pub use lid::PianoLidWidget;
pub use mics::MicStageWidget;
pub use physics_ui::Language;
pub use scope::{LissajousScopeWidget, StereoVuMeterWidget};
