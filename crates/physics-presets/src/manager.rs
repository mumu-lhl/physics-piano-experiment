//! Preset manager handling factory presets, user preset disk persistence,
//! and import/export functionality.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::preset::Preset;

/// Controller CC numbers that MIDI Learn must never bind: they already have
/// a fixed meaning (piano sustain / sostenuto / una corda / expression,
/// drum hi-hat pedal) and stealing them would break playing.
pub const MIDI_LEARN_EXCLUDED_CCS: [u8; 5] = [64, 66, 67, 11, 4];

/// Normalize user-entered tags: trim whitespace, drop empties, dedupe
/// case-insensitively preserving first-seen order and first-seen
/// spelling, so "Warm" and "warm" are one tag, displayed as "Warm".
fn normalize_tags(tags: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for tag in tags {
        let tag = tag.trim().to_string();
        if tag.is_empty() {
            continue;
        }
        let key = tag.to_lowercase();
        if !seen.contains(&key) {
            seen.push(key);
            out.push(tag);
        }
    }
    out
}

/// Map a preset ID to a file stem: alphanumerics, `_` and `-` survive,
/// every other character becomes `_`. Shared by save/delete/load so all
/// three agree on which file an ID owns; collisions are checked by the
/// callers, not hidden here.
fn sanitize_preset_filename(id: &str) -> String {
    id.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Write `contents` to `path` atomically: a temp file in the same
/// directory, then a rename, so a concurrent reader never sees a torn
/// file and a crash mid-write leaves the previous contents intact.
/// Temp names carry the pid plus a per-process counter so concurrent
/// writers in one process never share a temp file. This is
/// last-writer-wins: there is no locking and no re-read/merge, so two
/// instances writing the same sidecar can still clobber each other
/// (wholesale, never torn).
pub(crate) fn write_sidecar_atomic(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);
    let file_name = path.file_name().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "sidecar path has no file name",
        )
    })?;
    let dir = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let tmp = dir.join(format!(
        ".{}.tmp-{}-{}",
        file_name.to_string_lossy(),
        std::process::id(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    fs::write(&tmp, contents)?;
    match fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(err) => {
            let _ = fs::remove_file(&tmp);
            Err(err)
        }
    }
}

/// Best-effort backup of a malformed sidecar's raw bytes next to the file
/// (`<file>.corrupt`, overwriting any previous backup of the same file:
/// the newest corruption is the one worth keeping), plus a diagnostic.
/// The caller then starts from empty/defaults, so the next save cannot
/// silently destroy the user's data without a trace.
pub(crate) fn backup_corrupt_sidecar(path: &Path, raw: &[u8]) {
    let backup = PathBuf::from(format!("{}.corrupt", path.display()));
    match fs::write(&backup, raw) {
        Ok(()) => eprintln!(
            "physics-presets: ignoring malformed sidecar {}; raw bytes backed up to {}",
            path.display(),
            backup.display()
        ),
        Err(err) => eprintln!(
            "physics-presets: ignoring malformed sidecar {}; failed to write backup {}: {err}",
            path.display(),
            backup.display()
        ),
    }
}

/// Read a sidecar file's raw bytes. A missing file is silent (`None`);
/// any other read failure is reported and also yields `None`.
fn read_sidecar_bytes(path: &Path) -> Option<Vec<u8>> {
    match fs::read(path) {
        Ok(bytes) => Some(bytes),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
        Err(err) => {
            eprintln!(
                "physics-presets: failed to read sidecar {}: {err}",
                path.display()
            );
            None
        }
    }
}

#[derive(Debug, Clone)]
pub struct PresetManager {
    instrument: String,
    presets: Vec<Preset>,
    user_presets_dir: PathBuf,
    /// IDs of presets the user starred in the preset browser. Persisted in
    /// `favorites.json` next to the user presets, so factory presets can be
    /// starred without touching their embedded definitions.
    favorites: std::collections::HashSet<String>,
    /// MIDI Learn bindings: controller CC number -> parameter id string.
    /// Persisted in `midi_mappings.json` next to the user presets, so a
    /// controller mapping survives restarts and is independent of presets.
    midi_mappings: std::collections::HashMap<u8, String>,
    /// Tags assigned to presets: preset ID -> tags in entry order.
    /// Persisted in `preset_tags.json` next to the user presets, so factory
    /// presets can be tagged without touching their embedded definitions,
    /// and tagging never migrates the preset format.
    tags: std::collections::HashMap<String, Vec<String>>,
}

impl PresetManager {
    /// Create a new PresetManager for the given instrument with factory presets.
    pub fn new(instrument: &str, mut factory_presets: Vec<Preset>) -> Self {
        // Tag factory presets
        for p in &mut factory_presets {
            p.is_factory = true;
            p.instrument = instrument.to_string();
        }

        let user_presets_dir = default_user_presets_dir(instrument);

        let mut mgr = Self {
            instrument: instrument.to_string(),
            presets: factory_presets,
            user_presets_dir,
            favorites: std::collections::HashSet::new(),
            midi_mappings: std::collections::HashMap::new(),
            tags: std::collections::HashMap::new(),
        };

        // Attempt to load existing user presets from disk
        if let Err(err) = mgr.load_user_presets() {
            eprintln!(
                "physics-presets: failed to load user presets from {}: {err}",
                mgr.user_presets_dir.display()
            );
        }
        mgr.load_favorites();
        mgr.load_midi_mappings();
        mgr.load_tags();
        mgr
    }

    /// With custom user presets directory (useful for testing or customized configs)
    pub fn with_custom_dir(
        instrument: &str,
        mut factory_presets: Vec<Preset>,
        dir: PathBuf,
    ) -> Self {
        for p in &mut factory_presets {
            p.is_factory = true;
            p.instrument = instrument.to_string();
        }

        let mut mgr = Self {
            instrument: instrument.to_string(),
            presets: factory_presets,
            user_presets_dir: dir,
            favorites: std::collections::HashSet::new(),
            midi_mappings: std::collections::HashMap::new(),
            tags: std::collections::HashMap::new(),
        };
        if let Err(err) = mgr.load_user_presets() {
            eprintln!(
                "physics-presets: failed to load user presets from {}: {err}",
                mgr.user_presets_dir.display()
            );
        }
        mgr.load_favorites();
        mgr.load_midi_mappings();
        mgr.load_tags();
        mgr
    }

    /// Path to user presets directory on disk
    pub fn user_presets_dir(&self) -> &Path {
        &self.user_presets_dir
    }

    /// All available presets (factory presets first, followed by user presets)
    pub fn presets(&self) -> &[Preset] {
        &self.presets
    }

    /// Get preset by ID
    pub fn get_preset(&self, id: &str) -> Option<&Preset> {
        self.presets.iter().find(|p| p.id == id)
    }

    /// Path of the favorites file (a JSON array of preset IDs).
    pub fn favorites_file(&self) -> PathBuf {
        self.user_presets_dir.join("favorites.json")
    }

    /// Whether the given preset is starred. Unknown IDs are never favorites.
    pub fn is_favorite(&self, id: &str) -> bool {
        self.presets.iter().any(|p| p.id == id) && self.favorites.contains(id)
    }

    /// Starred preset IDs, in no particular order.
    pub fn favorites(&self) -> Vec<String> {
        self.favorites.iter().cloned().collect()
    }

    /// Load starred IDs from disk, leniently: non-string entries are
    /// dropped with a diagnostic, and only IDs that still exist are
    /// kept. A missing file means "no favorites yet"; a whole file that
    /// is not a JSON array is backed up as `<file>.corrupt` and loads
    /// empty, so the next save cannot silently destroy it.
    pub fn load_favorites(&mut self) {
        let path = self.favorites_file();
        let Some(raw) = read_sidecar_bytes(&path) else {
            return;
        };
        let Ok(content) = std::str::from_utf8(&raw) else {
            backup_corrupt_sidecar(&path, &raw);
            return;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(content) else {
            backup_corrupt_sidecar(&path, &raw);
            return;
        };
        let Some(entries) = value.as_array() else {
            backup_corrupt_sidecar(&path, &raw);
            return;
        };
        self.favorites = entries
            .iter()
            .filter_map(|entry| match entry {
                serde_json::Value::String(id) => Some(id.clone()),
                other => {
                    eprintln!(
                        "physics-presets: dropping non-string favorite entry {other} in {}",
                        path.display()
                    );
                    None
                }
            })
            .filter(|id| self.presets.iter().any(|p| &p.id == id))
            .collect();
    }

    /// Persist the current starred IDs to disk.
    pub fn save_favorites(&self) -> Result<(), std::io::Error> {
        if !self.user_presets_dir.exists() {
            fs::create_dir_all(&self.user_presets_dir)?;
        }
        let mut ids: Vec<&String> = self.favorites.iter().collect();
        ids.sort();
        let json = serde_json::to_string_pretty(&ids)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        write_sidecar_atomic(&self.favorites_file(), json.as_bytes())
    }

    /// Toggle the starred state of a preset and persist it. Returns the new
    /// state (`true` = starred). Unknown IDs are ignored and report `false`.
    pub fn toggle_favorite(&mut self, id: &str) -> Result<bool, std::io::Error> {
        if !self.presets.iter().any(|p| p.id == id) {
            return Ok(false);
        }
        let starred = if self.favorites.contains(id) {
            self.favorites.remove(id);
            false
        } else {
            self.favorites.insert(id.to_string());
            true
        };
        self.save_favorites()?;
        Ok(starred)
    }

    /// Path of the MIDI Learn mappings file (a JSON object of CC number
    /// to parameter id).
    pub fn midi_mappings_file(&self) -> PathBuf {
        self.user_presets_dir.join("midi_mappings.json")
    }

    /// Parameter id bound to the given controller CC, if any.
    pub fn midi_mapping(&self, cc: u8) -> Option<&str> {
        self.midi_mappings.get(&cc).map(|id| id.as_str())
    }

    /// All current MIDI Learn bindings (CC -> parameter id).
    pub fn midi_mappings(&self) -> &std::collections::HashMap<u8, String> {
        &self.midi_mappings
    }

    /// Load MIDI Learn bindings from disk, leniently: entries whose key
    /// is not a `u8` or whose value is not a string are dropped with a
    /// diagnostic, and excluded CCs are still filtered. A missing file
    /// means "no mappings yet"; a whole file that is not a JSON object
    /// is backed up as `<file>.corrupt` and loads empty.
    pub fn load_midi_mappings(&mut self) {
        let path = self.midi_mappings_file();
        let Some(raw) = read_sidecar_bytes(&path) else {
            return;
        };
        let Ok(content) = std::str::from_utf8(&raw) else {
            backup_corrupt_sidecar(&path, &raw);
            return;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(content) else {
            backup_corrupt_sidecar(&path, &raw);
            return;
        };
        let Some(entries) = value.as_object() else {
            backup_corrupt_sidecar(&path, &raw);
            return;
        };
        let mut mappings = std::collections::HashMap::new();
        for (key, entry) in entries {
            let Ok(cc) = key.parse::<u8>() else {
                eprintln!(
                    "physics-presets: dropping MIDI mapping with non-u8 key {key:?} in {}",
                    path.display()
                );
                continue;
            };
            let serde_json::Value::String(param_id) = entry else {
                eprintln!(
                    "physics-presets: dropping MIDI mapping for CC {cc} with non-string value {entry} in {}",
                    path.display()
                );
                continue;
            };
            if !MIDI_LEARN_EXCLUDED_CCS.contains(&cc) {
                mappings.insert(cc, param_id.clone());
            }
        }
        self.midi_mappings = mappings;
    }

    /// Persist the current MIDI Learn bindings to disk.
    pub fn save_midi_mappings(&self) -> Result<(), std::io::Error> {
        if !self.user_presets_dir.exists() {
            fs::create_dir_all(&self.user_presets_dir)?;
        }
        let sorted: std::collections::BTreeMap<u8, &String> = self
            .midi_mappings
            .iter()
            .map(|(cc, id)| (*cc, id))
            .collect();
        let json = serde_json::to_string_pretty(&sorted)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        write_sidecar_atomic(&self.midi_mappings_file(), json.as_bytes())
    }

    /// Bind a controller CC to a parameter id, exclusively: any previous
    /// binding of that parameter (to another CC) and any previous binding
    /// of that CC are replaced. Returns `false` (and saves nothing) for
    /// excluded CCs.
    pub fn bind_midi_cc(&mut self, cc: u8, param_id: &str) -> Result<bool, std::io::Error> {
        if MIDI_LEARN_EXCLUDED_CCS.contains(&cc) {
            return Ok(false);
        }
        self.midi_mappings.retain(|_, id| id != param_id);
        self.midi_mappings.insert(cc, param_id.to_string());
        self.save_midi_mappings()?;
        Ok(true)
    }

    /// Remove the binding for a controller CC, if any. Returns whether a
    /// binding was removed.
    pub fn unbind_midi_cc(&mut self, cc: u8) -> Result<bool, std::io::Error> {
        let removed = self.midi_mappings.remove(&cc).is_some();
        if removed {
            self.save_midi_mappings()?;
        }
        Ok(removed)
    }

    /// Remove every binding whose parameter ID is not in
    /// `valid_param_ids` (e.g. after a parameter rename/removal in the
    /// plugin layer, which owns the parameter table this manager never
    /// sees). Persists via [`Self::save_midi_mappings`] only if
    /// something was removed, and returns whether anything was pruned.
    /// A persist failure is reported but does not undo the in-memory
    /// prune.
    pub fn prune_stale_midi_mappings(
        &mut self,
        valid_param_ids: &std::collections::HashSet<String>,
    ) -> bool {
        let before = self.midi_mappings.len();
        self.midi_mappings
            .retain(|_, param_id| valid_param_ids.contains(param_id));
        let pruned = self.midi_mappings.len() != before;
        if pruned {
            match self.save_midi_mappings() {
                Ok(()) => {}
                Err(err) => eprintln!(
                    "physics-presets: failed to persist pruned MIDI mappings to {}: {err}",
                    self.midi_mappings_file().display()
                ),
            }
        }
        pruned
    }

    /// Path of the tags file (a JSON object of preset ID to tag list).
    pub fn tags_file(&self) -> PathBuf {
        self.user_presets_dir.join("preset_tags.json")
    }

    /// Tags assigned to the given preset, in entry order. Unknown IDs have none.
    pub fn tags_for(&self, id: &str) -> Vec<String> {
        if !self.presets.iter().any(|p| p.id == id) {
            return Vec::new();
        }
        self.tags.get(id).cloned().unwrap_or_default()
    }

    /// Every tag in use, in first-seen order over the preset list.
    /// Deduplicated case-insensitively (first-seen spelling wins), so
    /// "Warm" on one preset and "warm" on another are one chip.
    pub fn all_tags(&self) -> Vec<String> {
        let mut tags = Vec::new();
        let mut seen: Vec<String> = Vec::new();
        for preset in &self.presets {
            if let Some(preset_tags) = self.tags.get(&preset.id) {
                for tag in preset_tags {
                    let key = tag.to_lowercase();
                    if !seen.contains(&key) {
                        seen.push(key);
                        tags.push(tag.clone());
                    }
                }
            }
        }
        tags
    }

    /// Load tags from disk, leniently: entries for unknown presets are
    /// dropped, non-array values are dropped with a diagnostic, and
    /// non-string elements inside an array are dropped with a
    /// diagnostic while the valid strings are kept. A missing file
    /// means "no tags yet"; a whole file that is not a JSON object is
    /// backed up as `<file>.corrupt` and loads empty.
    pub fn load_tags(&mut self) {
        let path = self.tags_file();
        let Some(raw) = read_sidecar_bytes(&path) else {
            return;
        };
        let Ok(content) = std::str::from_utf8(&raw) else {
            backup_corrupt_sidecar(&path, &raw);
            return;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(content) else {
            backup_corrupt_sidecar(&path, &raw);
            return;
        };
        let Some(entries) = value.as_object() else {
            backup_corrupt_sidecar(&path, &raw);
            return;
        };
        let mut tags = std::collections::HashMap::new();
        for (id, entry) in entries {
            if !self.presets.iter().any(|p| &p.id == id) {
                continue;
            }
            let Some(list) = entry.as_array() else {
                eprintln!(
                    "physics-presets: dropping non-array tags entry for {id:?} in {}",
                    path.display()
                );
                continue;
            };
            let kept: Vec<String> = list
                .iter()
                .filter_map(|item| match item {
                    serde_json::Value::String(tag) => Some(tag.clone()),
                    other => {
                        eprintln!(
                            "physics-presets: dropping non-string tag {other} for {id:?} in {}",
                            path.display()
                        );
                        None
                    }
                })
                .collect();
            let kept = normalize_tags(kept);
            if !kept.is_empty() {
                tags.insert(id.clone(), kept);
            }
        }
        self.tags = tags;
    }

    /// Persist the current tags to disk.
    pub fn save_tags(&self) -> Result<(), std::io::Error> {
        if !self.user_presets_dir.exists() {
            fs::create_dir_all(&self.user_presets_dir)?;
        }
        let sorted: std::collections::BTreeMap<&String, &Vec<String>> = self.tags.iter().collect();
        let json = serde_json::to_string_pretty(&sorted)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        write_sidecar_atomic(&self.tags_file(), json.as_bytes())
    }

    /// Replace the tags of a preset and persist them. Returns whether the
    /// preset exists. Unknown IDs are ignored. An empty list clears the
    /// preset's tags.
    pub fn set_tags(&mut self, id: &str, tags: Vec<String>) -> Result<bool, std::io::Error> {
        if !self.presets.iter().any(|p| p.id == id) {
            return Ok(false);
        }
        let tags = normalize_tags(tags);
        if tags.is_empty() {
            self.tags.remove(id);
        } else {
            self.tags.insert(id.to_string(), tags);
        }
        self.save_tags()?;
        Ok(true)
    }

    /// Load or reload user presets from disk
    pub fn load_user_presets(&mut self) -> Result<usize, std::io::Error> {
        if !self.user_presets_dir.exists() {
            fs::create_dir_all(&self.user_presets_dir)?;
            return Ok(0);
        }

        // Retain only factory presets before reloading user presets
        self.presets.retain(|p| p.is_factory);

        let mut count = 0;
        let entries = fs::read_dir(&self.user_presets_dir)?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
                if let Ok(content) = fs::read_to_string(&path) {
                    if let Ok(mut preset) = Preset::from_json(&content) {
                        preset.is_factory = false;
                        preset.instrument = self.instrument.clone();
                        if preset.id.is_empty() {
                            eprintln!(
                                "physics-presets: skipping user preset with an empty ID in {}",
                                path.display()
                            );
                            continue;
                        }
                        // Prevent ID collision with factory presets, and
                        // filename collision with an already-loaded
                        // preset: two IDs sanitizing to one file would
                        // overwrite each other on save, so the first
                        // file seen wins.
                        let collides = self.presets.iter().any(|p| {
                            p.id == preset.id
                                || (p.id != preset.id
                                    && sanitize_preset_filename(&p.id)
                                        == sanitize_preset_filename(&preset.id))
                        });
                        if collides {
                            eprintln!(
                                "physics-presets: skipping user preset {:?} in {}: ID or sanitized file name collides with an already-loaded preset",
                                preset.id,
                                path.display()
                            );
                            continue;
                        }
                        self.presets.push(preset);
                        count += 1;
                    }
                }
            }
        }

        Ok(count)
    }

    /// Save a user preset to disk. Rejects an empty ID, and an ID whose
    /// sanitized file name collides with a different existing preset's
    /// (e.g. `"a b"` vs `"a_b"`), instead of silently overwriting the
    /// other preset's file. Re-saving the same ID is allowed.
    pub fn save_user_preset(&mut self, mut preset: Preset) -> Result<(), std::io::Error> {
        preset.is_factory = false;
        preset.instrument = self.instrument.clone();

        if preset.id.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "preset ID must not be empty",
            ));
        }
        let safe_filename = sanitize_preset_filename(&preset.id);
        if let Some(other) = self.presets.iter().find(|p| {
            p.id != preset.id && sanitize_preset_filename(&p.id) == safe_filename
        }) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!(
                    "preset ID {:?} sanitizes to the same file name as existing preset {:?}",
                    preset.id, other.id
                ),
            ));
        }

        if !self.user_presets_dir.exists() {
            fs::create_dir_all(&self.user_presets_dir)?;
        }

        let path = self.user_presets_dir.join(format!("{safe_filename}.json"));

        let json = preset
            .to_json()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        write_sidecar_atomic(&path, json.as_bytes())?;

        // Update in-memory list
        if let Some(pos) = self
            .presets
            .iter()
            .position(|p| p.id == preset.id && !p.is_factory)
        {
            self.presets[pos] = preset;
        } else {
            self.presets.push(preset);
        }

        Ok(())
    }

    /// Check if a preset is a user preset (and thus editable/overwritable)
    pub fn is_user_preset(&self, id: &str) -> bool {
        self.presets.iter().any(|p| p.id == id && !p.is_factory)
    }

    /// Overwrite an existing user preset's parameters with new parameter values
    pub fn overwrite_user_preset(
        &mut self,
        id: &str,
        params: std::collections::HashMap<String, f32>,
    ) -> Result<bool, std::io::Error> {
        if let Some(pos) = self
            .presets
            .iter()
            .position(|p| p.id == id && !p.is_factory)
        {
            self.presets[pos].params = params;
            let preset = self.presets[pos].clone();
            self.save_user_preset(preset)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Rename an existing user preset and persist change to disk
    pub fn rename_user_preset(&mut self, id: &str, new_name: &str) -> Result<bool, std::io::Error> {
        if let Some(pos) = self
            .presets
            .iter()
            .position(|p| p.id == id && !p.is_factory)
        {
            self.presets[pos].name = new_name.to_string();
            let preset = self.presets[pos].clone();
            self.save_user_preset(preset)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Delete a user preset from disk and memory
    pub fn delete_user_preset(&mut self, id: &str) -> Result<bool, std::io::Error> {
        let is_user = self.presets.iter().any(|p| p.id == id && !p.is_factory);
        if !is_user {
            return Ok(false);
        }

        let safe_filename = sanitize_preset_filename(id);
        let path = self.user_presets_dir.join(format!("{safe_filename}.json"));
        if path.exists() {
            fs::remove_file(path)?;
        }

        self.presets.retain(|p| p.id != id || p.is_factory);
        // Tags and favorites are keyed by preset ID; a deleted preset
        // takes both with it (best-effort: the delete itself already
        // succeeded), so a later preset reusing the ID does not inherit
        // the deleted preset's star or tags.
        if self.tags.remove(id).is_some() {
            let _ = self.save_tags();
        }
        if self.favorites.remove(id) {
            let _ = self.save_favorites();
        }
        Ok(true)
    }

    /// Export preset to any file location
    pub fn export_preset_to_file(&self, id: &str, path: &Path) -> Result<(), std::io::Error> {
        let preset = self
            .get_preset(id)
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "Preset not found"))?;
        let json = preset
            .to_json()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        write_sidecar_atomic(path, json.as_bytes())?;
        Ok(())
    }

    /// Import preset from an external JSON file
    pub fn import_preset_from_file(&mut self, path: &Path) -> Result<Preset, std::io::Error> {
        let content = fs::read_to_string(path)?;
        let mut preset = Preset::from_json(&content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        preset.is_factory = false;
        preset.instrument = self.instrument.clone();
        self.save_user_preset(preset.clone())?;
        Ok(preset)
    }
}

/// Computes the default user presets directory:
/// Linux: `$XDG_DATA_HOME/physics-instruments/presets/{instrument}/` (~/.local/share/physics-instruments/presets/{instrument}/)
/// Windows: `%APPDATA%\physics-instruments\presets\{instrument}\`
/// macOS: `~/Library/Application Support/physics-instruments/presets/{instrument}/`
pub(crate) fn default_user_presets_dir(instrument: &str) -> PathBuf {
    user_presets_dir_for_data_dir(dirs::data_dir(), instrument)
}

/// Pure form of [`default_user_presets_dir`], taking the data directory
/// as an argument so the fallback is testable without unsettling the
/// process environment. When no data directory is available, presets
/// go to a deterministic per-instrument directory under the system
/// temp dir (never the process working directory, which depends on
/// where the host was launched from), and the fallback is diagnosed
/// exactly once per process.
fn user_presets_dir_for_data_dir(data_dir: Option<PathBuf>, instrument: &str) -> PathBuf {
    match data_dir {
        Some(base) => base
            .join("physics-instruments")
            .join("presets")
            .join(instrument),
        None => {
            static FALLBACK_WARNED: AtomicBool = AtomicBool::new(false);
            if !FALLBACK_WARNED.swap(true, Ordering::Relaxed) {
                eprintln!(
                    "physics-presets: dirs::data_dir() is unavailable; using fallback presets directory under the system temp dir"
                );
            }
            std::env::temp_dir()
                .join("physics-instruments-presets")
                .join(instrument)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preset::Preset;
    use std::collections::HashMap;

    struct TempDirGuard(PathBuf);

    impl TempDirGuard {
        fn new(tag: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("ppe-fav-test-{}-{}", tag, std::process::id()));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).expect("tempdir");
            Self(path)
        }
    }

    impl Drop for TempDirGuard {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn test_manager(dir: &Path) -> PresetManager {
        let mut params = HashMap::new();
        params.insert("gain".to_string(), 0.5);
        let factory = vec![
            Preset::new("a", "Alpha", "piano", params.clone()),
            Preset::new("b", "Beta", "piano", params),
        ];
        PresetManager::with_custom_dir("piano", factory, dir.to_path_buf())
    }

    fn factory_pair() -> Vec<Preset> {
        vec![
            Preset::new("a", "Alpha", "piano", HashMap::new()),
            Preset::new("b", "Beta", "piano", HashMap::new()),
        ]
    }

    #[test]
    fn favorites_toggle_persists_and_reloads() {
        let guard = TempDirGuard::new("reload");
        let mut mgr = test_manager(&guard.0);
        assert!(!mgr.is_favorite("a"));
        assert_eq!(mgr.toggle_favorite("a").unwrap(), true);
        assert!(mgr.is_favorite("a"));
        // A fresh manager over the same dir picks the star back up.
        let mgr2 = PresetManager::with_custom_dir("piano", factory_pair(), guard.0.clone());
        assert!(mgr2.is_favorite("a"));
        assert!(!mgr2.is_favorite("b"));
        // Toggling off persists too.
        let mut mgr3 = mgr2;
        assert_eq!(mgr3.toggle_favorite("a").unwrap(), false);
        assert!(!mgr3.is_favorite("a"));
    }

    #[test]
    fn favorites_ignore_unknown_ids() {
        let guard = TempDirGuard::new("unknown");
        let mut mgr = test_manager(&guard.0);
        assert_eq!(mgr.toggle_favorite("nope").unwrap(), false);
        assert!(!mgr.is_favorite("nope"));
        assert!(mgr.favorites().is_empty());
    }

    #[test]
    fn midi_bind_persists_and_reloads() {
        let guard = TempDirGuard::new("midi-reload");
        let mut mgr = test_manager(&guard.0);
        assert_eq!(mgr.bind_midi_cc(7, "tone").unwrap(), true);
        assert_eq!(mgr.midi_mapping(7), Some("tone"));
        // A fresh manager over the same dir picks the binding back up.
        let mgr2 = PresetManager::with_custom_dir("piano", factory_pair(), guard.0.clone());
        assert_eq!(mgr2.midi_mapping(7), Some("tone"));
        assert_eq!(mgr2.midi_mapping(8), None);
    }

    #[test]
    fn midi_bind_is_exclusive_both_ways() {
        let guard = TempDirGuard::new("midi-exclusive");
        let mut mgr = test_manager(&guard.0);
        assert_eq!(mgr.bind_midi_cc(7, "tone").unwrap(), true);
        // Binding the same parameter to another CC steals it from CC 7.
        assert_eq!(mgr.bind_midi_cc(8, "tone").unwrap(), true);
        assert_eq!(mgr.midi_mapping(7), None);
        assert_eq!(mgr.midi_mapping(8), Some("tone"));
        // Binding another parameter to CC 8 replaces the old one.
        assert_eq!(mgr.bind_midi_cc(8, "gain").unwrap(), true);
        assert_eq!(mgr.midi_mapping(8), Some("gain"));
        assert_eq!(mgr.midi_mappings().len(), 1);
    }

    #[test]
    fn midi_bind_refuses_excluded_ccs() {
        let guard = TempDirGuard::new("midi-excluded");
        let mut mgr = test_manager(&guard.0);
        for cc in MIDI_LEARN_EXCLUDED_CCS {
            assert_eq!(mgr.bind_midi_cc(cc, "tone").unwrap(), false);
            assert_eq!(mgr.midi_mapping(cc), None);
        }
        assert!(mgr.midi_mappings().is_empty());
        // Excluded CCs are also dropped when loading a hand-edited file.
        fs::write(mgr.midi_mappings_file(), r#"{"64": "tone", "7": "gain"}"#).unwrap();
        let mgr2 = PresetManager::with_custom_dir("piano", factory_pair(), guard.0.clone());
        assert_eq!(mgr2.midi_mapping(64), None);
        assert_eq!(mgr2.midi_mapping(7), Some("gain"));
    }

    #[test]
    fn midi_mappings_ignore_corrupt_file() {
        let guard = TempDirGuard::new("midi-corrupt");
        let mgr = test_manager(&guard.0);
        fs::write(mgr.midi_mappings_file(), "not json at all").unwrap();
        let mgr2 = PresetManager::with_custom_dir("piano", factory_pair(), guard.0.clone());
        assert!(mgr2.midi_mappings().is_empty());
    }

    #[test]
    fn midi_unbind_removes_and_persists() {
        let guard = TempDirGuard::new("midi-unbind");
        let mut mgr = test_manager(&guard.0);
        assert_eq!(mgr.bind_midi_cc(7, "tone").unwrap(), true);
        assert_eq!(mgr.unbind_midi_cc(7).unwrap(), true);
        assert_eq!(mgr.unbind_midi_cc(7).unwrap(), false);
        let mgr2 = PresetManager::with_custom_dir("piano", factory_pair(), guard.0.clone());
        assert_eq!(mgr2.midi_mapping(7), None);
    }

    #[test]
    fn tags_set_persists_and_reloads() {
        let guard = TempDirGuard::new("tags-reload");
        let mut mgr = test_manager(&guard.0);
        assert!(mgr.tags_for("a").is_empty());
        assert_eq!(
            mgr.set_tags("a", vec!["warm".to_string(), "jazz".to_string()])
                .unwrap(),
            true
        );
        assert_eq!(mgr.tags_for("a"), vec!["warm", "jazz"]);
        assert_eq!(mgr.all_tags(), vec!["warm", "jazz"]);
        // A fresh manager over the same dir picks the tags back up.
        let mgr2 = PresetManager::with_custom_dir("piano", factory_pair(), guard.0.clone());
        assert_eq!(mgr2.tags_for("a"), vec!["warm", "jazz"]);
        assert_eq!(mgr2.tags_for("b"), Vec::<String>::new());
    }

    #[test]
    fn tags_are_normalized_and_empty_clears() {
        let guard = TempDirGuard::new("tags-normalize");
        let mut mgr = test_manager(&guard.0);
        assert_eq!(
            mgr.set_tags(
                "a",
                vec![
                    " warm ".to_string(),
                    "warm".to_string(),
                    "WARM".to_string(),
                    "".to_string(),
                    "  ".to_string(),
                    "jazz".to_string(),
                    "JAZZ".to_string(),
                ]
            )
            .unwrap(),
            true
        );
        assert_eq!(mgr.tags_for("a"), vec!["warm", "jazz"]);
        // Clearing removes the entry entirely, including from all_tags.
        assert_eq!(mgr.set_tags("a", vec![]).unwrap(), true);
        assert!(mgr.tags_for("a").is_empty());
        assert!(mgr.all_tags().is_empty());
        let mgr2 = PresetManager::with_custom_dir("piano", factory_pair(), guard.0.clone());
        assert!(mgr2.tags_for("a").is_empty());
    }

    #[test]
    fn tags_ignore_unknown_ids() {
        let guard = TempDirGuard::new("tags-unknown");
        let mut mgr = test_manager(&guard.0);
        assert_eq!(mgr.set_tags("nope", vec!["x".to_string()]).unwrap(), false);
        assert!(mgr.tags_for("nope").is_empty());
        assert!(mgr.all_tags().is_empty());
        // Hand-edited entries for unknown IDs are dropped on load.
        fs::write(mgr.tags_file(), r#"{"nope": ["x"], "a": ["warm"]}"#).unwrap();
        let mgr2 = PresetManager::with_custom_dir("piano", factory_pair(), guard.0.clone());
        assert!(mgr2.tags_for("nope").is_empty());
        assert_eq!(mgr2.tags_for("a"), vec!["warm"]);
        // Malformed files simply mean "no tags yet".
        fs::write(mgr.tags_file(), "not json at all").unwrap();
        let mgr3 = PresetManager::with_custom_dir("piano", factory_pair(), guard.0.clone());
        assert!(mgr3.all_tags().is_empty());
    }

    #[test]
    fn tags_follow_preset_order_and_deletion() {
        let guard = TempDirGuard::new("tags-order");
        let mut mgr = test_manager(&guard.0);
        mgr.set_tags("b", vec!["shared".to_string(), "bright".to_string()])
            .unwrap();
        mgr.set_tags("a", vec!["shared".to_string(), "warm".to_string()])
            .unwrap();
        // First-seen order follows the preset list (a before b), not
        // the order the tags were set in.
        assert_eq!(mgr.all_tags(), vec!["shared", "warm", "bright"]);
        // Deleting a user preset drops its tags.
        let mut params = HashMap::new();
        params.insert("gain".to_string(), 0.5);
        let user = Preset::new("u1", "User One", "piano", params);
        mgr.save_user_preset(user).unwrap();
        mgr.set_tags("u1", vec!["mine".to_string()]).unwrap();
        assert!(mgr.delete_user_preset("u1").unwrap());
        assert!(mgr.tags_for("u1").is_empty());
        assert!(!mgr.all_tags().contains(&"mine".to_string()));
        let mgr2 = PresetManager::with_custom_dir("piano", factory_pair(), guard.0.clone());
        assert!(!mgr2.all_tags().contains(&"mine".to_string()));
    }

    #[test]
    fn favorites_lenient_load_keeps_valid_entries() {
        let guard = TempDirGuard::new("fav-lenient");
        let mgr = test_manager(&guard.0);
        fs::write(
            mgr.favorites_file(),
            r#"["a", 42, true, "b", null, "nope"]"#,
        )
        .unwrap();
        let mgr2 = PresetManager::with_custom_dir("piano", factory_pair(), guard.0.clone());
        assert!(mgr2.is_favorite("a"));
        assert!(mgr2.is_favorite("b"));
        assert_eq!(mgr2.favorites().len(), 2);
        // A missing favorites file creates no .corrupt backup.
        let guard2 = TempDirGuard::new("fav-missing");
        let _mgr3 = test_manager(&guard2.0);
        assert!(!guard2.0.join("favorites.json.corrupt").exists());
    }

    #[test]
    fn midi_mappings_lenient_load_keeps_valid_entries() {
        let guard = TempDirGuard::new("midi-lenient");
        let mgr = test_manager(&guard.0);
        fs::write(
            mgr.midi_mappings_file(),
            r#"{"7": "gain", "abc": "tone", "300": "x", "8": 42, "64": "tone", "9": "lid"}"#,
        )
        .unwrap();
        let mgr2 = PresetManager::with_custom_dir("piano", factory_pair(), guard.0.clone());
        assert_eq!(mgr2.midi_mapping(7), Some("gain"));
        assert_eq!(mgr2.midi_mapping(9), Some("lid"));
        assert_eq!(mgr2.midi_mapping(8), None);
        assert_eq!(mgr2.midi_mapping(64), None);
        assert_eq!(mgr2.midi_mappings().len(), 2);
    }

    #[test]
    fn tags_lenient_load_keeps_valid_entries() {
        let guard = TempDirGuard::new("tags-lenient");
        let mgr = test_manager(&guard.0);
        fs::write(
            mgr.tags_file(),
            r#"{"a": ["Warm", 42, "jazz"], "b": "not-an-array", "nope": ["x"]}"#,
        )
        .unwrap();
        let mgr2 = PresetManager::with_custom_dir("piano", factory_pair(), guard.0.clone());
        assert_eq!(mgr2.tags_for("a"), vec!["Warm", "jazz"]);
        assert!(mgr2.tags_for("b").is_empty());
        assert!(mgr2.tags_for("nope").is_empty());
    }

    #[test]
    fn truncated_favorites_file_backed_up_and_preserved_by_save() {
        let guard = TempDirGuard::new("fav-truncated");
        let raw = b"[\"a\", ";
        fs::write(guard.0.join("favorites.json"), raw).unwrap();
        let backup = guard.0.join("favorites.json.corrupt");
        assert!(!backup.exists());
        let mut mgr = PresetManager::with_custom_dir("piano", factory_pair(), guard.0.clone());
        // Truncated file loads empty and is backed up verbatim.
        assert!(mgr.favorites().is_empty());
        assert_eq!(fs::read(&backup).unwrap(), raw);
        // The next save rewrites favorites.json but never the backup.
        assert!(mgr.toggle_favorite("a").unwrap());
        assert_eq!(fs::read(&backup).unwrap(), raw);
        assert_eq!(
            serde_json::from_str::<Vec<String>>(
                &fs::read_to_string(mgr.favorites_file()).unwrap()
            )
            .unwrap(),
            vec!["a"]
        );
    }

    #[test]
    fn write_sidecar_atomic_round_trip_and_torn_temp_leaves_target_intact() {
        let guard = TempDirGuard::new("atomic");
        let path = guard.0.join("sidecar.json");
        write_sidecar_atomic(&path, b"v1").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"v1");
        // A crashed writer's partial temp file must never corrupt the
        // target: the next atomic write replaces it wholesale.
        let planted = guard.0.join(".sidecar.json.tmp-0-0");
        fs::write(&planted, b"partial{\"").unwrap();
        write_sidecar_atomic(&path, b"v2-complete").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"v2-complete");
        // The planted file is the test's own fixture, not a leftover
        // from the write; remove it before checking for leftovers.
        fs::remove_file(&planted).unwrap();
        // No temp files are left behind by successful writes.
        for entry in fs::read_dir(&guard.0).unwrap().flatten() {
            let name = entry.file_name();
            assert!(
                !name.to_string_lossy().contains(".tmp-"),
                "leftover temp file: {name:?}"
            );
        }
    }

    #[test]
    fn delete_user_preset_removes_favorite_and_persists() {
        let guard = TempDirGuard::new("delete-fav");
        let mut mgr = test_manager(&guard.0);
        let mut params = HashMap::new();
        params.insert("gain".to_string(), 0.5);
        mgr.save_user_preset(Preset::new("u1", "User One", "piano", params))
            .unwrap();
        assert!(mgr.toggle_favorite("u1").unwrap());
        assert!(mgr.delete_user_preset("u1").unwrap());
        // The favorites file itself was rewritten without the ID, so a
        // later preset reusing the ID is not silently starred.
        let ids: Vec<String> =
            serde_json::from_str(&fs::read_to_string(mgr.favorites_file()).unwrap()).unwrap();
        assert!(ids.is_empty());
        let mgr2 = PresetManager::with_custom_dir("piano", factory_pair(), guard.0.clone());
        assert!(mgr2.get_preset("u1").is_none());
        assert!(!mgr2.is_favorite("u1"));
    }

    #[test]
    fn save_user_preset_rejects_empty_id() {
        let guard = TempDirGuard::new("save-empty-id");
        let mut mgr = test_manager(&guard.0);
        let err = mgr
            .save_user_preset(Preset::new("", "Nameless", "piano", HashMap::new()))
            .unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
        assert!(mgr.get_preset("").is_none());
        assert!(!guard.0.join(".json").exists());
    }

    #[test]
    fn save_user_preset_rejects_sanitized_filename_collision() {
        let guard = TempDirGuard::new("save-collision");
        let mut mgr = test_manager(&guard.0);
        mgr.save_user_preset(Preset::new("a_b", "First", "piano", HashMap::new()))
            .unwrap();
        let err = mgr
            .save_user_preset(Preset::new("a b", "Second", "piano", HashMap::new()))
            .unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
        assert!(mgr.get_preset("a b").is_none());
        // The first preset's file was not overwritten.
        let first: Preset = serde_json::from_str(
            &fs::read_to_string(guard.0.join("a_b.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(first.id, "a_b");
        // Re-saving the same ID still works.
        mgr.save_user_preset(Preset::new("a_b", "First Renamed", "piano", HashMap::new()))
            .unwrap();
        assert_eq!(mgr.get_preset("a_b").unwrap().name, "First Renamed");
    }

    #[test]
    fn load_user_presets_skips_empty_id_and_keeps_first_on_filename_collision() {
        let guard = TempDirGuard::new("load-collision");
        let write_preset = |file: &str, id: &str| {
            let preset = Preset::new(id, id, "piano", HashMap::new());
            fs::write(guard.0.join(file), preset.to_json().unwrap()).unwrap();
        };
        write_preset("empty.json", "");
        write_preset("a b.json", "a b");
        write_preset("a_b.json", "a_b");
        write_preset("a-b.json", "a-b");
        let mgr = PresetManager::with_custom_dir("piano", factory_pair(), guard.0.clone());
        assert!(mgr.get_preset("").is_none());
        // Exactly one of the colliding pair survives (first seen wins);
        // the distinct "a-b" preset is always kept.
        assert_eq!(
            mgr.get_preset("a b").is_some() as usize + mgr.get_preset("a_b").is_some() as usize,
            1
        );
        assert!(mgr.get_preset("a-b").is_some());
        assert_eq!(mgr.presets().len(), 4);
    }

    #[test]
    fn default_presets_dir_helper_some_and_none() {
        let some = user_presets_dir_for_data_dir(
            Some(PathBuf::from("/data")),
            "piano",
        );
        assert_eq!(
            some,
            PathBuf::from("/data/physics-instruments/presets/piano")
        );
        let none = user_presets_dir_for_data_dir(None, "piano");
        assert_eq!(
            none,
            std::env::temp_dir()
                .join("physics-instruments-presets")
                .join("piano")
        );
        // Calling with None again returns the same deterministic path.
        assert_eq!(
            user_presets_dir_for_data_dir(None, "guitar"),
            std::env::temp_dir()
                .join("physics-instruments-presets")
                .join("guitar")
        );
    }

    #[test]
    fn tags_case_fold_keeps_first_spelling_across_presets() {
        let guard = TempDirGuard::new("tags-casefold");
        let mut mgr = test_manager(&guard.0);
        mgr.set_tags(
            "a",
            vec!["Warm".to_string(), "warm".to_string(), "JAZZ".to_string()],
        )
        .unwrap();
        assert_eq!(mgr.tags_for("a"), vec!["Warm", "JAZZ"]);
        mgr.set_tags(
            "b",
            vec!["WARM".to_string(), "bright".to_string()],
        )
        .unwrap();
        assert_eq!(mgr.tags_for("b"), vec!["WARM", "bright"]);
        // all_tags folds across presets: b's "WARM" is a's "Warm".
        assert_eq!(mgr.all_tags(), vec!["Warm", "JAZZ", "bright"]);
    }

    #[test]
    fn prune_stale_midi_mappings_removes_and_persists_only_when_changed() {
        let guard = TempDirGuard::new("prune");
        let mut mgr = test_manager(&guard.0);
        mgr.bind_midi_cc(7, "tone").unwrap();
        mgr.bind_midi_cc(8, "gone-param").unwrap();
        let valid: std::collections::HashSet<String> =
            std::collections::HashSet::from(["tone".to_string()]);
        assert!(mgr.prune_stale_midi_mappings(&valid));
        assert_eq!(mgr.midi_mapping(7), Some("tone"));
        assert_eq!(mgr.midi_mapping(8), None);
        let mgr2 = PresetManager::with_custom_dir("piano", factory_pair(), guard.0.clone());
        assert_eq!(mgr2.midi_mapping(8), None);
        // Nothing left to prune: no rewrite.
        let before = fs::read_to_string(mgr2.midi_mappings_file()).unwrap();
        let mut mgr3 = mgr2;
        assert!(!mgr3.prune_stale_midi_mappings(&valid));
        assert_eq!(
            fs::read_to_string(mgr3.midi_mappings_file()).unwrap(),
            before
        );
    }
}
