//! Shared GUI support and theming for all physical instrument plugins.

mod language;
mod preset_panel;
pub mod skia_compat;
mod theme;
mod widgets;

pub use language::{Language, setup_vizia_fonts, translate, translate_format};
pub use preset_panel::{PresetPanelAction, PresetPanelLayout, PresetPanelSignals, preset_panel};
pub use theme::add_base_theme;
pub use widgets::{
    CancelParamGestureEvent, discrete_selector, map_param_history_event, parameter_slider,
    preset_choices, preset_display_name, redraw_custom_view, set_param,
};
