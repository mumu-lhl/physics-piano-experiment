/// Locale selected by the instrument editor.
///
/// Values are indexes into the locale table generated from `locales/*.po`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Language(u8);

#[allow(non_upper_case_globals)]
impl Language {
    /// English source locale, always stored at index zero.
    pub const English: Self = Self(0);
    /// Existing Simplified Chinese locale, kept as a convenience for callers.
    pub const SimplifiedChinese: Self = Self(1);

    pub fn from_index(index: u8) -> Self {
        if (index as usize) < locale_data::LANGUAGE_CODES.len() {
            Self(index)
        } else {
            Self::English
        }
    }

    pub const fn index(self) -> u8 {
        self.0
    }

    pub fn code(self) -> &'static str {
        locale_data::LANGUAGE_CODES
            .get(self.0 as usize)
            .copied()
            .unwrap_or("en")
    }

    pub fn native_name(self) -> &'static str {
        let name = translate(self, "meta.language-name", "Language name");
        if name == "Language name" {
            self.code()
        } else {
            name
        }
    }

    pub fn available() -> impl Iterator<Item = Self> {
        (0..locale_data::LANGUAGE_CODES.len()).map(|index| Self(index as u8))
    }

    pub fn from_code(locale: &str) -> Option<Self> {
        let normalized = normalize_locale(locale);
        locale_data::LANGUAGE_CODES
            .iter()
            .position(|code| normalize_locale(code) == normalized)
            .or_else(|| {
                let base = normalized.split('_').next()?;
                locale_data::LANGUAGE_CODES
                    .iter()
                    .position(|code| normalize_locale(code).split('_').next() == Some(base))
            })
            .map(|index| Self(index as u8))
    }

    pub fn from_system_locale() -> Self {
        ["LC_ALL", "LC_MESSAGES", "LANG"]
            .into_iter()
            .filter_map(|key| std::env::var(key).ok())
            .find(|locale| !locale.is_empty())
            .and_then(|locale| Self::from_code(&locale))
            .unwrap_or(Self::English)
    }

    /// Resolves a persisted language index (see `ViziaState::language_index`).
    /// `u8::MAX` is the sentinel for "system locale". Any other out-of-range
    /// index also resolves to the system locale (unlike [`Self::from_index`],
    /// which falls back to English): a persisted index can only be
    /// out-of-range if the locale table shrank since the state was saved,
    /// and the system locale is the least surprising fallback there.
    pub fn resolve_persisted(index: u8) -> Self {
        if index == u8::MAX || (index as usize) >= locale_data::LANGUAGE_CODES.len() {
            Self::from_system_locale()
        } else {
            Self(index)
        }
    }
}

fn normalize_locale(locale: &str) -> String {
    let locale = locale.split(['.', '@']).next().unwrap_or(locale);
    locale.replace('-', "_").to_ascii_lowercase()
}

/// Looks up a gettext message by context, returning the English source on a miss.
pub fn translate<'a>(language: Language, context: &str, message: &'a str) -> &'a str {
    let key = (language.code(), context, message);
    locale_data::TRANSLATIONS
        .binary_search_by(|entry| (entry.0, entry.1, entry.2).cmp(&key))
        .ok()
        .map(|index| locale_data::TRANSLATIONS[index].3)
        .unwrap_or(message)
}

/// Formats named `{placeholder}` values in a translated message.
pub fn translate_format(
    language: Language,
    context: &str,
    message: &'static str,
    values: &[(&str, &str)],
) -> String {
    let mut translated = translate(language, context, message).to_owned();
    for (name, value) in values {
        translated = translated.replace(&format!("{{{name}}}"), value);
    }
    translated
}

mod locale_data {
    include!(concat!(env!("OUT_DIR"), "/locales.rs"));
}

/// CJK fallback font bytes, loaded at most once per process. Vizia's
/// `add_font_mem` requires `&'static [u8]`, so the bytes are leaked once
/// (unavoidable) and the leaked slice is reused on every later editor open
/// instead of leaking a fresh copy per open.
static CACHED_FONT_BYTES: std::sync::OnceLock<Option<&'static [u8]>> =
    std::sync::OnceLock::new();

/// First available CJK fallback font file, read once and cached (including
/// the "no font found" outcome, so later opens don't re-scan the disk).
fn cached_font_bytes() -> Option<&'static [u8]> {
    *CACHED_FONT_BYTES.get_or_init(|| {
        const CANDIDATE_PATHS: &[&str] = &[
            "/usr/share/fonts/google-droid-sans-fonts/DroidSansFallbackFull.ttf",
            "/usr/share/fonts/google-noto-sans-cjk-fonts/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
            "/usr/share/fonts/wqy-microhei/wqy-microhei.ttc",
            "C:\\Windows\\Fonts\\msyh.ttc",
            "C:\\Windows\\Fonts\\simsun.ttc",
            "C:\\Windows\\Fonts\\simhei.ttf",
            "/System/Library/Fonts/PingFang.ttc",
            "/Library/Fonts/Songti.ttc",
        ];

        for path in CANDIDATE_PATHS {
            if let Ok(bytes) = std::fs::read(path) {
                return Some(Box::leak(bytes.into_boxed_slice()));
            }
        }
        None
    })
}

/// Loads the first available CJK fallback font into a Vizia context.
pub fn setup_vizia_fonts(cx: &mut vizia_plug::vizia::prelude::Context) {
    if let Some(static_bytes) = cached_font_bytes() {
        cx.add_font_mem(static_bytes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_persisted_sentinel_means_system_locale() {
        assert_eq!(
            Language::resolve_persisted(u8::MAX),
            Language::from_system_locale()
        );
    }

    #[test]
    fn resolve_persisted_known_indexes_resolve_verbatim() {
        assert_eq!(Language::resolve_persisted(0), Language::English);
        assert_eq!(
            Language::resolve_persisted(1),
            Language::SimplifiedChinese
        );
    }

    #[test]
    fn resolve_persisted_out_of_range_means_system_locale() {
        // Well beyond any plausible locale-table size: must not snap to
        // English (index 0) the way `from_index` does for bad indexes.
        assert_eq!(
            Language::resolve_persisted(200),
            Language::from_system_locale()
        );
    }

    #[test]
    fn font_bytes_are_loaded_once_and_reused() {
        let first = cached_font_bytes();
        let second = cached_font_bytes();
        assert_eq!(first.map(|b| b.as_ptr()), second.map(|b| b.as_ptr()));
        assert_eq!(first.map(|b| b.len()), second.map(|b| b.len()));
    }
}
