//! Usable drum editor with an interactive kit layout and animated voice pads.

mod pad_view;

use crate::nice_plugin::{GuiDrumEvent, PhysicsDrumParams};
use nice_plug::prelude::Editor;
use pad_view::DrumPadWidget;
use physics_ui::{parameter_slider, redraw_custom_view, setup_vizia_fonts};
use std::sync::Arc;
use vizia_plug::vizia::prelude::*;
use vizia_plug::{ViziaTheming, create_vizia_editor};

pub const EDITOR_WIDTH: u32 = 900;
pub const EDITOR_HEIGHT: u32 = 575;

#[derive(Debug, Clone, Copy)]
enum DrumUiEvent {
    RefreshDisplay,
}

struct DrumUiState;

impl Model for DrumUiState {
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|event: &DrumUiEvent, _| match event {
            DrumUiEvent::RefreshDisplay => redraw_custom_view(cx, "drum-pad-widget"),
        });
    }
}

pub fn create_vizia_drum_editor(
    params: Arc<PhysicsDrumParams>,
    voice_energies: Arc<[std::sync::atomic::AtomicU32; 10]>,
    gui_tx: crossbeam_channel::Sender<GuiDrumEvent>,
) -> Option<Box<dyn Editor>> {
    let editor_state = params.editor_state.clone();
    create_vizia_editor(editor_state, ViziaTheming::Custom, move |cx, _| {
        setup_vizia_fonts(cx);
        if let Err(error) = cx.add_stylesheet(include_style!("src/gui/theme.css")) {
            eprintln!("Failed to load drum Vizia stylesheet: {error:?}");
        }

        DrumUiState.build(cx);
        let refresh_timer = cx.add_timer(std::time::Duration::from_millis(33), None, |cx, _| {
            cx.emit(DrumUiEvent::RefreshDisplay);
        });
        cx.start_timer(refresh_timer);

        VStack::new(cx, |cx| {
            HStack::new(cx, |cx| {
                VStack::new(cx, |cx| {
                    Label::new(cx, "PHYSICS DRUM").class("title");
                    Label::new(
                        cx,
                        "Bessel membrane modes · Hunt-Crossley impact · coupled heads · nonlinear cymbals",
                    )
                    .class("subtitle");
                })
                .width(Stretch(1.0));
                Label::new(cx, "GM 35–51 · click pads to audition")
                    .class("status");
            })
            .class("header")
            .width(Stretch(1.0));

            DrumPadWidget::new(cx, voice_energies.clone(), gui_tx.clone())
                .class("instrument-view")
                .width(Stretch(1.0))
                .height(Pixels(350.0));

            HStack::new(cx, |cx| {
                VStack::new(cx, |cx| {
                    Label::new(cx, "Snare / membrane").class("rack-title");
                    parameter_slider(cx, "Snare tightness", &params.snare_tightness, 112.0);
                    parameter_slider(cx, "Snare decay", &params.snare_decay, 112.0);
                    Label::new(cx, "Bottom head drives 20 bounded wire contacts")
                        .class("small-note");
                })
                .class("rack-box")
                .width(Stretch(1.0));
                VStack::new(cx, |cx| {
                    Label::new(cx, "Cymbal / output").class("rack-title");
                    parameter_slider(cx, "Hi-hat open", &params.hihat_open, 112.0);
                    parameter_slider(cx, "Cymbal decay", &params.cymbal_decay, 112.0);
                    parameter_slider(cx, "Master gain", &params.master_gain, 112.0);
                })
                .class("rack-box")
                .width(Stretch(1.0));
            })
            .class("rack-row")
            .width(Stretch(1.0))
            .height(Pixels(100.0))
            .horizontal_gap(Pixels(8.0));

            Label::new(
                cx,
                "Closed hat chokes the open hat · CC4 controls pedal opening · MIDI note-off never truncates a physical tail",
            )
            .class("hint-text");
        })
        .class("root")
        .width(Stretch(1.0))
        .height(Stretch(1.0));
    })
}
