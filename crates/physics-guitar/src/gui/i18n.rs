//! Internationalization (i18n) for Physics Guitar GUI supporting English and Simplified Chinese.

pub use physics_ui::Language;

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
            Language::English => "Electric",
            Language::SimplifiedChinese => "电吉他",
        }
    }

    pub fn mode_acoustic(lang: Language) -> &'static str {
        match lang {
            Language::English => "Acoustic",
            Language::SimplifiedChinese => "木吉他",
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

    pub fn rack_mechanics(lang: Language) -> &'static str {
        match lang {
            Language::English => "MECHANICS & NOISE",
            Language::SimplifiedChinese => "触弦与微观杂音",
        }
    }

    pub fn rack_master(lang: Language) -> &'static str {
        match lang {
            Language::English => "GROOVE & MASTER",
            Language::SimplifiedChinese => "伴奏律动与主输出",
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

    pub fn preset_label(lang: Language) -> &'static str {
        match lang {
            Language::English => "Preset:",
            Language::SimplifiedChinese => "出厂预设:",
        }
    }

    pub fn preset(lang: Language) -> &'static str {
        Self::preset_label(lang)
    }

    pub fn finger_squeak(lang: Language) -> &'static str {
        match lang {
            Language::English => "Finger Squeak:",
            Language::SimplifiedChinese => "指擦副噪声:",
        }
    }

    pub fn groove_pattern(lang: Language) -> &'static str {
        match lang {
            Language::English => "Groove Style:",
            Language::SimplifiedChinese => "伴奏切片风格:",
        }
    }

    pub fn groove_name(idx: i32, lang: Language) -> &'static str {
        match idx {
            1 => match lang {
                Language::English => "Folk 4/4 Basic",
                Language::SimplifiedChinese => "民谣 4/4 扫弦",
            },
            2 => match lang {
                Language::English => "Ballad 6/8 Arp",
                Language::SimplifiedChinese => "慢摇 6/8 分解",
            },
            3 => match lang {
                Language::English => "Funk 16th Mute",
                Language::SimplifiedChinese => "放克 16分 切音",
            },
            4 => match lang {
                Language::English => "Rock 8th Chug",
                Language::SimplifiedChinese => "摇滚 8分 闷音",
            },
            _ => match lang {
                Language::English => "Off (Manual)",
                Language::SimplifiedChinese => "关闭 (自由演奏)",
            },
        }
    }

    pub fn mode_param_label(lang: Language) -> &'static str {
        match lang {
            Language::English => "Mode:",
            Language::SimplifiedChinese => "模式:",
        }
    }

    pub fn pickup_pos_label(lang: Language) -> &'static str {
        match lang {
            Language::English => "Pickup:",
            Language::SimplifiedChinese => "拾音位:",
        }
    }

    pub fn pickup_type_label(lang: Language) -> &'static str {
        match lang {
            Language::English => "Coil:",
            Language::SimplifiedChinese => "拾音结构:",
        }
    }

    pub fn tone_label(lang: Language) -> &'static str {
        match lang {
            Language::English => "Tone:",
            Language::SimplifiedChinese => "音色旋钮:",
        }
    }

    pub fn amp_drive_label(lang: Language) -> &'static str {
        match lang {
            Language::English => "Overdrive:",
            Language::SimplifiedChinese => "前级过载:",
        }
    }

    pub fn palm_mute_label(lang: Language) -> &'static str {
        match lang {
            Language::English => "PalmMute:",
            Language::SimplifiedChinese => "掌心制音:",
        }
    }

    pub fn pluck_pos_label(lang: Language) -> &'static str {
        match lang {
            Language::English => "PluckPos:",
            Language::SimplifiedChinese => "拨弦位置:",
        }
    }

    pub fn pluck_style_label(lang: Language) -> &'static str {
        match lang {
            Language::English => "Style:",
            Language::SimplifiedChinese => "拨弦方式:",
        }
    }

    pub fn strum_speed_label(lang: Language) -> &'static str {
        match lang {
            Language::English => "Speed:",
            Language::SimplifiedChinese => "扫弦速度:",
        }
    }

    pub fn fret_buzz_label(lang: Language) -> &'static str {
        match lang {
            Language::English => "Buzz:",
            Language::SimplifiedChinese => "打品碰撞:",
        }
    }

    pub fn finger_squeak_label(lang: Language) -> &'static str {
        match lang {
            Language::English => "Squeak:",
            Language::SimplifiedChinese => "指擦杂音:",
        }
    }

    pub fn groove_pattern_label(lang: Language) -> &'static str {
        match lang {
            Language::English => "Pattern:",
            Language::SimplifiedChinese => "伴奏风格:",
        }
    }

    pub fn bpm_label(lang: Language) -> &'static str {
        match lang {
            Language::English => "BPM:",
            Language::SimplifiedChinese => "伴奏速度:",
        }
    }

    pub fn master_gain_label(lang: Language) -> &'static str {
        match lang {
            Language::English => "Volume:",
            Language::SimplifiedChinese => "主音量:",
        }
    }
}

pub use physics_ui::setup_vizia_fonts;
