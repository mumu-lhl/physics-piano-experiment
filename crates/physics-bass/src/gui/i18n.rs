//! English and Simplified Chinese strings for the bass editor.

pub use physics_ui::Language;

pub struct I18n;

impl I18n {
    pub fn title(lang: Language) -> &'static str {
        match lang {
            Language::English => "PHYSICS BASS",
            Language::SimplifiedChinese => "物理建模贝斯",
        }
    }

    pub fn subtitle(lang: Language) -> &'static str {
        match lang {
            Language::English => {
                "FDTD stiff string · fret contact · finite pickup aperture · body modes"
            }
            Language::SimplifiedChinese => "FDTD 刚性弦 · 品丝接触 · 有限拾音孔径 · 琴体模态",
        }
    }

    pub fn preset(lang: Language) -> &'static str {
        match lang {
            Language::English => "Preset:",
            Language::SimplifiedChinese => "音色预设:",
        }
    }

    pub fn audition(lang: Language) -> &'static str {
        match lang {
            Language::English => "Click or drag the fretboard below to audition",
            Language::SimplifiedChinese => "点击或拖动下方指板即可试听",
        }
    }

    pub fn instrument(lang: Language) -> &'static str {
        match lang {
            Language::English => "INSTRUMENT",
            Language::SimplifiedChinese => "乐器",
        }
    }

    pub fn model(lang: Language) -> &'static str {
        match lang {
            Language::English => "Model",
            Language::SimplifiedChinese => "模型",
        }
    }

    pub fn electric(lang: Language) -> &'static str {
        match lang {
            Language::English => "Electric",
            Language::SimplifiedChinese => "电贝斯",
        }
    }

    pub fn acoustic(lang: Language) -> &'static str {
        match lang {
            Language::English => "Acoustic",
            Language::SimplifiedChinese => "木贝斯",
        }
    }

    pub fn exciter(lang: Language) -> &'static str {
        match lang {
            Language::English => "Exciter",
            Language::SimplifiedChinese => "激励方式",
        }
    }

    pub fn finger(lang: Language) -> &'static str {
        match lang {
            Language::English => "Finger",
            Language::SimplifiedChinese => "指弹",
        }
    }

    pub fn pick(lang: Language) -> &'static str {
        match lang {
            Language::English => "Pick",
            Language::SimplifiedChinese => "拨片",
        }
    }

    pub fn slap(lang: Language) -> &'static str {
        match lang {
            Language::English => "Slap",
            Language::SimplifiedChinese => "击弦",
        }
    }

    pub fn tuning(lang: Language) -> &'static str {
        match lang {
            Language::English => "Tuning",
            Language::SimplifiedChinese => "调弦",
        }
    }

    pub fn five_string(lang: Language) -> &'static str {
        match lang {
            Language::English => "Five-string B0",
            Language::SimplifiedChinese => "五弦低音 B0",
        }
    }

    pub fn string_pickup(lang: Language) -> &'static str {
        match lang {
            Language::English => "STRING / PICKUP",
            Language::SimplifiedChinese => "琴弦 / 拾音器",
        }
    }

    pub fn pluck_position(lang: Language) -> &'static str {
        match lang {
            Language::English => "Pluck position",
            Language::SimplifiedChinese => "拨弦位置",
        }
    }

    pub fn pickup_position(lang: Language) -> &'static str {
        match lang {
            Language::English => "Pickup position",
            Language::SimplifiedChinese => "拾音器位置",
        }
    }

    pub fn tone(lang: Language) -> &'static str {
        match lang {
            Language::English => "Tone",
            Language::SimplifiedChinese => "音色",
        }
    }

    pub fn contact_body(lang: Language) -> &'static str {
        match lang {
            Language::English => "CONTACT / BODY",
            Language::SimplifiedChinese => "接触 / 琴体",
        }
    }

    pub fn fret_buzz(lang: Language) -> &'static str {
        match lang {
            Language::English => "Fret buzz",
            Language::SimplifiedChinese => "品丝杂音",
        }
    }

    pub fn body_mix(lang: Language) -> &'static str {
        match lang {
            Language::English => "Body mix",
            Language::SimplifiedChinese => "琴体混合",
        }
    }

    pub fn slap_note(lang: Language) -> &'static str {
        match lang {
            Language::English => "Slap uses the same bounded contact state as the FDTD string",
            Language::SimplifiedChinese => "击弦使用与 FDTD 琴弦相同的有界接触状态",
        }
    }

    pub fn output(lang: Language) -> &'static str {
        match lang {
            Language::English => "OUTPUT",
            Language::SimplifiedChinese => "输出",
        }
    }

    pub fn master_gain(lang: Language) -> &'static str {
        match lang {
            Language::English => "Master gain",
            Language::SimplifiedChinese => "总输出增益",
        }
    }

    pub fn output_note(lang: Language) -> &'static str {
        match lang {
            Language::English => "Electric: finite-gap pickup · Acoustic: A0 / B1 / bridge hill",
            Language::SimplifiedChinese => "电贝斯：有限磁隙拾音 · 木贝斯：A0 / B1 / 琴桥峰",
        }
    }

    pub fn fretboard_hint(lang: Language) -> &'static str {
        match lang {
            Language::English => "Open notes + 24 frets · MIDI pitch bend remains available",
            Language::SimplifiedChinese => "空弦 + 24 品 · MIDI 仍可控制弯音",
        }
    }
}
