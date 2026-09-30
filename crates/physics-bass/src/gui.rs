//! Compact Vizia control surface for the bass parameters.

use crate::nice_plugin::PhysicsBassParams;
use nice_plug::prelude::Editor;
use physics_ui::{parameter_slider, setup_vizia_fonts};
use std::sync::Arc;
use vizia_plug::vizia::prelude::*;
use vizia_plug::widgets::{ParamButton, ParamButtonExt};
use vizia_plug::{ViziaTheming, create_vizia_editor};

pub const EDITOR_WIDTH: u32 = 760;
pub const EDITOR_HEIGHT: u32 = 390;

pub fn create_vizia_bass_editor(params: Arc<PhysicsBassParams>) -> Option<Box<dyn Editor>> {
    let editor_state = params.editor_state.clone();
    create_vizia_editor(editor_state, ViziaTheming::Custom, move |cx, _| {
        setup_vizia_fonts(cx);
        VStack::new(cx, |cx| {
            Label::new(cx, "PHYSICS BASS").class("title");
            Label::new(
                cx,
                "CFL-bounded stiff string · finite pickup aperture · body modes",
            )
            .class("subtitle");
            HStack::new(cx, |cx| {
                VStack::new(cx, |cx| {
                    Label::new(cx, "Instrument").class("section");
                    parameter_slider(cx, "Mode (0/1)", &params.mode, 120.0);
                    parameter_slider(cx, "Pluck (0/1/2)", &params.pluck_style, 120.0);
                    ParamButton::new(cx, &params.five_string).with_label("Five-string");
                })
                .width(Stretch(1.0));
                VStack::new(cx, |cx| {
                    Label::new(cx, "Acoustic / pickup").class("section");
                    parameter_slider(cx, "Pickup position", &params.pickup_position, 120.0);
                    parameter_slider(cx, "Pluck position", &params.pluck_position, 120.0);
                    parameter_slider(cx, "Tone", &params.tone, 120.0);
                    parameter_slider(cx, "Body mix", &params.body_mix, 120.0);
                })
                .width(Stretch(1.0));
                VStack::new(cx, |cx| {
                    Label::new(cx, "Articulation / output").class("section");
                    parameter_slider(cx, "Fret buzz", &params.fret_buzz, 120.0);
                    parameter_slider(cx, "Master gain", &params.master_gain, 120.0);
                })
                .width(Stretch(1.0));
            })
            .width(Stretch(1.0))
            .horizontal_gap(Pixels(12.0));
        })
        .class("root")
        .width(Stretch(1.0))
        .height(Stretch(1.0));
    })
}
