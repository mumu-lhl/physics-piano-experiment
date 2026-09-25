//! Internationalization (i18n) for Physics Guitar GUI supporting English and Simplified Chinese.

use nih_plug_egui::egui::{self, FontDefinitions, FontFamily, FontData};
use std::sync::Arc;

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
            Language::English => "PHYSICS GUITAR",
            Language::SimplifiedChinese => "物理建模吉他",
        }
    }

    pub fn subtitle(lang: Language) -> &'static str {
        match lang {
            Language::English => "FIRST-PRINCIPLES PHYSICAL MODELING VIRTUAL INSTRUMENT",
            Language::SimplifiedChinese => "第一性原理物理声学建模虚拟吉他乐器",
        }
    }

    pub fn rack_instrument(lang: Language) -> &'static str {
        match lang {
            Language::English => "INSTRUMENT",
            Language::SimplifiedChinese => "乐器模式",
        }
    }

    pub fn mode_electric(lang: Language) -> &'static str {
        match lang {
            Language::English => "Electric Guitar",
            Language::SimplifiedChinese => "电吉他",
        }
    }

    pub fn mode_acoustic(lang: Language) -> &'static str {
        match lang {
            Language::English => "Acoustic Guitar",
            Language::SimplifiedChinese => "原声木吉他",
        }
    }

    pub fn rack_pickup(lang: Language) -> &'static str {
        match lang {
            Language::English => "PICKUP SELECTOR",
            Language::SimplifiedChinese => "拾音器选择",
        }
    }

    pub fn pickup_pos(lang: Language) -> &'static str {
        match lang {
            Language::English => "Position:",
            Language::SimplifiedChinese => "拾音位置:",
        }
    }

    pub fn pickup_bridge(lang: Language) -> &'static str {
        match lang {
            Language::English => "Bridge",
            Language::SimplifiedChinese => "琴桥",
        }
    }

    pub fn pickup_mid(lang: Language) -> &'static str {
        match lang {
            Language::English => "Mid",
            Language::SimplifiedChinese => "中间",
        }
    }

    pub fn pickup_neck(lang: Language) -> &'static str {
        match lang {
            Language::English => "Neck",
            Language::SimplifiedChinese => "琴颈",
        }
    }

    pub fn pickup_type(lang: Language) -> &'static str {
        match lang {
            Language::English => "Type:",
            Language::SimplifiedChinese => "拾音结构:",
        }
    }

    pub fn pickup_single(lang: Language) -> &'static str {
        match lang {
            Language::English => "Single",
            Language::SimplifiedChinese => "单线圈",
        }
    }

    pub fn pickup_humbucker(lang: Language) -> &'static str {
        match lang {
            Language::English => "Humbucker",
            Language::SimplifiedChinese => "双线圈",
        }
    }

    pub fn rack_pluck(lang: Language) -> &'static str {
        match lang {
            Language::English => "PLUCK & TONE",
            Language::SimplifiedChinese => "拨弦与音色",
        }
    }

    pub fn pluck_style(lang: Language) -> &'static str {
        match lang {
            Language::English => "Pluck Style:",
            Language::SimplifiedChinese => "拨弦方式:",
        }
    }

    pub fn pluck_plectrum(lang: Language) -> &'static str {
        match lang {
            Language::English => "Plectrum",
            Language::SimplifiedChinese => "拨片",
        }
    }

    pub fn pluck_finger(lang: Language) -> &'static str {
        match lang {
            Language::English => "Finger",
            Language::SimplifiedChinese => "指弹",
        }
    }

    pub fn palm_mute(lang: Language) -> &'static str {
        match lang {
            Language::English => "Palm Mute:",
            Language::SimplifiedChinese => "手掌弱音 (制音):",
        }
    }

    pub fn rack_output(lang: Language) -> &'static str {
        match lang {
            Language::English => "MASTER OUTPUT",
            Language::SimplifiedChinese => "总输出",
        }
    }

    pub fn master_gain(lang: Language) -> &'static str {
        match lang {
            Language::English => "Master Gain:",
            Language::SimplifiedChinese => "主输出增益:",
        }
    }

    pub fn pluck_pos(lang: Language) -> &'static str {
        match lang {
            Language::English => "Pluck Position:",
            Language::SimplifiedChinese => "拨弦位置:",
        }
    }

    pub fn pickup_bn(lang: Language) -> &'static str {
        match lang {
            Language::English => "B+N",
            Language::SimplifiedChinese => "桥+颈",
        }
    }

    pub fn pickup_bm(lang: Language) -> &'static str {
        match lang {
            Language::English => "B+M",
            Language::SimplifiedChinese => "桥+中",
        }
    }

    pub fn tone_knob(lang: Language) -> &'static str {
        match lang {
            Language::English => "Passive Tone:",
            Language::SimplifiedChinese => "被动音色旋钮:",
        }
    }

    pub fn rack_amp(lang: Language) -> &'static str {
        match lang {
            Language::English => "AMP & CABINET",
            Language::SimplifiedChinese => "电子管音箱与箱体",
        }
    }

    pub fn amp_drive(lang: Language) -> &'static str {
        match lang {
            Language::English => "12AX7 Tube Drive:",
            Language::SimplifiedChinese => "12AX7 前级增益:",
        }
    }

    pub fn cab_enabled(lang: Language) -> &'static str {
        match lang {
            Language::English => "12\" Celestion Cab",
            Language::SimplifiedChinese => "12寸箱体模拟",
        }
    }

    pub fn rack_strum(lang: Language) -> &'static str {
        match lang {
            Language::English => "STRUM & ACTION",
            Language::SimplifiedChinese => "智能扫弦与手感",
        }
    }

    pub fn strum_speed(lang: Language) -> &'static str {
        match lang {
            Language::English => "Strum Speed:",
            Language::SimplifiedChinese => "扫弦时差 (毫秒):",
        }
    }

    pub fn fret_buzz(lang: Language) -> &'static str {
        match lang {
            Language::English => "Fret Clatter / Buzz:",
            Language::SimplifiedChinese => "品丝碰撞打品度:",
        }
    }

    pub fn fretboard_hint(lang: Language) -> &'static str {
        match lang {
            Language::English => "Interactive 6-String Fretboard (Click or Drag frets to play):",
            Language::SimplifiedChinese => "6弦交互式物理指板 (鼠标点击或拖拽品位弹奏):",
        }
    }
}

/// Discovers available system CJK fonts and installs fallback font definitions into egui context.
pub fn setup_cjk_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();

    // Priority list of common Chinese fonts on Linux, Windows, macOS
    let candidate_paths = [
        // Linux system fonts
        "/usr/share/fonts/google-noto-sans-cjk-vf-fonts/NotoSansCJK-VF.ttc",
        "/usr/share/fonts/google-noto-sans-cjk-fonts/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/google-noto-sans-cjk-fonts/NotoSansCJK-Medium.ttc",
        "/usr/share/fonts/google-droid-sans-fonts/DroidSansFallbackFull.ttf",
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

    let mut loaded = false;
    for &path in &candidate_paths {
        if let Ok(bytes) = std::fs::read(path) {
            fonts.font_data.insert(
                "cjk_fallback".to_owned(),
                Arc::new(FontData::from_owned(bytes)),
            );
            fonts.families
                .entry(FontFamily::Proportional)
                .or_default()
                .push("cjk_fallback".to_owned());
            fonts.families
                .entry(FontFamily::Monospace)
                .or_default()
                .push("cjk_fallback".to_owned());
            loaded = true;
            break;
        }
    }

    if loaded {
        ctx.set_fonts(fonts);
    }
}
