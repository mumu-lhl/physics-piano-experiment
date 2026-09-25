//! GUI modules for interactive piano visualization and controls.

pub mod keyboard;
pub mod controls;

pub use keyboard::PianoKeyboardWidget;
pub use controls::{render_vu_meter, render_lissajous_scope};
