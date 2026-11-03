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

use crate::preset_panel::attach_dropdown_keyboard_nav;
use crate::{Language, SaveFailureKind, save_failure_message, translate};

/// Builds the device row under the MIDI Learn row. Does nothing when this
/// process is a plugin rather than a standalone binary.
///
/// `save_status` is the editor-owned save-status line (shared with the
/// preset/favorite/tag save sites): device save and restart failures
/// show a short localized message there, and a successful save clears it.
pub fn device_settings_row(
    cx: &mut Context,
    presets_dir: PathBuf,
    lang: Language,
    save_status: Signal<String>,
) {
    if !device_settings::is_standalone() {
        return;
    }
    let available = device_settings::enumerate_devices();
    let initial = DeviceSettings::load_from_dir(&presets_dir).validate(&available);
    let settings = Signal::new(initial);
    let dir = presets_dir.clone();
    let status = save_status.clone();

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
            status.clone(),
            lang,
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
            status.clone(),
            lang,
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
            status.clone(),
            lang,
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
            status.clone(),
            lang,
        );
        Button::new(cx, |cx| {
            Label::new(
                cx,
                translate(lang, "gui.device-restart", "Restart to apply"),
            )
        })
        .class("btn-action")
        .width(Pixels(118.0))
        .on_press(move |_| {
            // The plugin has no API to end the wrapper's event loop, so the
            // process exits directly once the relaunched child is spawned;
            // the child waits before opening devices so this process
            // releases them first. A spawn failure must not exit (the
            // child never started); it is logged and shown on the
            // editor's save-status line, and the button stays inert.
            match device_settings::spawn_restart(Duration::from_millis(800)) {
                Ok(()) => std::process::exit(0),
                Err(err) => {
                    eprintln!("Failed to spawn restarted standalone: {err}");
                    status.set(save_failure_message(lang, SaveFailureKind::Restart));
                }
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
    save_status: Signal<String>,
    lang: Language,
) {
    let label = label.to_string();
    let none_label = none_label.to_string();
    HStack::new(cx, move |cx| {
        Label::new(cx, label).width(Pixels(66.0));
        let trigger_devices = devices.clone();
        let trigger_none = none_label.clone();
        // Keyboard navigation (same helper as the preset/language
        // dropdowns): the option list is the unset sentinel followed
        // by the enumerated devices, matching the popup rows below.
        // The selection logic is factored into `select` so the
        // mouse rows and the keyboard path run identical code.
        let nav_options: Vec<Option<String>> = std::iter::once(None)
            .chain(devices.iter().cloned().map(Some))
            .collect();
        let nav_settings = settings;
        let select = move |_cx: &mut EventContext, value: Option<String>| {
            let mut next = settings.get();
            set(&mut next, value);
            if let Err(err) = next.save_to_dir(&dir) {
                // The save failed (read-only/full presets dir):
                // say so and revert the signal to the last
                // saved sidecar instead of showing a device
                // that a restart would discard. The trigger
                // already maps unknown names to the unset
                // label, so the unvalidated reload is safe
                // to display.
                eprintln!("Failed to save device settings to {}: {err}", dir.display());
                save_status.set(save_failure_message(lang, SaveFailureKind::Device));
                settings.set(DeviceSettings::load_from_dir(&dir));
            } else {
                save_status.set(String::new());
                settings.set(next);
            }
        };
        let popup_select = select.clone();
        let popup_options = nav_options.clone();
        let popup_none = none_label.clone();
        let dropdown_open = Signal::new(false);
        let dropdown_highlighted: Signal<Option<usize>> = Signal::new(None);
        let device_dropdown = Dropdown::new(
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
            {
                let highlighted = dropdown_highlighted;
                move |cx| {
                    for (index, value) in popup_options.clone().into_iter().enumerate() {
                        let select = popup_select.clone();
                        let display = value.clone().unwrap_or_else(|| popup_none.clone());
                        Button::new(cx, move |cx| {
                            Label::new(cx, display)
                                .alignment(Alignment::Left)
                                .width(Stretch(1.0))
                                .text_overflow(TextOverflow::Ellipsis)
                        })
                        .toggle_class(
                            "preset-item-highlight",
                            highlighted.map(move |current| *current == Some(index)),
                        )
                        .on_hover(move |_| highlighted.set(Some(index)))
                        .on_press(move |cx| {
                            select(cx, value.clone());
                            cx.emit(PopupEvent::Close);
                        })
                        .width(Stretch(1.0))
                        .max_width(Pixels(320.0));
                    }
                }
            },
        )
        .width(Pixels(140.0));
        attach_dropdown_keyboard_nav(
            device_dropdown,
            dropdown_open,
            dropdown_highlighted,
            {
                let nav_options = nav_options.clone();
                move || {
                    let settings_value = nav_settings.get();
                    let current = get(&settings_value);
                    nav_options
                        .iter()
                        .position(|option| option == current)
                        .or(Some(0))
                }
            },
            {
                let nav_options = nav_options.clone();
                move || nav_options.len()
            },
            {
                let nav_options = nav_options.clone();
                let select = select.clone();
                move |cx: &mut EventContext, index: usize| {
                    if let Some(value) = nav_options.get(index) {
                        select(cx, value.clone());
                    }
                }
            },
        );
    })
    .horizontal_gap(Pixels(6.0));
}
