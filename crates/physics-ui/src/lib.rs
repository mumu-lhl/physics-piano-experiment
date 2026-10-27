//! Shared GUI support and theming for all physical instrument plugins.

mod curve_editor;
mod language;
pub mod midi_learn;
mod preset_browser;
mod preset_panel;
pub mod skia_compat;
mod spectrum;
mod theme;
mod vu_meter;
mod widgets;

pub use curve_editor::VelocityCurveEditorWidget;
pub use language::{Language, setup_vizia_fonts, translate, translate_format};
pub use midi_learn::{LastCc, LearnArm, apply_midi_cc, build_param_map, build_ptr_to_id_map};
pub use preset_browser::{
    PresetBrowserAction, PresetBrowserItem, PresetBrowserSignals, filter_browser_items,
    preset_browser, preset_browser_categories, preset_browser_items,
};
pub use preset_panel::{PresetPanelAction, PresetPanelLayout, PresetPanelSignals, preset_panel};
pub use spectrum::{
    SPECTRUM_BAR_COUNT, SPECTRUM_DB_FLOOR, SPECTRUM_FFT_LEN, SPECTRUM_FREQ_MAX, SPECTRUM_FREQ_MIN,
    SPECTRUM_HISTORY_LEN, SpectrumAnalyzer, SpectrumAnalyzerWidget, bar_center_frequencies,
    db_to_norm,
};
pub use theme::add_base_theme;
pub use vu_meter::StereoVuMeterWidget;
pub use widgets::{
    CancelParamGestureEvent, discrete_selector, map_param_history_event, parameter_knob,
    parameter_slider, preset_choices, preset_display_name, redraw_custom_view, set_param,
};
