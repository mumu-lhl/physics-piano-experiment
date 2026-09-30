//! English and Simplified Chinese strings for the drum editor.

pub use physics_ui::Language;

pub struct I18n;

impl I18n {
    pub fn title(lang: Language) -> &'static str {
        match lang {
            Language::English => "PHYSICS DRUM",
            Language::SimplifiedChinese => "物理建模架子鼓",
        }
    }

    pub fn subtitle(lang: Language) -> &'static str {
        match lang {
            Language::English => {
                "Bessel membrane modes · Hunt-Crossley impact · coupled heads · nonlinear cymbals"
            }
            Language::SimplifiedChinese => {
                "Bessel 膜模态 · Hunt-Crossley 冲击 · 双膜耦合 · 非线性镲片"
            }
        }
    }

    pub fn audition(lang: Language) -> &'static str {
        match lang {
            Language::English => "Click the pads below to audition",
            Language::SimplifiedChinese => "点击下方鼓垫即可试听",
        }
    }

    pub fn preset(lang: Language) -> &'static str {
        match lang {
            Language::English => "Preset:",
            Language::SimplifiedChinese => "音色预设:",
        }
    }

    pub fn snare_membrane(lang: Language) -> &'static str {
        match lang {
            Language::English => "SNARE / MEMBRANE",
            Language::SimplifiedChinese => "军鼓 / 鼓膜",
        }
    }

    pub fn snare_tightness(lang: Language) -> &'static str {
        match lang {
            Language::English => "Snare tightness",
            Language::SimplifiedChinese => "响弦松紧",
        }
    }

    pub fn snare_decay(lang: Language) -> &'static str {
        match lang {
            Language::English => "Snare decay",
            Language::SimplifiedChinese => "军鼓衰减",
        }
    }

    pub fn snare_note(lang: Language) -> &'static str {
        match lang {
            Language::English => "Bottom head drives 20 bounded wire contacts",
            Language::SimplifiedChinese => "底膜驱动 20 个有界响弦接触状态",
        }
    }

    pub fn cymbal_output(lang: Language) -> &'static str {
        match lang {
            Language::English => "CYMBAL / OUTPUT",
            Language::SimplifiedChinese => "镲片 / 输出",
        }
    }

    pub fn hihat_open(lang: Language) -> &'static str {
        match lang {
            Language::English => "Hi-hat open",
            Language::SimplifiedChinese => "踩镲开度",
        }
    }

    pub fn cymbal_decay(lang: Language) -> &'static str {
        match lang {
            Language::English => "Cymbal decay",
            Language::SimplifiedChinese => "镲片衰减",
        }
    }

    pub fn master_gain(lang: Language) -> &'static str {
        match lang {
            Language::English => "Master gain",
            Language::SimplifiedChinese => "总输出增益",
        }
    }

    pub fn hint(lang: Language) -> &'static str {
        match lang {
            Language::English => {
                "Closed hat chokes open hat · CC4 controls pedal opening · note-off preserves physical tails"
            }
            Language::SimplifiedChinese => {
                "闭镲会切断开镲 · CC4 控制踏板开度 · Note-Off 保留物理衰减尾音"
            }
        }
    }
}
