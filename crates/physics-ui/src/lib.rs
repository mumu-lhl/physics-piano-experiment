//! Shared GUI support for the piano and guitar instrument plugins.

mod language;
mod preset_panel;
pub mod skia_compat;
mod widgets;

pub use language::{Language, setup_vizia_fonts};
pub use preset_panel::{PresetPanelAction, PresetPanelLayout, PresetPanelSignals, preset_panel};
pub use widgets::{
    discrete_selector, parameter_slider, preset_choices, redraw_custom_view, set_param, ui_text,
};
