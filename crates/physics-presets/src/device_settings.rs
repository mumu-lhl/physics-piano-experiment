//! Standalone audio/MIDI device selection: enumeration, persistence, and
//! startup injection.
//!
//! The device names are the ones the standalone wrapper matches exactly
//! (`--input-device`, `--output-device`, `--midi-input`, `--midi-output`).
//! A selection takes effect on the next standalone launch: the GUI saves it
//! to a per-instrument sidecar next to `favorites.json`, and the standalone
//! binary injects the matching flags at startup. The wrapper exits with an
//! error on an unknown device name, so stale names (unplugged devices) are
//! dropped by [`DeviceSettings::validate`] before they ever reach it.
//!
//! The selectors are standalone-only: each standalone binary calls
//! [`mark_standalone`] from `main`, and the plugin builds (CLAP/VST) never
//! do, so the in-app selectors stay hidden there where the host owns the
//! devices.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use serde::{Deserialize, Serialize};

use crate::manager::{backup_corrupt_sidecar, write_sidecar_atomic};

/// File name of the per-instrument device sidecar, next to `favorites.json`
/// and `preset_tags.json` in the instrument's presets directory.
pub const DEVICE_SETTINGS_FILE: &str = "device_settings.json";

/// Env var the restart helper sets on the relaunched child so it waits
/// before opening devices, giving the exiting parent time to release them.
pub const RESTART_DELAY_ENV_VAR: &str = "PHYSICS_INSTRUMENTS_RESTART_DELAY_MS";

/// Set by each standalone binary's `main`. A plain static, not an env var:
/// the plugin path never runs `main`, so the flag is false there without any
/// process-global environment mutation.
static IS_STANDALONE: AtomicBool = AtomicBool::new(false);

/// Record that this process is a standalone binary. Called once from `main`
/// before the wrapper starts.
pub fn mark_standalone() {
    IS_STANDALONE.store(true, Ordering::Release);
}

/// Whether this process is a standalone binary (as opposed to a plugin
/// running inside a host).
pub fn is_standalone() -> bool {
    IS_STANDALONE.load(Ordering::Acquire)
}

/// Test-only reset for the process-global standalone flag, so tests
/// that exercise [`mark_standalone`] cannot poison the flag assertion
/// of a test running later in the same process.
#[cfg(test)]
fn reset_standalone_flag() {
    IS_STANDALONE.store(false, Ordering::Release);
}

/// Saved device selection. `None` means "wrapper default": no input device
/// for `input`, the system default output for `output`, and no MIDI device
/// for the MIDI fields.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub midi_input: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub midi_output: Option<String>,
}

/// Devices currently visible to the host APIs. Enumeration never fails: a
/// missing backend yields an empty list, and the selectors fall back to the
/// default/none options.
#[derive(Debug, Clone, Default)]
pub struct DeviceList {
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    pub midi_inputs: Vec<String>,
    pub midi_outputs: Vec<String>,
}

/// The long flag for each device field, in the same order as the fields.
const DEVICE_FLAGS: [(&str, fn(&DeviceSettings) -> &Option<String>); 4] = [
    ("--input-device", |s| &s.input),
    ("--output-device", |s| &s.output),
    ("--midi-input", |s| &s.midi_input),
    ("--midi-output", |s| &s.midi_output),
];

fn dedup(names: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    names
        .into_iter()
        .filter(|name| !name.is_empty() && seen.insert(name.clone()))
        .collect()
}

/// One optional device-name field from a parsed sidecar object: a
/// string is kept, a missing field or JSON `null` means unset, and any
/// other type is dropped with a diagnostic naming the file and field.
fn string_field(
    object: &serde_json::Map<String, serde_json::Value>,
    path: &Path,
    field: &str,
) -> Option<String> {
    match object.get(field) {
        None | Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::String(value)) => Some(value.clone()),
        Some(other) => {
            eprintln!(
                "physics-presets: dropping wrong-typed device setting {field}={other} in {}",
                path.display()
            );
            None
        }
    }
}

/// Enumerate audio input/output devices (cpal) and MIDI ports (midir).
/// Every backend is best-effort: an unavailable backend contributes an
/// empty list instead of an error.
pub fn enumerate_devices() -> DeviceList {
    use cpal::traits::{DeviceTrait, HostTrait};

    let host = cpal::default_host();
    let inputs = host
        .input_devices()
        .map(|devices| {
            devices
                .filter_map(|device| {
                    device
                        .description()
                        .ok()
                        .map(|description| description.name().to_string())
                })
                .collect()
        })
        .unwrap_or_default();
    let outputs = host
        .output_devices()
        .map(|devices| {
            devices
                .filter_map(|device| {
                    device
                        .description()
                        .ok()
                        .map(|description| description.name().to_string())
                })
                .collect()
        })
        .unwrap_or_default();
    let midi_inputs = midir::MidiInput::new("physics-instruments")
        .map(|backend| {
            backend
                .ports()
                .iter()
                .filter_map(|port| backend.port_name(port).ok())
                .collect()
        })
        .unwrap_or_default();
    let midi_outputs = midir::MidiOutput::new("physics-instruments")
        .map(|backend| {
            backend
                .ports()
                .iter()
                .filter_map(|port| backend.port_name(port).ok())
                .collect()
        })
        .unwrap_or_default();
    DeviceList {
        inputs: dedup(inputs),
        outputs: dedup(outputs),
        midi_inputs: dedup(midi_inputs),
        midi_outputs: dedup(midi_outputs),
    }
}

impl DeviceSettings {
    /// Path of the sidecar for one instrument, in its presets directory.
    pub fn file_path(instrument: &str) -> PathBuf {
        crate::manager::default_user_presets_dir(instrument).join(DEVICE_SETTINGS_FILE)
    }

    /// Load the saved selection from a directory, leniently: fields
    /// with the wrong type fall back to their default (unset) with a
    /// diagnostic, while valid fields are kept. A missing file means
    /// "all defaults"; a whole file that is not a JSON object is backed
    /// up as `<file>.corrupt` and loads defaults.
    pub fn load_from_dir(dir: &Path) -> DeviceSettings {
        let path = dir.join(DEVICE_SETTINGS_FILE);
        let raw = match std::fs::read(&path) {
            Ok(raw) => raw,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return DeviceSettings::default();
            }
            Err(err) => {
                eprintln!(
                    "physics-presets: failed to read device settings {}: {err}",
                    path.display()
                );
                return DeviceSettings::default();
            }
        };
        let Ok(content) = std::str::from_utf8(&raw) else {
            backup_corrupt_sidecar(&path, &raw);
            return DeviceSettings::default();
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(content) else {
            backup_corrupt_sidecar(&path, &raw);
            return DeviceSettings::default();
        };
        let Some(object) = value.as_object() else {
            backup_corrupt_sidecar(&path, &raw);
            return DeviceSettings::default();
        };
        DeviceSettings {
            input: string_field(object, &path, "input"),
            output: string_field(object, &path, "output"),
            midi_input: string_field(object, &path, "midi_input"),
            midi_output: string_field(object, &path, "midi_output"),
        }
    }

    /// Save the selection to a directory, creating it as needed.
    pub fn save_to_dir(&self, dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let json = serde_json::to_string_pretty(self)
            .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;
        write_sidecar_atomic(&dir.join(DEVICE_SETTINGS_FILE), json.as_bytes())
    }

    /// Load the saved selection for one instrument.
    pub fn load(instrument: &str) -> DeviceSettings {
        Self::load_from_dir(&crate::manager::default_user_presets_dir(instrument))
    }

    /// Save the selection for one instrument.
    pub fn save(&self, instrument: &str) -> std::io::Result<()> {
        self.save_to_dir(&crate::manager::default_user_presets_dir(instrument))
    }

    /// Drop selections naming devices that are not currently available.
    /// Unset fields stay unset.
    pub fn validate(&self, available: &DeviceList) -> DeviceSettings {
        let keep = |name: &Option<String>, list: &[String]| {
            name.as_ref()
                .filter(|name| list.iter().any(|candidate| candidate == *name))
                .cloned()
        };
        DeviceSettings {
            input: keep(&self.input, &available.inputs),
            output: keep(&self.output, &available.outputs),
            midi_input: keep(&self.midi_input, &available.midi_inputs),
            midi_output: keep(&self.midi_output, &available.midi_outputs),
        }
    }

    /// The wrapper flags for the set fields, as `--flag value` pairs.
    pub fn to_args(&self) -> Vec<String> {
        let mut args = Vec::new();
        for (flag, field) in DEVICE_FLAGS {
            if let Some(name) = field(self) {
                args.push(flag.to_string());
                args.push(name.clone());
            }
        }
        args
    }
}

/// Whether `args` already selects the device for one long flag, in either
/// `--flag value` or `--flag=value` form.
pub fn has_device_flag(args: &[String], flag: &str) -> bool {
    args.iter()
        .any(|arg| arg == flag || arg.starts_with(&format!("{flag}=")))
}

/// Load the saved selection, drop stale names, and append the wrapper flags
/// for the surviving selections — one field at a time, so an explicit flag
/// for one device never blocks the saved selection for another. Skipped
/// entirely when the user asked for help.
pub fn inject_saved_devices(args: &mut Vec<String>, instrument: &str) {
    let dir = crate::manager::default_user_presets_dir(instrument);
    let available = enumerate_devices();
    inject_saved_devices_in_dir(args, &dir, &available);
}

/// [`inject_saved_devices`] against an explicit directory and device
/// list, so tests can exercise startup injection with a temp dir and a
/// stubbed [`DeviceList`] instead of the real home directory and real
/// hardware enumeration.
pub fn inject_saved_devices_in_dir(args: &mut Vec<String>, dir: &Path, available: &DeviceList) {
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        return;
    }
    let settings = DeviceSettings::load_from_dir(dir).validate(available);
    for (flag, field) in DEVICE_FLAGS {
        if let Some(name) = field(&settings) {
            if !has_device_flag(args, flag) {
                args.push(flag.to_string());
                args.push(name.clone());
            }
        }
    }
}

/// Remove device selections from an argv tail (both `--flag value` and
/// `--flag=value` forms), so a relaunch falls back to the saved sidecar
/// values instead of the flags the current process started with.
pub fn strip_device_flags(args: &[String]) -> Vec<String> {
    let mut kept = Vec::new();
    let mut skip_next = false;
    for arg in args {
        if skip_next {
            skip_next = false;
            continue;
        }
        if DEVICE_FLAGS.iter().any(|(flag, _)| arg == flag) {
            skip_next = true;
            continue;
        }
        if DEVICE_FLAGS
            .iter()
            .any(|(flag, _)| arg.starts_with(&format!("{flag}=")))
        {
            continue;
        }
        kept.push(arg.clone());
    }
    kept
}

/// Relaunch the current executable with the saved device selection applied.
/// The child reads the sidecar at startup (see [`inject_saved_devices`]),
/// and waits `delay` before opening devices so this process can release
/// them first. Returns after spawning; the caller exits the old process.
pub fn spawn_restart(delay: std::time::Duration) -> std::io::Result<()> {
    let exe = std::env::current_exe()?;
    let args = strip_device_flags(&std::env::args().skip(1).collect::<Vec<_>>());
    std::process::Command::new(exe)
        .args(args)
        .env(RESTART_DELAY_ENV_VAR, delay.as_millis().to_string())
        .spawn()?;
    Ok(())
}

/// How long a relaunched child waits before opening devices, parsed from
/// [`RESTART_DELAY_ENV_VAR`]. Zero (or an absent/unparsable value) means
/// "no wait".
pub fn restart_delay() -> std::time::Duration {
    std::env::var(RESTART_DELAY_ENV_VAR)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(std::time::Duration::from_millis)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn temp_dir(tag: &str) -> PathBuf {
        // Per-test tag + pid: under `cargo test` several tests share one
        // process (one pid), so a pid-only name lets them clobber the
        // same directory. Nextest's per-test processes mask this.
        let dir = std::env::temp_dir().join(format!(
            "physics-device-settings-test-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn full_settings() -> DeviceSettings {
        DeviceSettings {
            input: Some("Mic".to_string()),
            output: Some("Speakers".to_string()),
            midi_input: Some("Keyboard".to_string()),
            midi_output: Some("Synth".to_string()),
        }
    }

    #[test]
    fn round_trip_save_load() {
        let dir = temp_dir("round-trip");
        let settings = full_settings();
        settings.save_to_dir(&dir).unwrap();
        assert_eq!(DeviceSettings::load_from_dir(&dir), settings);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_missing_or_malformed_returns_defaults() {
        let dir = temp_dir("malformed");
        assert_eq!(
            DeviceSettings::load_from_dir(&dir),
            DeviceSettings::default()
        );
        std::fs::write(dir.join(DEVICE_SETTINGS_FILE), "{not json").unwrap();
        assert_eq!(
            DeviceSettings::load_from_dir(&dir),
            DeviceSettings::default()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn validate_drops_stale_names_only() {
        let available = DeviceList {
            inputs: vec!["Mic".to_string()],
            outputs: vec![],
            midi_inputs: vec!["Keyboard".to_string()],
            midi_outputs: vec![],
        };
        let validated = full_settings().validate(&available);
        assert_eq!(
            validated,
            DeviceSettings {
                input: Some("Mic".to_string()),
                output: None,
                midi_input: Some("Keyboard".to_string()),
                midi_output: None,
            }
        );
        assert_eq!(
            DeviceSettings::default().validate(&available),
            DeviceSettings::default()
        );
    }

    #[test]
    fn to_args_emits_set_fields_only() {
        assert!(DeviceSettings::default().to_args().is_empty());
        assert_eq!(
            DeviceSettings {
                input: None,
                output: Some("Speakers".to_string()),
                midi_input: None,
                midi_output: Some("Synth".to_string()),
            }
            .to_args(),
            vec![
                "--output-device".to_string(),
                "Speakers".to_string(),
                "--midi-output".to_string(),
                "Synth".to_string(),
            ]
        );
    }

    #[test]
    fn has_device_flag_matches_both_forms() {
        let args = vec![
            "--backend".to_string(),
            "alsa".to_string(),
            "--output-device=Speakers".to_string(),
            "--midi-input".to_string(),
            "Keyboard".to_string(),
        ];
        assert!(has_device_flag(&args, "--output-device"));
        assert!(has_device_flag(&args, "--midi-input"));
        assert!(!has_device_flag(&args, "--input-device"));
        assert!(!has_device_flag(&args, "--midi-output"));
    }

    #[test]
    fn strip_device_flags_removes_pairs_and_joined_forms() {
        let args = vec![
            "-b".to_string(),
            "alsa".to_string(),
            "--input-device".to_string(),
            "Mic".to_string(),
            "--output-device=Speakers".to_string(),
            "--midi-input".to_string(),
            "Keyboard".to_string(),
            "--tempo".to_string(),
            "120".to_string(),
        ];
        assert_eq!(
            strip_device_flags(&args),
            vec![
                "-b".to_string(),
                "alsa".to_string(),
                "--tempo".to_string(),
                "120".to_string(),
            ]
        );
        assert!(strip_device_flags(&[]).is_empty());
    }

    #[test]
    fn enumerate_devices_never_fails() {
        // No assertion on contents: the sandbox may have no audio/MIDI
        // hardware at all. The contract is graceful degradation.
        let _ = enumerate_devices();
    }

    #[test]
    fn restart_delay_defaults_to_zero() {
        // Save/restore the process-global env var so this test asserts
        // unconditionally and never leaks a value into another test.
        // SAFETY: no other test in this crate reads this variable.
        let saved = std::env::var_os(RESTART_DELAY_ENV_VAR);
        unsafe {
            std::env::remove_var(RESTART_DELAY_ENV_VAR);
        }
        assert_eq!(restart_delay(), Duration::from_millis(0));
        unsafe {
            std::env::set_var(RESTART_DELAY_ENV_VAR, "250");
        }
        assert_eq!(restart_delay(), Duration::from_millis(250));
        unsafe {
            match saved {
                Some(value) => std::env::set_var(RESTART_DELAY_ENV_VAR, value),
                None => std::env::remove_var(RESTART_DELAY_ENV_VAR),
            }
        }
    }

    #[test]
    fn standalone_flag_defaults_to_false() {
        // Reset first: the flag is process-global, and a test running
        // earlier in this process may have called `mark_standalone`.
        // Reset again at the end so this test leaves the flag clean.
        reset_standalone_flag();
        assert!(!is_standalone());
        mark_standalone();
        assert!(is_standalone());
        reset_standalone_flag();
        assert!(!is_standalone());
    }

    #[test]
    fn load_from_dir_keeps_valid_fields_and_drops_wrong_typed() {
        let dir = temp_dir("lenient");
        std::fs::write(
            dir.join(DEVICE_SETTINGS_FILE),
            r#"{"input": 42, "output": "Speakers", "midi_input": null, "bogus": true}"#,
        )
        .unwrap();
        assert_eq!(
            DeviceSettings::load_from_dir(&dir),
            DeviceSettings {
                input: None,
                output: Some("Speakers".to_string()),
                midi_input: None,
                midi_output: None,
            }
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_from_dir_truncated_file_backed_up_and_preserved_by_save() {
        let dir = temp_dir("truncated");
        let raw = b"{\"input\": \"Mi";
        std::fs::write(dir.join(DEVICE_SETTINGS_FILE), raw).unwrap();
        let backup = dir.join(format!("{DEVICE_SETTINGS_FILE}.corrupt"));
        assert_eq!(
            DeviceSettings::load_from_dir(&dir),
            DeviceSettings::default()
        );
        assert_eq!(std::fs::read(&backup).unwrap(), raw);
        full_settings().save_to_dir(&dir).unwrap();
        assert_eq!(std::fs::read(&backup).unwrap(), raw);
        assert_eq!(DeviceSettings::load_from_dir(&dir), full_settings());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn inject_saved_devices_in_dir_explicit_flag_wins_per_field() {
        let dir = temp_dir("inject-explicit");
        full_settings().save_to_dir(&dir).unwrap();
        let available = DeviceList {
            inputs: vec!["Mic".to_string()],
            outputs: vec!["Speakers".to_string(), "Other".to_string()],
            midi_inputs: vec!["Keyboard".to_string(), "Other2".to_string()],
            midi_outputs: vec!["Synth".to_string()],
        };
        let mut args = vec![
            "--output-device".to_string(),
            "Other".to_string(),
            "--midi-input=Other2".to_string(),
        ];
        inject_saved_devices_in_dir(&mut args, &dir, &available);
        // Explicit flags win for their own field only; the saved
        // selections for the other fields are still injected.
        assert_eq!(
            args,
            vec![
                "--output-device".to_string(),
                "Other".to_string(),
                "--midi-input=Other2".to_string(),
                "--input-device".to_string(),
                "Mic".to_string(),
                "--midi-output".to_string(),
                "Synth".to_string(),
            ]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn inject_saved_devices_in_dir_help_skips() {
        let dir = temp_dir("inject-help");
        full_settings().save_to_dir(&dir).unwrap();
        let available = DeviceList {
            inputs: vec!["Mic".to_string()],
            outputs: vec!["Speakers".to_string()],
            midi_inputs: vec!["Keyboard".to_string()],
            midi_outputs: vec!["Synth".to_string()],
        };
        for help in ["--help", "-h"] {
            let mut args = vec![help.to_string()];
            inject_saved_devices_in_dir(&mut args, &dir, &available);
            assert_eq!(args, vec![help.to_string()]);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn inject_saved_devices_in_dir_drops_stale_names() {
        let dir = temp_dir("inject-stale");
        full_settings().save_to_dir(&dir).unwrap();
        // Only the input and MIDI input are still available.
        let available = DeviceList {
            inputs: vec!["Mic".to_string()],
            outputs: vec![],
            midi_inputs: vec!["Keyboard".to_string()],
            midi_outputs: vec![],
        };
        let mut args = vec!["--backend".to_string(), "alsa".to_string()];
        inject_saved_devices_in_dir(&mut args, &dir, &available);
        assert_eq!(
            args,
            vec![
                "--backend".to_string(),
                "alsa".to_string(),
                "--input-device".to_string(),
                "Mic".to_string(),
                "--midi-input".to_string(),
                "Keyboard".to_string(),
            ]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
