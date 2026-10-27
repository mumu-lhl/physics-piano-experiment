//! Preset manager handling factory presets, user preset disk persistence,
//! and import/export functionality.

use std::fs;
use std::path::{Path, PathBuf};

use crate::preset::Preset;

/// Controller CC numbers that MIDI Learn must never bind: they already have
/// a fixed meaning (piano sustain / una corda / expression, drum hi-hat
/// pedal) and stealing them would break playing.
pub const MIDI_LEARN_EXCLUDED_CCS: [u8; 4] = [64, 67, 11, 4];

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
        };

        // Attempt to load existing user presets from disk
        let _ = mgr.load_user_presets();
        mgr.load_favorites();
        mgr.load_midi_mappings();
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
        };
        let _ = mgr.load_user_presets();
        mgr.load_favorites();
        mgr.load_midi_mappings();
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

    /// Load starred IDs from disk, silently keeping only IDs that still exist.
    /// A missing or malformed file simply means "no favorites yet".
    pub fn load_favorites(&mut self) {
        let Ok(content) = fs::read_to_string(self.favorites_file()) else {
            return;
        };
        let Ok(ids) = serde_json::from_str::<Vec<String>>(&content) else {
            return;
        };
        self.favorites = ids
            .into_iter()
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
        fs::write(self.favorites_file(), json)
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

    /// Load MIDI Learn bindings from disk, silently keeping only bindings
    /// whose CC is learnable. A missing or malformed file simply means
    /// "no mappings yet".
    pub fn load_midi_mappings(&mut self) {
        let Ok(content) = fs::read_to_string(self.midi_mappings_file()) else {
            return;
        };
        let Ok(map) = serde_json::from_str::<std::collections::HashMap<u8, String>>(&content)
        else {
            return;
        };
        self.midi_mappings = map
            .into_iter()
            .filter(|(cc, _)| !MIDI_LEARN_EXCLUDED_CCS.contains(cc))
            .collect();
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
        fs::write(self.midi_mappings_file(), json)
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
                        // Prevent ID collision with factory presets
                        if !self.presets.iter().any(|p| p.id == preset.id) {
                            self.presets.push(preset);
                            count += 1;
                        }
                    }
                }
            }
        }

        Ok(count)
    }

    /// Save a user preset to disk
    pub fn save_user_preset(&mut self, mut preset: Preset) -> Result<(), std::io::Error> {
        preset.is_factory = false;
        preset.instrument = self.instrument.clone();

        if !self.user_presets_dir.exists() {
            fs::create_dir_all(&self.user_presets_dir)?;
        }

        // Sanitize filename
        let safe_filename: String = preset
            .id
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '_' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let path = self.user_presets_dir.join(format!("{safe_filename}.json"));

        let json = preset
            .to_json()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        fs::write(path, json)?;

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

        let safe_filename: String = id
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '_' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let path = self.user_presets_dir.join(format!("{safe_filename}.json"));
        if path.exists() {
            fs::remove_file(path)?;
        }

        self.presets.retain(|p| p.id != id || p.is_factory);
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
        fs::write(path, json)?;
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
fn default_user_presets_dir(instrument: &str) -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("physics-instruments")
        .join("presets")
        .join(instrument)
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
}
