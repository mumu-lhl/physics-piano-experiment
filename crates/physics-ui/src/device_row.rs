//! The standalone-only audio/MIDI device row (issue #3 item 10).
//!
//! Four dropdowns (audio in/out, MIDI in/out) bound to the saved
//! [`DeviceSettings`], plus a restart button. A selection saves immediately
//! to the per-instrument sidecar; it takes effect when the standalone is
//! relaunched, because the wrapper reads the device names once at startup.
//! The row is hidden in plugin builds, where the host owns the devices
//! (see [`device_settings::is_standalone`]).

use std::path::PathBuf;
use std::time::Duration;

use physics_presets::device_settings::{self, DeviceSettings};
use vizia_plug::vizia::prelude::*;

use crate::{Language, translate};

/// Builds the device row under the MIDI Learn row. Does nothing when this
/// process is a plugin rather than a standalone binary.
pub fn device_settings_row(cx: &mut Context, presets_dir: PathBuf, lang: Language) {
    if !device_settings::is_standalone() {
        return;
    }
    let available = device_settings::enumerate_devices();
    let initial = DeviceSettings::load_from_dir(&presets_dir).validate(&available);
    let settings = Signal::new(initial);
    let dir = presets_dir.clone();

    HStack::new(cx, |cx| {
        device_dropdown(
            cx,
            translate(lang, "gui.device-audio-in", "Audio In"),
            settings,
            |current| &current.input,
            |next, value| next.input = value,
            available.inputs,
            translate(lang, "gui.device-none", "None"),
            dir.clone(),
        );
        device_dropdown(
            cx,
            translate(lang, "gui.device-audio-out", "Audio Out"),
            settings,
            |current| &current.output,
            |next, value| next.output = value,
            available.outputs,
            translate(lang, "gui.device-default-output", "System default"),
            dir.clone(),
        );
        device_dropdown(
            cx,
            translate(lang, "gui.device-midi-in", "MIDI In"),
            settings,
            |current| &current.midi_input,
            |next, value| next.midi_input = value,
            available.midi_inputs,
            translate(lang, "gui.device-none", "None"),
            dir.clone(),
        );
        device_dropdown(
            cx,
            translate(lang, "gui.device-midi-out", "MIDI Out"),
            settings,
            |current| &current.midi_output,
            |next, value| next.midi_output = value,
            available.midi_outputs,
            translate(lang, "gui.device-none", "None"),
            dir.clone(),
        );
        Button::new(cx, |cx| {
            Label::new(
                cx,
                translate(lang, "gui.device-restart", "Restart to apply"),
            )
        })
        .class("btn-action")
        .width(Pixels(118.0))
        .on_press(|_| {
            // The plugin has no API to end the wrapper's event loop, so the
            // process exits directly once the relaunched child is spawned;
            // the child waits before opening devices so this process
            // releases them first.
            if device_settings::spawn_restart(Duration::from_millis(800)).is_ok() {
                std::process::exit(0);
            }
        });
    })
    .height(Pixels(23.0))
    .horizontal_gap(Pixels(8.0));
}

/// One labeled device dropdown. The first option is the unset sentinel
/// (`none_label`); the rest are the enumerated device names. Selecting an
/// option updates the shared settings signal and saves the sidecar.
#[allow(clippy::too_many_arguments)]
fn device_dropdown(
    cx: &mut Context,
    label: &str,
    settings: Signal<DeviceSettings>,
    get: fn(&DeviceSettings) -> &Option<String>,
    set: fn(&mut DeviceSettings, Option<String>),
    devices: Vec<String>,
    none_label: &str,
    dir: PathBuf,
) {
    let label = label.to_string();
    let none_label = none_label.to_string();
    HStack::new(cx, move |cx| {
        Label::new(cx, label).width(Pixels(66.0));
        let trigger_devices = devices.clone();
        let trigger_none = none_label.clone();
        Dropdown::new(
            cx,
            move |cx| {
                let display_devices = trigger_devices.clone();
                let display_none = trigger_none.clone();
                Button::new(cx, move |cx| {
                    HStack::new(cx, move |cx| {
                        Label::new(
                            cx,
                            settings.map(move |current| {
                                get(current)
                                    .clone()
                                    .filter(|name| display_devices.contains(name))
                                    .unwrap_or_else(|| display_none.clone())
                            }),
                        )
                        .width(Stretch(1.0))
                        .text_overflow(TextOverflow::Ellipsis);
                        Label::new(cx, "▾").hoverable(false);
                    })
                    .width(Stretch(1.0))
                })
                .on_press(|cx| cx.emit(PopupEvent::Switch))
                .width(Stretch(1.0))
                .height(Pixels(22.0));
            },
            move |cx| {
                let popup_devices = devices.clone();
                let popup_none = none_label.clone();
                let options = std::iter::once(None)
                    .chain(popup_devices.iter().cloned().map(Some))
                    .collect::<Vec<_>>();
                for value in options {
                    let dir = dir.clone();
                    let display = value.clone().unwrap_or_else(|| popup_none.clone());
                    Button::new(cx, move |cx| {
                        Label::new(cx, display).alignment(Alignment::Left)
                    })
                    .on_press(move |cx| {
                        let mut next = settings.get();
                        set(&mut next, value.clone());
                        settings.set(next.clone());
                        let _ = next.save_to_dir(&dir);
                        cx.emit(PopupEvent::Close);
                    })
                    .width(Stretch(1.0));
                }
            },
        )
        .width(Pixels(140.0));
    })
    .horizontal_gap(Pixels(6.0));
}
