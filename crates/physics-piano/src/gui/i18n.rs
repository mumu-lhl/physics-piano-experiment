//! Internationalization (i18n) for Physics Piano GUI supporting English and Simplified Chinese.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    English,
    SimplifiedChinese,
}

impl Language {
    pub fn from_system_locale() -> Self {
        let is_chinese = std::env::var("LANG")
            .map(|l| l.contains("zh"))
            .unwrap_or(false)
            || std::env::var("LC_ALL")
                .map(|l| l.contains("zh"))
                .unwrap_or(false);

        if is_chinese {
            Language::SimplifiedChinese
        } else {
            Language::English
        }
    }
}

pub struct I18n;

impl I18n {
    pub fn title(lang: Language) -> &'static str {
        match lang {
            Language::English => "PHYSICS PIANO",
            Language::SimplifiedChinese => "物理建模钢琴",
        }
    }

    pub fn subtitle(lang: Language) -> &'static str {
        match lang {
            Language::English => "Acoustic Grand Physical Modeling Synthesizer (Rust Engine)",
            Language::SimplifiedChinese => "三角钢琴原生物理声学合成器 (Rust Engine)",
        }
    }

    pub fn rack_pedals(lang: Language) -> &'static str {
        match lang {
            Language::English => "PEDALS & MECHANICS",
            Language::SimplifiedChinese => "踏板与微观机械",
        }
    }

    pub fn sustain(lang: Language) -> &'static str {
        match lang {
            Language::English => "Sustain:",
            Language::SimplifiedChinese => "延音踏板:",
        }
    }

    pub fn una_corda(lang: Language) -> &'static str {
        match lang {
            Language::English => "Una Corda",
            Language::SimplifiedChinese => "柔音踏板",
        }
    }

    pub fn key_action(lang: Language) -> &'static str {
        match lang {
            Language::English => "Key Action:",
            Language::SimplifiedChinese => "琴键撞击:",
        }
    }

    pub fn damper_noise(lang: Language) -> &'static str {
        match lang {
            Language::English => "Damper Noise:",
            Language::SimplifiedChinese => "制音器摩擦:",
        }
    }

    pub fn pedal_shock(lang: Language) -> &'static str {
        match lang {
            Language::English => "Pedal Shock:",
            Language::SimplifiedChinese => "铸铁板冲击:",
        }
    }

    pub fn rack_string(lang: Language) -> &'static str {
        match lang {
            Language::English => "STRING & HAMMER",
            Language::SimplifiedChinese => "琴弦与琴槌物理",
        }
    }

    pub fn inharmonicity(lang: Language) -> &'static str {
        match lang {
            Language::English => "Inharmonicity B:",
            Language::SimplifiedChinese => "非谐波度 B:",
        }
    }

    pub fn hammer_hardness(lang: Language) -> &'static str {
        match lang {
            Language::English => "Hammer Hardness:",
            Language::SimplifiedChinese => "琴槌硬度:",
        }
    }

    pub fn unison_detune(lang: Language) -> &'static str {
        match lang {
            Language::English => "Unison Detune:",
            Language::SimplifiedChinese => "同音弦微调:",
        }
    }

    pub fn phantom_partials(lang: Language) -> &'static str {
        match lang {
            Language::English => "Phantom Partials:",
            Language::SimplifiedChinese => "幻象泛音:",
        }
    }

    pub fn rack_spatial(lang: Language) -> &'static str {
        match lang {
            Language::English => "SPATIAL MICS & LID",
            Language::SimplifiedChinese => "多拾音麦位与琴盖",
        }
    }

    pub fn mic_close(lang: Language) -> &'static str {
        match lang {
            Language::English => "Close Mic:",
            Language::SimplifiedChinese => "近场麦位:",
        }
    }

    pub fn mic_player(lang: Language) -> &'static str {
        match lang {
            Language::English => "Player Mic:",
            Language::SimplifiedChinese => "演奏者麦位:",
        }
    }

    pub fn mic_ambient(lang: Language) -> &'static str {
        match lang {
            Language::English => "Ambient Mic:",
            Language::SimplifiedChinese => "环境空间麦:",
        }
    }

    pub fn lid_angle(lang: Language) -> &'static str {
        match lang {
            Language::English => "Lid Angle:",
            Language::SimplifiedChinese => "琴盖角度:",
        }
    }

    pub fn rack_output(lang: Language) -> &'static str {
        match lang {
            Language::English => "OUTPUT",
            Language::SimplifiedChinese => "总输出",
        }
    }

    pub fn master_gain(lang: Language) -> &'static str {
        match lang {
            Language::English => "Master Gain:",
            Language::SimplifiedChinese => "主输出增益:",
        }
    }

    pub fn velocity_curve(lang: Language) -> &'static str {
        match lang {
            Language::English => "Touch Curve:",
            Language::SimplifiedChinese => "触键力度曲线:",
        }
    }

    pub fn preset(lang: Language) -> &'static str {
        match lang {
            Language::English => "Preset:",
            Language::SimplifiedChinese => "音色预设:",
        }
    }

    pub fn keyboard_hint(lang: Language) -> &'static str {
        match lang {
            Language::English => "Virtual 88-Key Keyboard (A0-C8) | Play via mouse or QWERTY keys (Z-M / Q-U)",
            Language::SimplifiedChinese => "88键虚拟物理键盘 (A0-C8) | 支持鼠标点击或电脑键盘弹奏 (Z-M / Q-U)",
        }
    }
}

/// Discovers available system CJK fonts and installs fallback font definitions into Vizia context.
pub fn setup_vizia_fonts(cx: &mut nih_plug_vizia::vizia::prelude::Context) {
    nih_plug_vizia::assets::register_noto_sans_regular(cx);
    nih_plug_vizia::assets::register_noto_sans_bold(cx);
    nih_plug_vizia::assets::register_noto_sans_light(cx);

    // Priority list of common Chinese fonts on Linux, Windows, macOS
    let candidate_paths = [
        // Linux system fonts
        "/usr/share/fonts/google-droid-sans-fonts/DroidSansFallbackFull.ttf",
        "/usr/share/fonts/google-noto-sans-cjk-fonts/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
        "/usr/share/fonts/wqy-microhei/wqy-microhei.ttc",
        "/home/mumulhl/.local/share/fonts/LXGWWenKai-Regular.ttf",
        // Windows system fonts
        "C:\\Windows\\Fonts\\msyh.ttc",
        "C:\\Windows\\Fonts\\simsun.ttc",
        "C:\\Windows\\Fonts\\simhei.ttf",
        // macOS system fonts
        "/System/Library/Fonts/PingFang.ttc",
        "/Library/Fonts/Songti.ttc",
    ];

    for &path in &candidate_paths {
        if let Ok(bytes) = std::fs::read(path) {
            let static_bytes: &'static [u8] = Box::leak(bytes.into_boxed_slice());
            cx.add_font_mem(static_bytes);
            break;
        }
    }
}
