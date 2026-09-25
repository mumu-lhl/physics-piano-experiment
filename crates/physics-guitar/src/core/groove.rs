//! Smart Strumming & Groove Pattern Engine.
//!
//! Provides tempo-synced and humanized strumming rhythm patterns,
//! fingerstyle arpeggios, funk chops, and palm-muted rock chugs.

use crate::core::strummer::StrumDirection;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroovePattern {
    Off = 0,
    FolkStrum44 = 1,
    BalladArp68 = 2,
    Funk16th = 3,
    RockChug8th = 4,
}

impl GroovePattern {
    pub fn from_index(idx: i32) -> Self {
        match idx {
            1 => GroovePattern::FolkStrum44,
            2 => GroovePattern::BalladArp68,
            3 => GroovePattern::Funk16th,
            4 => GroovePattern::RockChug8th,
            _ => GroovePattern::Off,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum GrooveAction {
    None,
    Strum {
        direction: StrumDirection,
        velocity: f64,
        palm_mute_override: Option<f64>,
    },
    PluckString {
        string_rel_index: usize, // 0 = lowest held, 1 = second lowest, etc.
        velocity: f64,
    },
}

#[derive(Debug, Clone)]
pub struct GrooveEngine {
    pub pattern: GroovePattern,
    pub bpm: f64,
    pub sample_rate: f64,
    sample_counter: usize,
    samples_per_subdivision: usize,
    step_index: usize,
    next_step_samples: usize,
    pub humanize_ms: f64,
    rng_state: u64,
}

impl GrooveEngine {
    pub fn new(sample_rate: f64) -> Self {
        let mut engine = Self {
            pattern: GroovePattern::Off,
            bpm: 120.0,
            sample_rate,
            sample_counter: 0,
            samples_per_subdivision: (sample_rate * 60.0 / 120.0 / 4.0).round() as usize, // default 16th note
            step_index: 0,
            next_step_samples: 0,
            humanize_ms: 3.5, // 3.5ms natural human timing deviation
            rng_state: 0x9371_8462_0192_8374,
        };
        engine.recalculate_subdivision();
        engine
    }

    pub fn set_bpm(&mut self, bpm: f64) {
        let clamped = bpm.clamp(40.0, 240.0);
        if (self.bpm - clamped).abs() > 0.05 {
            self.bpm = clamped;
            self.recalculate_subdivision();
        }
    }

    pub fn set_pattern(&mut self, pattern: GroovePattern) {
        if self.pattern != pattern {
            self.pattern = pattern;
            self.step_index = 0;
            self.sample_counter = 0;
            self.recalculate_subdivision();
        }
    }

    fn recalculate_subdivision(&mut self) {
        let subdivision_factor = match self.pattern {
            GroovePattern::Off => 4.0,
            GroovePattern::FolkStrum44 => 4.0, // 16th notes (4 per beat)
            GroovePattern::BalladArp68 => 3.0, // 8th note triplets in 6/8
            GroovePattern::Funk16th => 4.0,    // 16th notes
            GroovePattern::RockChug8th => 2.0, // 8th notes (2 per beat)
        };
        let nominal = (self.sample_rate * 60.0 / self.bpm / subdivision_factor).max(100.0);
        self.samples_per_subdivision = nominal.round() as usize;
        self.next_step_samples = self.samples_per_subdivision;
    }

    #[inline(always)]
    fn next_jitter(&mut self) -> f64 {
        self.rng_state ^= self.rng_state << 13;
        self.rng_state ^= self.rng_state >> 7;
        self.rng_state ^= self.rng_state << 17;
        let uniform = (self.rng_state as f64 / u64::MAX as f64) * 2.0 - 1.0;
        uniform * (self.humanize_ms * 0.001 * self.sample_rate)
    }

    pub fn reset(&mut self) {
        self.step_index = 0;
        self.sample_counter = 0;
    }

    /// Advances 1 audio sample and produces an action when a groove step triggers.
    #[inline(always)]
    pub fn step_sample(&mut self) -> GrooveAction {
        if self.pattern == GroovePattern::Off {
            return GrooveAction::None;
        }

        self.sample_counter += 1;
        if self.sample_counter < self.next_step_samples {
            return GrooveAction::None;
        }

        self.sample_counter = 0;
        let jitter = self.next_jitter();
        self.next_step_samples = ((self.samples_per_subdivision as f64 + jitter).round() as isize).max(100) as usize;

        let current_step = self.step_index;
        let action = match self.pattern {
            GroovePattern::Off => GrooveAction::None,

            GroovePattern::FolkStrum44 => {
                // 16-step bar: D . D . D . . U . U D . D . U .
                // Classic folk syncopated acoustic rhythm
                self.step_index = (self.step_index + 1) % 16;
                match current_step {
                    0 => GrooveAction::Strum { direction: StrumDirection::Down, velocity: 0.95, palm_mute_override: None },
                    2 => GrooveAction::Strum { direction: StrumDirection::Down, velocity: 0.70, palm_mute_override: None },
                    4 => GrooveAction::Strum { direction: StrumDirection::Down, velocity: 0.88, palm_mute_override: None },
                    7 => GrooveAction::Strum { direction: StrumDirection::Up, velocity: 0.75, palm_mute_override: None },
                    9 => GrooveAction::Strum { direction: StrumDirection::Up, velocity: 0.78, palm_mute_override: None },
                    10 => GrooveAction::Strum { direction: StrumDirection::Down, velocity: 0.85, palm_mute_override: None },
                    12 => GrooveAction::Strum { direction: StrumDirection::Down, velocity: 0.82, palm_mute_override: None },
                    14 => GrooveAction::Strum { direction: StrumDirection::Up, velocity: 0.72, palm_mute_override: None },
                    _ => GrooveAction::None,
                }
            }

            GroovePattern::BalladArp68 => {
                // 6-step arpeggio pattern: Bass -> 3 -> 2 -> 1 -> 2 -> 3
                self.step_index = (self.step_index + 1) % 6;
                match current_step {
                    0 => GrooveAction::PluckString { string_rel_index: 0, velocity: 0.95 }, // Bass note
                    1 => GrooveAction::PluckString { string_rel_index: 1, velocity: 0.70 },
                    2 => GrooveAction::PluckString { string_rel_index: 2, velocity: 0.75 },
                    3 => GrooveAction::PluckString { string_rel_index: 3, velocity: 0.85 }, // Treble climax
                    4 => GrooveAction::PluckString { string_rel_index: 2, velocity: 0.68 },
                    5 => GrooveAction::PluckString { string_rel_index: 1, velocity: 0.68 },
                    _ => GrooveAction::None,
                }
            }

            GroovePattern::Funk16th => {
                // 16-step funk rhythm with percussive muted chops
                self.step_index = (self.step_index + 1) % 16;
                match current_step {
                    0 => GrooveAction::Strum { direction: StrumDirection::Down, velocity: 0.95, palm_mute_override: None },
                    2 => GrooveAction::Strum { direction: StrumDirection::Up, velocity: 0.55, palm_mute_override: Some(0.85) }, // muted chick
                    4 => GrooveAction::Strum { direction: StrumDirection::Down, velocity: 0.82, palm_mute_override: None },
                    6 => GrooveAction::Strum { direction: StrumDirection::Up, velocity: 0.70, palm_mute_override: None },
                    7 => GrooveAction::Strum { direction: StrumDirection::Down, velocity: 0.50, palm_mute_override: Some(0.90) }, // muted chick
                    10 => GrooveAction::Strum { direction: StrumDirection::Down, velocity: 0.88, palm_mute_override: None },
                    12 => GrooveAction::Strum { direction: StrumDirection::Up, velocity: 0.55, palm_mute_override: Some(0.85) }, // muted chick
                    14 => GrooveAction::Strum { direction: StrumDirection::Up, velocity: 0.76, palm_mute_override: None },
                    _ => GrooveAction::None,
                }
            }

            GroovePattern::RockChug8th => {
                // 8-step hard rock heavy palm-muted 8th chug with accented downbeats
                self.step_index = (self.step_index + 1) % 8;
                let vel = match current_step {
                    0 => 0.98,
                    2 => 0.85,
                    4 => 0.92,
                    6 => 0.85,
                    _ => 0.72,
                };
                GrooveAction::Strum {
                    direction: StrumDirection::Down,
                    velocity: vel,
                    palm_mute_override: Some(0.70),
                }
            }
        };

        action
    }
}
