//! GUI modules for interactive piano visualization and controls.

pub mod editor;
pub mod i18n;
pub mod keyboard;
pub mod lid;
pub mod mics;
pub mod scope;

pub use editor::{create_vizia_piano_editor, default_vizia_state};
pub use i18n::{I18n, Language};
pub use keyboard::PianoKeyboardWidget;
pub use lid::PianoLidWidget;
pub use mics::MicStageWidget;
pub use scope::{LissajousScopeWidget, StereoVuMeterWidget};
