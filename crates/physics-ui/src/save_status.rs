//! Short, localized status messages for preset/favorite/tag/device save
//! failures (P2-28).
//!
//! The editors keep the `eprintln!` diagnostic (with the OS error) as the
//! detailed record, and show one of these short messages on the shared
//! save-status line next to the Learn status. Success clears the line.
//! The messages deliberately carry no OS error text: the line is short
//! and ellipsized, and the raw error is already in the log.

use crate::{Language, translate};

/// Which save operation failed. One short message per kind, so the status
/// line names the operation the user just performed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveFailureKind {
    /// Starring/unstarring a preset (`toggle_favorite`).
    Favorite,
    /// Editing a preset's tags (`set_tags`).
    Tags,
    /// Saving or overwriting a user preset (`save_user_preset` /
    /// `overwrite_user_preset`).
    Preset,
    /// Saving the standalone device selections (`DeviceSettings::save_to_dir`).
    Device,
    /// Spawning the relaunched standalone (`spawn_restart`).
    Restart,
}

/// Localized short message for a failed save of `kind`. Each
/// `translate()` call uses literal context/default strings so the i18n
/// key scanner (`i18n_keys.rs`) can extract it; the matching entries
/// must exist in both `locales/*.po` files.
pub fn save_failure_message(language: Language, kind: SaveFailureKind) -> String {
    match kind {
        SaveFailureKind::Favorite => {
            translate(language, "gui.save_failed_favorite", "Failed to save favorite").to_string()
        }
        SaveFailureKind::Tags => {
            translate(language, "gui.save_failed_tags", "Failed to save tags").to_string()
        }
        SaveFailureKind::Preset => {
            translate(language, "gui.save_failed_preset", "Failed to save preset").to_string()
        }
        SaveFailureKind::Device => translate(
            language,
            "gui.save_failed_device",
            "Failed to save device settings",
        )
        .to_string(),
        SaveFailureKind::Restart => {
            translate(language, "gui.save_failed_restart", "Failed to restart").to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_are_distinct_and_non_empty_in_english() {
        let kinds = [
            SaveFailureKind::Favorite,
            SaveFailureKind::Tags,
            SaveFailureKind::Preset,
            SaveFailureKind::Device,
            SaveFailureKind::Restart,
        ];
        let mut seen = Vec::new();
        for kind in kinds {
            let message = save_failure_message(Language::English, kind);
            assert!(!message.is_empty(), "{kind:?} message is empty");
            assert!(!seen.contains(&message), "{kind:?} duplicates a message");
            seen.push(message);
        }
        assert_eq!(
            save_failure_message(Language::English, SaveFailureKind::Favorite),
            "Failed to save favorite"
        );
        assert_eq!(
            save_failure_message(Language::English, SaveFailureKind::Device),
            "Failed to save device settings"
        );
    }

    #[test]
    fn messages_are_translated_in_chinese() {
        // A missing .po entry would fall back to the English default, so
        // these assertions prove the Chinese entries are embedded.
        assert_eq!(
            save_failure_message(Language::SimplifiedChinese, SaveFailureKind::Favorite),
            "保存收藏失败"
        );
        assert_eq!(
            save_failure_message(Language::SimplifiedChinese, SaveFailureKind::Tags),
            "保存标签失败"
        );
        assert_eq!(
            save_failure_message(Language::SimplifiedChinese, SaveFailureKind::Preset),
            "保存预设失败"
        );
        assert_eq!(
            save_failure_message(Language::SimplifiedChinese, SaveFailureKind::Device),
            "保存设备设置失败"
        );
        assert_eq!(
            save_failure_message(Language::SimplifiedChinese, SaveFailureKind::Restart),
            "重启失败"
        );
    }
}
