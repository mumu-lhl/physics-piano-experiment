//! Internationalization (i18n) for Physics Piano GUI supporting English and Simplified Chinese.

pub use physics_ui::Language;

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
            Language::English => {
                "Virtual 88-Key Keyboard (A0-C8) | Play via mouse or QWERTY keys (A-K / W,E,T,Y,U), Z/X to shift octave"
            }
            Language::SimplifiedChinese => {
                "88键虚拟物理键盘 (A0-C8) | 支持鼠标点击或电脑键盘弹奏 (A-K / W,E,T,Y,U)，按 Z/X 切换八度"
            }
        }
    }
}

pub use physics_ui::setup_vizia_fonts;
