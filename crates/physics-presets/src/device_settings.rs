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
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DeviceSettings {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub midi_input: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub midi_output: Option<String>,
    /// Last state this value was loaded from disk with, for the
    /// save-time three-way merge in [`DeviceSettings::save_to_dir`]
    /// (same per-key rules as the aggregate sidecars in
    /// `manager.rs`: only-disk-changed fields are kept, only-writer-
    /// changed fields win — including an explicit clear to `None` —
    /// and both-changed fields resolve writer-wins). Never
    /// serialized, and ignored by equality: a value loaded from disk
    /// equals the value it was saved from. `None` means "no known
    /// disk baseline" (freshly constructed, or loaded from a missing
    /// or malformed file); such a value saves wholesale, as before.
    #[serde(skip)]
    baseline: Option<Box<DeviceSettings>>,
}

/// Equality compares only the four device fields, never the merge
/// baseline: loading a saved value must compare equal to the value
/// that was saved, even though only the loaded one carries a
/// baseline.
impl PartialEq for DeviceSettings {
    fn eq(&self, other: &Self) -> bool {
        self.input == other.input
            && self.output == other.output
            && self.midi_input == other.midi_input
            && self.midi_output == other.midi_output
    }
}

impl Eq for DeviceSettings {}

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

/// Leniently parse a device-settings sidecar's raw bytes: fields
/// with the wrong type fall back to unset with a diagnostic, while
/// valid fields are kept. Returns `None` when the whole file is not
/// a JSON object (the raw bytes are backed up by
/// [`backup_corrupt_sidecar`]). The returned value carries no merge
/// baseline; [`DeviceSettings::load_from_dir`] attaches one.
fn parse_device_settings(path: &Path, raw: &[u8]) -> Option<DeviceSettings> {
    let Ok(content) = std::str::from_utf8(raw) else {
        backup_corrupt_sidecar(path, raw);
        return None;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(content) else {
        backup_corrupt_sidecar(path, raw);
        return None;
    };
    let Some(object) = value.as_object() else {
        backup_corrupt_sidecar(path, raw);
        return None;
    };
    Some(DeviceSettings {
        input: string_field(object, path, "input"),
        output: string_field(object, path, "output"),
        midi_input: string_field(object, path, "midi_input"),
        midi_output: string_field(object, path, "midi_output"),
        baseline: None,
    })
}

/// Re-read a device-settings sidecar from disk for a save-time
/// merge. A missing file means "all defaults"; a malformed or
/// otherwise unreadable file yields `None`, telling the caller to
/// fall back to a wholesale write.
fn read_device_settings_from_disk(path: &Path) -> Option<DeviceSettings> {
    match std::fs::read(path) {
        Ok(raw) => parse_device_settings(path, &raw),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Some(DeviceSettings::default()),
        Err(err) => {
            eprintln!(
                "physics-presets: failed to re-read device settings {path}: {err}",
                path = path.display()
            );
            None
        }
    }
}

/// Per-field three-way merge, with the same rules as the aggregate
/// sidecars (see `manager.rs`): a field only the writer changed
/// wins — including an explicit clear to `None`, which is how a
/// cleared selection is distinguished from a field the writer
/// never touched — a field only changed on disk wins, including
/// an external clear, and a field both sides changed resolves
/// writer-wins.
fn merge_device_settings(
    baseline: &DeviceSettings,
    disk: &DeviceSettings,
    memory: &DeviceSettings,
) -> DeviceSettings {
    let merge_field = |base: &Option<String>,
                       on_disk: &Option<String>,
                       in_memory: &Option<String>|
     -> Option<String> {
        if on_disk == base {
            in_memory.clone()
        } else if in_memory == base {
            on_disk.clone()
        } else {
            in_memory.clone()
        }
    };
    DeviceSettings {
        input: merge_field(&baseline.input, &disk.input, &memory.input),
        output: merge_field(&baseline.output, &disk.output, &memory.output),
        midi_input: merge_field(&baseline.midi_input, &disk.midi_input, &memory.midi_input),
        midi_output: merge_field(
            &baseline.midi_output,
            &disk.midi_output,
            &memory.midi_output,
        ),
        baseline: None,
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
        let Some(mut settings) = parse_device_settings(&path, &raw) else {
            return DeviceSettings::default();
        };
        // The parsed state is the merge baseline for later saves:
        // this value was read from disk, so a save can tell which
        // fields the caller changed from the on-disk state.
        let baseline = settings.clone();
        settings.baseline = Some(Box::new(baseline));
        settings
    }

    /// This value without its merge baseline, for wholesale writes
    /// and serialization (the baseline is never serialized anyway).
    fn without_baseline(&self) -> DeviceSettings {
        let mut settings = self.clone();
        settings.baseline = None;
        settings
    }

    /// Save the selection to a directory, creating it as needed.
    /// When this value was loaded from disk (see
    /// [`DeviceSettings::load_from_dir`]), the save re-reads the
    /// file and merges per field, so a selection another instance
    /// saved for a different field survives; a value with no known
    /// disk baseline saves wholesale, as before. The write itself
    /// is atomic (temp file + rename) either way.
    pub fn save_to_dir(&self, dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join(DEVICE_SETTINGS_FILE);
        let merged = match &self.baseline {
            Some(baseline) => match read_device_settings_from_disk(&path) {
                Some(disk) => merge_device_settings(baseline, &disk, self),
                // Malformed/unreadable re-read: wholesale write.
                None => self.without_baseline(),
            },
            None => self.without_baseline(),
        };
        let json = serde_json::to_string_pretty(&merged)
            .map_err(|err| std::io::Error::new(std::io::ErrorKind::InvalidData, err))?;
        write_sidecar_atomic(&path, json.as_bytes())
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
            // Validation drops stale names from the value, but the
            // merge baseline stays the on-disk state: a field
            // dropped here counts as writer-changed on the next
            // save, so a stale name is not resurrected by merging.
            baseline: self.baseline.clone(),
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
            baseline: None,
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
                baseline: None,
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
                baseline: None,
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
                baseline: None,
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

    #[test]
    fn save_to_dir_merges_disjoint_field_changes() {
        let dir = temp_dir("merge-disjoint");
        std::fs::write(
            dir.join(DEVICE_SETTINGS_FILE),
            r#"{"input": "Mic", "output": "Speakers"}"#,
        )
        .unwrap();
        let mut loaded = DeviceSettings::load_from_dir(&dir);
        // Another instance changes the output and adds a MIDI input
        // after this value was loaded.
        std::fs::write(
            dir.join(DEVICE_SETTINGS_FILE),
            r#"{"input": "Mic", "output": "Other", "midi_input": "Keyboard"}"#,
        )
        .unwrap();
        loaded.input = Some("Mic2".to_string());
        loaded.save_to_dir(&dir).unwrap();
        assert_eq!(
            DeviceSettings::load_from_dir(&dir),
            DeviceSettings {
                input: Some("Mic2".to_string()),
                output: Some("Other".to_string()),
                midi_input: Some("Keyboard".to_string()),
                midi_output: None,
                baseline: None,
            }
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_to_dir_explicit_clear_wins_over_unchanged_disk() {
        let dir = temp_dir("merge-clear");
        std::fs::write(
            dir.join(DEVICE_SETTINGS_FILE),
            r#"{"input": "Mic", "output": "Speakers"}"#,
        )
        .unwrap();
        let mut loaded = DeviceSettings::load_from_dir(&dir);
        // Clearing a field is a writer change, not a stale default:
        // the merged file must not keep the old device.
        loaded.output = None;
        loaded.save_to_dir(&dir).unwrap();
        assert_eq!(
            DeviceSettings::load_from_dir(&dir),
            DeviceSettings {
                input: Some("Mic".to_string()),
                output: None,
                midi_input: None,
                midi_output: None,
                baseline: None,
            }
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_to_dir_external_clear_is_not_resurrected() {
        let dir = temp_dir("merge-ext-clear");
        full_settings().save_to_dir(&dir).unwrap();
        let mut loaded = DeviceSettings::load_from_dir(&dir);
        // Another instance clears everything but the input.
        std::fs::write(dir.join(DEVICE_SETTINGS_FILE), r#"{"input": "Mic"}"#).unwrap();
        loaded.midi_output = Some("Synth2".to_string());
        loaded.save_to_dir(&dir).unwrap();
        assert_eq!(
            DeviceSettings::load_from_dir(&dir),
            DeviceSettings {
                input: Some("Mic".to_string()),
                output: None,
                midi_input: None,
                midi_output: Some("Synth2".to_string()),
                baseline: None,
            }
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_to_dir_without_baseline_writes_wholesale() {
        let dir = temp_dir("merge-wholesale");
        full_settings().save_to_dir(&dir).unwrap();
        // A freshly constructed value never read the file, so it
        // carries no baseline and saves wholesale, as before.
        let fresh = DeviceSettings {
            input: Some("Mic2".to_string()),
            output: None,
            midi_input: None,
            midi_output: None,
            baseline: None,
        };
        fresh.save_to_dir(&dir).unwrap();
        assert_eq!(DeviceSettings::load_from_dir(&dir), fresh);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_to_dir_loaded_value_rewrites_identical_bytes() {
        let dir = temp_dir("merge-noop");
        full_settings().save_to_dir(&dir).unwrap();
        let before = std::fs::read(dir.join(DEVICE_SETTINGS_FILE)).unwrap();
        let loaded = DeviceSettings::load_from_dir(&dir);
        loaded.save_to_dir(&dir).unwrap();
        assert_eq!(
            std::fs::read(dir.join(DEVICE_SETTINGS_FILE)).unwrap(),
            before
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
