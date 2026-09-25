//! Smart Guitar Strumming Engine.
//!
//! Converts simultaneous MIDI chord notes into realistic physical down-strum
//! and up-strum picking sequences with staggered string delays (5ms ~ 50ms)
//! and pick resistance velocity dynamics.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrumDirection {
    /// Low E (string 6) to High E (string 1)
    Down,
    /// High E (string 1) to Low E (string 6)
    Up,
    /// Automatically alternates Down and Up strokes
    Auto,
}

#[derive(Debug, Clone, Copy)]
pub struct StrumPluckEvent {
    pub string_index: usize, // 0..5 (0 = String 1 high E, 5 = String 6 low E)
    pub fret: u8,
    pub velocity: f64,
    pub delay_samples: usize,
}

#[derive(Debug, Clone)]
pub struct SmartStrummer {
    /// Strum speed in milliseconds per stroke [0.0 = instant/off, 5.0ms ~ 50.0ms]
    pub strum_speed_ms: f64,
    pub direction: StrumDirection,
    pub sample_rate: f64,
    pub pending_plucks: Vec<StrumPluckEvent>,
    last_direction_was_down: bool,
    /// Note accumulation buffer for grouping near-simultaneous MIDI chord notes
    pub accum_buffer: Vec<(usize, u8, f64)>,
    pub accum_timer: usize,
}

impl SmartStrummer {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            strum_speed_ms: 18.0, // Natural ~18ms acoustic strum by default
            direction: StrumDirection::Auto,
            sample_rate,
            pending_plucks: Vec::with_capacity(16),
            last_direction_was_down: false,
            accum_buffer: Vec::with_capacity(6),
            accum_timer: 0,
        }
    }

    pub fn set_strum_speed_ms(&mut self, ms: f64) {
        self.strum_speed_ms = ms.clamp(0.0, 80.0);
    }

    /// Receives an individual note event (e.g. from MIDI Note On).
    /// If strumming is disabled (speed <= 1ms), it triggers immediately.
    /// Otherwise, aggregates notes arriving within ~6ms into a single chord strum.
    pub fn trigger_note(&mut self, string_index: usize, fret: u8, velocity: f64) {
        if self.strum_speed_ms <= 1.0 {
            self.pending_plucks.push(StrumPluckEvent {
                string_index,
                fret,
                velocity,
                delay_samples: 0,
            });
            return;
        }

        // Add to accumulation buffer and reset/start grouping window (~6ms)
        self.accum_buffer.retain(|&(s, _, _)| s != string_index);
        self.accum_buffer.push((string_index, fret, velocity));
        self.accum_timer = (0.006 * self.sample_rate).round() as usize;
    }

    /// Enqueues a cluster of chord notes to be strummed.
    /// `notes` is a slice of `(string_index, fret, velocity)` where `string_index` is 0..5.
    pub fn trigger_chord(&mut self, notes: &[(usize, u8, f64)]) {
        if notes.is_empty() {
            return;
        }

        if self.strum_speed_ms <= 1.0 {
            // Instant trigger without delay
            for &(s_idx, fret, vel) in notes {
                self.pending_plucks.push(StrumPluckEvent {
                    string_index: s_idx,
                    fret,
                    velocity: vel,
                    delay_samples: 0,
                });
            }
            return;
        }

        // Determine strum direction
        let is_down = match self.direction {
            StrumDirection::Down => true,
            StrumDirection::Up => false,
            StrumDirection::Auto => !self.last_direction_was_down,
        };
        self.last_direction_was_down = is_down;

        let mut sorted = notes.to_vec();
        if is_down {
            // Down-strum: low pitch string (index 5) down to high pitch string (index 0)
            sorted.sort_by(|a, b| b.0.cmp(&a.0));
        } else {
            // Up-strum: high pitch string (index 0) up to low pitch string (index 5)
            sorted.sort_by(|a, b| a.0.cmp(&b.0));
        }

        let num_strings = sorted.len();
        let total_delay_samples = (self.strum_speed_ms * 0.001 * self.sample_rate).max(1.0);
        let step_delay = if num_strings > 1 {
            total_delay_samples / (num_strings - 1) as f64
        } else {
            0.0
        };

        for (i, &(s_idx, fret, base_vel)) in sorted.iter().enumerate() {
            // Pick resistance dynamics: subtle velocity drop along pick travel
            let vel_scale = 1.0 - 0.04 * (i as f64);
            let vel = (base_vel * vel_scale).clamp(0.1, 1.0);
            let delay = (i as f64 * step_delay).round() as usize;

            self.pending_plucks.push(StrumPluckEvent {
                string_index: s_idx,
                fret,
                velocity: vel,
                delay_samples: delay,
            });
        }
    }

    /// Advances 1 audio sample and collects all plucks that are ready into `ready`.
    #[inline(always)]
    pub fn step_into(&mut self, ready: &mut Vec<StrumPluckEvent>) {
        if self.accum_timer > 0 {
            self.accum_timer -= 1;
            if self.accum_timer == 0 && !self.accum_buffer.is_empty() {
                let notes = std::mem::take(&mut self.accum_buffer);
                self.trigger_chord(&notes);
            }
        }

        let mut i = 0;
        while i < self.pending_plucks.len() {
            if self.pending_plucks[i].delay_samples == 0 {
                let p = self.pending_plucks.swap_remove(i);
                ready.push(p);
            } else {
                self.pending_plucks[i].delay_samples -= 1;
                i += 1;
            }
        }
    }

    /// Advances 1 audio sample and drains all plucks that are ready to trigger.
    #[inline(always)]
    pub fn step(&mut self, mut on_pluck: impl FnMut(usize, u8, f64)) {
        if self.accum_timer > 0 {
            self.accum_timer -= 1;
            if self.accum_timer == 0 && !self.accum_buffer.is_empty() {
                let notes = std::mem::take(&mut self.accum_buffer);
                self.trigger_chord(&notes);
            }
        }

        let mut i = 0;
        while i < self.pending_plucks.len() {
            if self.pending_plucks[i].delay_samples == 0 {
                let p = self.pending_plucks.swap_remove(i);
                on_pluck(p.string_index, p.fret, p.velocity);
            } else {
                self.pending_plucks[i].delay_samples -= 1;
                i += 1;
            }
        }
    }
}
