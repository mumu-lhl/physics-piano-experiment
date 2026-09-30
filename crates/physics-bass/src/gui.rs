//! Usable bass editor: a real interactive fretboard plus bounded parameter racks.

mod fretboard_view;

use crate::nice_plugin::{GuiBassEvent, PhysicsBassParams};
use fretboard_view::BassFretboardWidget;
use nice_plug::prelude::Editor;
use physics_ui::{
    discrete_selector, parameter_slider, redraw_custom_view, set_param, setup_vizia_fonts,
};
use std::sync::Arc;
use vizia_plug::vizia::prelude::*;
use vizia_plug::widgets::{ParamButton, ParamButtonExt};
use vizia_plug::{ViziaTheming, create_vizia_editor};

pub const EDITOR_WIDTH: u32 = 1120;
pub const EDITOR_HEIGHT: u32 = 650;

#[derive(Debug, Clone, Copy)]
enum BassUiEvent {
    RefreshDisplay,
}

struct BassUiState {
    params: Arc<PhysicsBassParams>,
    mode: Signal<i32>,
    pluck_style: Signal<i32>,
}

impl Model for BassUiState {
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|event: &BassUiEvent, _| match event {
            BassUiEvent::RefreshDisplay => {
                self.mode.set(self.params.mode.value());
                self.pluck_style.set(self.params.pluck_style.value());
                redraw_custom_view(cx, "bass-fretboard-widget");
            }
        });
    }
}

pub fn create_vizia_bass_editor(
    params: Arc<PhysicsBassParams>,
    active_frets: Arc<[std::sync::atomic::AtomicU8; 5]>,
    string_energies: Arc<[std::sync::atomic::AtomicU32; 5]>,
    gui_tx: crossbeam_channel::Sender<GuiBassEvent>,
) -> Option<Box<dyn Editor>> {
    let editor_state = params.editor_state.clone();
    create_vizia_editor(editor_state, ViziaTheming::Custom, move |cx, _| {
        setup_vizia_fonts(cx);
        if let Err(error) = cx.add_stylesheet(include_style!("src/gui/theme.css")) {
            eprintln!("Failed to load bass Vizia stylesheet: {error:?}");
        }

        let mode = Signal::new(params.mode.value());
        let pluck_style = Signal::new(params.pluck_style.value());
        BassUiState {
            params: params.clone(),
            mode: mode.clone(),
            pluck_style: pluck_style.clone(),
        }
        .build(cx);

        let refresh_timer = cx.add_timer(std::time::Duration::from_millis(33), None, |cx, _| {
            cx.emit(BassUiEvent::RefreshDisplay);
        });
        cx.start_timer(refresh_timer);

        VStack::new(cx, |cx| {
            HStack::new(cx, |cx| {
                VStack::new(cx, |cx| {
                    Label::new(cx, "PHYSICS BASS").class("title");
                    Label::new(
                        cx,
                        "FDTD stiff string · nonlinear fret contact · magnetic aperture / body modes",
                    )
                    .class("subtitle");
                })
                .width(Stretch(1.0));
                Label::new(cx, "MIDI 28–67 · click the strings to audition")
                    .class("status");
            })
            .class("header")
            .width(Stretch(1.0));

            BassFretboardWidget::new(
                cx,
                params.clone(),
                active_frets.clone(),
                string_energies.clone(),
                gui_tx.clone(),
            )
            .class("instrument-view")
            .width(Stretch(1.0))
            .height(Pixels(300.0));

            Label::new(
                cx,
                "Drag across a string for legato-style re-triggering · pitch bend remains available from MIDI",
            )
            .class("hint-text");

            HStack::new(cx, |cx| {
                VStack::new(cx, |cx| {
                    Label::new(cx, "Instrument").class("rack-title");
                    discrete_selector(
                        cx,
                        "Model",
                        mode.clone(),
                        vec![
                            (0, "Electric / 电贝斯".to_string()),
                            (1, "Acoustic / 木贝斯".to_string()),
                        ],
                        94.0,
                        {
                            let params = params.clone();
                            move |cx, value| set_param(cx, &params.mode, value)
                        },
                    );
                    discrete_selector(
                        cx,
                        "Exciter",
                        pluck_style.clone(),
                        vec![
                            (0, "Finger".to_string()),
                            (1, "Pick".to_string()),
                            (2, "Slap".to_string()),
                        ],
                        94.0,
                        {
                            let params = params.clone();
                            move |cx, value| set_param(cx, &params.pluck_style, value)
                        },
                    );
                    HStack::new(cx, |cx| {
                        Label::new(cx, "Tuning").class("param-label");
                        ParamButton::new(cx, &params.five_string).with_label("Five-string B0");
                    })
                    .height(Pixels(26.0))
                    .horizontal_gap(Pixels(6.0));
                })
                .class("rack-box")
                .width(Stretch(1.0));

                VStack::new(cx, |cx| {
                    Label::new(cx, "String / pickup").class("rack-title");
                    parameter_slider(cx, "Pluck position", &params.pluck_position, 108.0);
                    parameter_slider(cx, "Pickup position", &params.pickup_position, 108.0);
                    parameter_slider(cx, "Tone", &params.tone, 108.0);
                })
                .class("rack-box")
                .width(Stretch(1.0));

                VStack::new(cx, |cx| {
                    Label::new(cx, "Contact / body").class("rack-title");
                    parameter_slider(cx, "Fret buzz", &params.fret_buzz, 108.0);
                    parameter_slider(cx, "Body mix", &params.body_mix, 108.0);
                    Label::new(cx, "Slap impact uses the same bounded contact state as the FDTD string")
                        .class("small-note");
                })
                .class("rack-box")
                .width(Stretch(1.0));

                VStack::new(cx, |cx| {
                    Label::new(cx, "Output").class("rack-title");
                    parameter_slider(cx, "Master gain", &params.master_gain, 108.0);
                    Label::new(cx, "Electric: finite-gap pickup · Acoustic: A0 / B1 / bridge hill")
                        .class("small-note");
                })
                .class("rack-box")
                .width(Stretch(1.0));
            })
            .class("rack-row")
            .width(Stretch(1.0))
            .height(Pixels(145.0))
            .horizontal_gap(Pixels(8.0));
        })
        .class("root")
        .width(Stretch(1.0))
        .height(Stretch(1.0));
    })
}
