use std::sync::Arc;

use vizia_plug::vizia::prelude::*;
use vizia_plug::ViziaState;

use crate::{translate, Language};

/// Install the shared Piano-based dark theme before an instrument's local overrides.
pub fn add_base_theme(cx: &mut Context) -> std::io::Result<()> {
    cx.add_stylesheet(include_str!("theme.css"))?;
    cx.add_stylesheet(include_str!("light.css"))
}

/// Build the header button that toggles the light theme. The label shows the
/// currently active theme and follows the UI language; pressing the button
/// flips the persisted choice in `ViziaState`, which the root container's
/// `theme-light` class binding picks up immediately.
pub fn theme_toggle_button(cx: &mut Context, state: Arc<ViziaState>, lang: Language) {
    let theme_signal = state.theme_signal();
    Binding::new(cx, theme_signal, move |cx| {
        let light = theme_signal.get();
        let state = state.clone();
        Button::new(cx, |cx| {
            Label::new(
                cx,
                translate(
                    lang,
                    if light {
                        "gui.theme-light"
                    } else {
                        "gui.theme-dark"
                    },
                    if light { "Light" } else { "Dark" },
                ),
            )
        })
        .class("btn-action")
        .on_press(move |_| {
            state.set_light_theme(!state.light_theme());
        })
        .width(Pixels(56.0));
    });
}
