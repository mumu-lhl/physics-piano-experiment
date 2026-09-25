//! GUI modules for interactive piano visualization and controls.

pub mod keyboard;
pub mod controls;
pub mod i18n;

pub use keyboard::PianoKeyboardWidget;
pub use controls::{render_vu_meter, render_lissajous_scope};
pub use i18n::{Language, I18n, setup_cjk_fonts};
