/// Languages supported by the instrument editors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    English,
    SimplifiedChinese,
}

impl Language {
    pub fn from_system_locale() -> Self {
        let is_chinese = std::env::var("LANG")
            .map(|locale| locale.contains("zh"))
            .unwrap_or(false)
            || std::env::var("LC_ALL")
                .map(|locale| locale.contains("zh"))
                .unwrap_or(false);

        if is_chinese {
            Self::SimplifiedChinese
        } else {
            Self::English
        }
    }
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
