use vizia_plug::vizia::prelude::Context;

/// Install the shared Piano-based dark theme before an instrument's local overrides.
pub fn add_base_theme(cx: &mut Context) -> std::io::Result<()> {
    cx.add_stylesheet(include_str!("theme.css"))
}
