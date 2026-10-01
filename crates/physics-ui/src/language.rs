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

/// Loads the first available CJK fallback font into a Vizia context.
pub fn setup_vizia_fonts(cx: &mut vizia_plug::vizia::prelude::Context) {
    const CANDIDATE_PATHS: &[&str] = &[
        "/usr/share/fonts/google-droid-sans-fonts/DroidSansFallbackFull.ttf",
        "/usr/share/fonts/google-noto-sans-cjk-fonts/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
        "/usr/share/fonts/wqy-microhei/wqy-microhei.ttc",
        "/home/mumulhl/.local/share/fonts/LXGWWenKai-Regular.ttf",
        "C:\\Windows\\Fonts\\msyh.ttc",
        "C:\\Windows\\Fonts\\simsun.ttc",
        "C:\\Windows\\Fonts\\simhei.ttf",
        "/System/Library/Fonts/PingFang.ttc",
        "/Library/Fonts/Songti.ttc",
    ];

    for path in CANDIDATE_PATHS {
        if let Ok(bytes) = std::fs::read(path) {
            let static_bytes: &'static [u8] = Box::leak(bytes.into_boxed_slice());
            cx.add_font_mem(static_bytes);
            break;
        }
    }
}
