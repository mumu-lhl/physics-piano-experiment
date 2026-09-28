//! Preset data structure and serialization.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A physical instrument preset defining voicing, physics, acoustic, and stage parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Preset {
    /// Machine-readable identifier
    pub id: String,

    /// English display name
    pub name: String,

    /// Optional Simplified Chinese display name
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name_zh: Option<String>,

    /// Preset creator / sound designer
    #[serde(default = "default_author")]
    pub author: String,

    /// Target instrument ("piano" or "guitar")
    pub instrument: String,

    /// Category / genre (e.g., "Concert Grand", "Acoustic Fingerstyle", "High Gain Lead")
    #[serde(default = "default_category")]
    pub category: String,

    /// English description of acoustic characteristics
    #[serde(default)]
    pub description: String,

    /// Optional Simplified Chinese description
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description_zh: Option<String>,

    /// Parameter mapping: parameter ID -> floating-point value
    pub params: HashMap<String, f32>,

    /// Whether this preset is a protected factory preset
    #[serde(default)]
    pub is_factory: bool,
}

fn default_author() -> String {
    "Physics Soundlab".to_string()
}

fn default_category() -> String {
    "General".to_string()
}

impl Preset {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        instrument: impl Into<String>,
        params: HashMap<String, f32>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            name_zh: None,
            author: default_author(),
            instrument: instrument.into(),
            category: default_category(),
            description: String::new(),
            description_zh: None,
            params,
            is_factory: false,
        }
    }

    /// Display name according to locale (true = Chinese, false = English)
    pub fn display_name(&self, is_chinese: bool) -> &str {
        if is_chinese {
            if let Some(ref zh) = self.name_zh {
                return zh.as_str();
            }
        }
        &self.name
    }

    /// Description according to locale
    pub fn display_description(&self, is_chinese: bool) -> &str {
        if is_chinese {
            if let Some(ref zh) = self.description_zh {
                return zh.as_str();
            }
        }
        &self.description
    }

    /// Serialize preset to formatted JSON string
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Deserialize preset from JSON string
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}
