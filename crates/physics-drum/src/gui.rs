//! Compact Vizia control surface for the physical drum-kit controls.

use crate::nice_plugin::PhysicsDrumParams;
use nice_plug::prelude::Editor;
use physics_ui::{parameter_slider, setup_vizia_fonts};
use std::sync::Arc;
use vizia_plug::vizia::prelude::*;
use vizia_plug::{ViziaTheming, create_vizia_editor};

pub const EDITOR_WIDTH: u32 = 660;
pub const EDITOR_HEIGHT: u32 = 260;

pub fn create_vizia_drum_editor(params: Arc<PhysicsDrumParams>) -> Option<Box<dyn Editor>> {
    let editor_state = params.editor_state.clone();
    create_vizia_editor(editor_state, ViziaTheming::Custom, move |cx, _| {
        setup_vizia_fonts(cx);
        VStack::new(cx, |cx| {
            Label::new(cx, "PHYSICS DRUM").class("title");
            Label::new(
                cx,
                "Bessel membrane modes · Hunt-Crossley impact · snare chatter · cymbal cascade",
            )
            .class("subtitle");
            parameter_slider(cx, "Snare tightness", &params.snare_tightness, 130.0);
            parameter_slider(cx, "Snare decay", &params.snare_decay, 130.0);
            parameter_slider(cx, "Hi-hat open", &params.hihat_open, 130.0);
            parameter_slider(cx, "Cymbal decay", &params.cymbal_decay, 130.0);
            parameter_slider(cx, "Master gain", &params.master_gain, 130.0);
        })
        .class("root")
        .width(Stretch(1.0))
        .height(Stretch(1.0));
    })
}
