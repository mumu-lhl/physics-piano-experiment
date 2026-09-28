//! Preset manager handling factory presets, user preset disk persistence,
//! and import/export functionality.

use std::fs;
use std::path::{Path, PathBuf};

use crate::preset::Preset;

#[derive(Debug, Clone)]
pub struct PresetManager {
    instrument: String,
    presets: Vec<Preset>,
    user_presets_dir: PathBuf,
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
        };

        // Attempt to load existing user presets from disk
        let _ = mgr.load_user_presets();
        mgr
    }

    /// With custom user presets directory (useful for testing or customized configs)
    pub fn with_custom_dir(instrument: &str, mut factory_presets: Vec<Preset>, dir: PathBuf) -> Self {
        for p in &mut factory_presets {
            p.is_factory = true;
            p.instrument = instrument.to_string();
        }

        let mut mgr = Self {
            instrument: instrument.to_string(),
            presets: factory_presets,
            user_presets_dir: dir,
        };
        let _ = mgr.load_user_presets();
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
            .map(|c| if c.is_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
            .collect();
        let path = self.user_presets_dir.join(format!("{safe_filename}.json"));

        let json = preset
            .to_json()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        fs::write(path, json)?;

        // Update in-memory list
        if let Some(pos) = self.presets.iter().position(|p| p.id == preset.id && !p.is_factory) {
            self.presets[pos] = preset;
        } else {
            self.presets.push(preset);
        }

        Ok(())
    }

    /// Delete a user preset from disk and memory
    pub fn delete_user_preset(&mut self, id: &str) -> Result<bool, std::io::Error> {
        let is_user = self.presets.iter().any(|p| p.id == id && !p.is_factory);
        if !is_user {
            return Ok(false);
        }

        let safe_filename: String = id
            .chars()
            .map(|c| if c.is_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
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
