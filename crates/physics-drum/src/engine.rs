//! General-MIDI mapped sample-accurate drum-kit engine.

use crate::voices::{CymbalVoice, KickVoice, SnareVoice, TomVoice};

/// The drum components implemented by the kit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrumVoice {
    Kick,
    Snare,
    Tom1,
    Tom2,
    Tom3,
    ClosedHat,
    PedalHat,
    OpenHat,
    Crash,
    Ride,
}

/// Maps the General-MIDI note numbers implemented by the kit.
#[inline]
pub fn voice_for_note(note: u8) -> Option<DrumVoice> {
    Some(match note {
        35 | 36 => DrumVoice::Kick,
        38 | 40 => DrumVoice::Snare,
        41 => DrumVoice::Tom1,
        43 | 45 => DrumVoice::Tom2,
        47 | 48 | 50 => DrumVoice::Tom3,
        42 => DrumVoice::ClosedHat,
        44 => DrumVoice::PedalHat,
        46 => DrumVoice::OpenHat,
        49 => DrumVoice::Crash,
        51 => DrumVoice::Ride,
        _ => return None,
    })
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DrumEvent {
    NoteOn {
        time: usize,
        note: u8,
        velocity: f64,
    },
    NoteOff {
        time: usize,
        note: u8,
    },
    HiHatOpen {
        time: usize,
        amount: f64,
    },
}

/// Fixed-state physical drum kit. General MIDI notes are used so the adapter can
/// be driven directly by a DAW without a second mapping layer.
#[derive(Debug, Clone)]
pub struct DrumEngine {
    pub sample_rate: f64,
    pub dt: f64,
    pub kick: KickVoice,
    pub snare: SnareVoice,
    pub toms: [TomVoice; 3],
    pub hats: CymbalVoice,
    pub crash: CymbalVoice,
    pub ride: CymbalVoice,
    pub hi_hat_open: f64,
    pub cymbal_decay: f64,
    pub master_gain: f64,
    pub snare_tightness: f64,
    pub snare_decay: f64,
}

impl DrumEngine {
    pub fn new(sample_rate: f64) -> Self {
        let sample_rate = sample_rate.max(1.0);
        Self {
            sample_rate,
            dt: 1.0 / sample_rate,
            kick: KickVoice::new(sample_rate, 58.0, 0.62, 90_000.0),
            snare: SnareVoice::new(sample_rate),
            toms: [
                TomVoice::new(sample_rate, 92.0, 0.82, 42_000.0),
                TomVoice::new(sample_rate, 128.0, 0.92, 32_000.0),
                TomVoice::new(sample_rate, 170.0, 1.05, 25_000.0),
            ],
            hats: CymbalVoice::new(sample_rate),
            crash: CymbalVoice::new(sample_rate),
            ride: CymbalVoice::new(sample_rate),
            hi_hat_open: 0.85,
            cymbal_decay: 1.0,
            master_gain: 0.78,
            snare_tightness: 0.62,
            snare_decay: 0.58,
        }
    }

    pub fn set_hi_hat_open(&mut self, amount: f64) {
        self.hi_hat_open = amount.clamp(0.0, 1.0);
        self.hats.set_open_amount(self.hi_hat_open);
    }

    pub fn set_snare_tightness(&mut self, amount: f64) {
        let amount = amount.clamp(0.0, 1.0);
        if (self.snare_tightness - amount).abs() > 1e-5 {
            self.snare_tightness = amount;
            self.snare.set_tightness(self.snare_tightness);
        }
    }

    pub fn set_snare_decay(&mut self, amount: f64) {
        let amount = amount.clamp(0.0, 1.0);
        if (self.snare_decay - amount).abs() > 1e-5 {
            self.snare_decay = amount;
            self.snare.set_decay(self.snare_decay);
        }
    }

    pub fn set_cymbal_decay(&mut self, amount: f64) {
        let amount = amount.clamp(0.0, 1.0);
        let scale = 0.25 + 1.75 * amount;
        if (self.cymbal_decay - amount).abs() > 1e-5 {
            self.cymbal_decay = amount;
            self.hats.set_decay_scale(scale);
            self.crash.set_decay_scale(scale);
            self.ride.set_decay_scale(scale);
        }
    }

    pub fn set_master_gain(&mut self, gain: f64) {
        self.master_gain = gain.clamp(0.0, 2.0);
    }

    pub fn trigger(&mut self, note: u8, velocity: f64) {
        let velocity = velocity.clamp(0.001, 1.0);
        match note {
            35 | 36 => self.kick.trigger(velocity),
            38 | 40 => {
                self.snare.set_tightness(self.snare_tightness);
                self.snare.set_decay(self.snare_decay);
                self.snare.trigger(velocity);
            }
            41 => self.toms[0].trigger(velocity),
            43 | 45 => self.toms[1].trigger(velocity),
            47 | 48 | 50 => self.toms[2].trigger(velocity),
            42 => {
                // Closed hats choke an already open cymbal before the new hit.
                self.hats.choke(0.08);
                self.hats.trigger(velocity, 0.0);
            }
            44 => {
                self.hats.choke(0.0);
                self.hats.trigger(velocity * 0.72, 0.0);
            }
            46 => self.hats.trigger(velocity, self.hi_hat_open),
            49 => self.crash.trigger(velocity, 1.0),
            51 => self.ride.trigger(velocity * 0.9, 1.0),
            _ => {}
        }
    }

    pub fn release(&mut self, note: u8) {
        // Drum voices are one-shot physical decays. A note-off is meaningful for
        // cymbal choke groups and intentionally does not hard-zero a membrane.
        if matches!(note, 42 | 44 | 46) {
            self.hats.choke(0.0);
        }
    }

    /// Renders one sample. Stereo is a deterministic small pan rather than a
    /// second physical model, so both channels share the same energy evolution.
    #[inline]
    pub fn process_sample(&mut self) -> (f64, f64) {
        let kick = self.kick.step(self.dt);
        let snare = self.snare.step(self.dt);
        let tom1 = self.toms[0].step(self.dt);
        let tom2 = self.toms[1].step(self.dt);
        let tom3 = self.toms[2].step(self.dt);
        let hats = self.hats.step();
        let crash = self.crash.step();
        let ride = self.ride.step();
        let left = (kick * 1.00
            + snare * 0.94
            + tom1 * 0.82
            + tom2 * 0.88
            + tom3 * 0.94
            + hats * 0.76
            + crash * 0.82
            + ride * 0.70)
            * self.master_gain;
        let right = (kick * 0.98
            + snare * 1.00
            + tom1 * 0.92
            + tom2 * 0.88
            + tom3 * 0.82
            + hats * 0.88
            + crash * 0.92
            + ride * 1.00)
            * self.master_gain;
        (soft_limit(left), soft_limit(right))
    }

    pub fn process_block(
        &mut self,
        num_samples: usize,
        events: &[DrumEvent],
        out_left: &mut [f64],
        out_right: &mut [f64],
    ) {
        assert!(out_left.len() >= num_samples);
        assert!(out_right.len() >= num_samples);
        let mut event_index = 0;
        for sample in 0..num_samples {
            while event_index < events.len() && event_time(events[event_index]) <= sample {
                self.dispatch(events[event_index]);
                event_index += 1;
            }
            let (left, right) = self.process_sample();
            out_left[sample] = left;
            out_right[sample] = right;
        }
    }

    pub fn reset(&mut self) {
        self.kick.reset();
        self.snare.reset();
        for tom in &mut self.toms {
            tom.reset();
        }
        self.hats.reset();
        self.crash.reset();
        self.ride.reset();
    }

    pub fn energy(&self) -> f64 {
        self.kick.top.energy()
            + self.kick.bottom.energy()
            + self.snare.top.energy()
            + self.snare.bottom.energy()
            + self
                .toms
                .iter()
                .map(|tom| tom.top.energy() + tom.bottom.energy())
                .sum::<f64>()
            + self.hats.energy()
            + self.crash.energy()
            + self.ride.energy()
    }

    fn dispatch(&mut self, event: DrumEvent) {
        match event {
            DrumEvent::NoteOn { note, velocity, .. } => self.trigger(note, velocity),
            DrumEvent::NoteOff { note, .. } => self.release(note),
            DrumEvent::HiHatOpen { amount, .. } => self.set_hi_hat_open(amount),
        }
    }
}

/// Enables FTZ/DAZ on x86 audio threads to avoid denormal slow paths.
#[inline]
pub fn sanitize_floating_point_environment() {
    #[cfg(any(target_arch = "x86_64", target_arch = "x86"))]
    unsafe {
        use std::arch::asm;
        let mut control_word: u32 = 0;
        asm!(
            "stmxcsr [{ptr}]",
            ptr = in(reg) &mut control_word,
            options(nostack, preserves_flags)
        );
        control_word |= 0x8040;
        asm!(
            "ldmxcsr [{ptr}]",
            ptr = in(reg) &control_word,
            options(nostack, preserves_flags)
        );
    }
}

#[inline(always)]
fn event_time(event: DrumEvent) -> usize {
    match event {
        DrumEvent::NoteOn { time, .. }
        | DrumEvent::NoteOff { time, .. }
        | DrumEvent::HiHatOpen { time, .. } => time,
    }
}

#[inline(always)]
fn soft_limit(sample: f64) -> f64 {
    let magnitude = sample.abs();
    if magnitude <= 0.88 {
        sample
    } else {
        sample.signum() * (0.88 + 0.12 * ((magnitude - 0.88) / 0.12).tanh())
    }
}

#[cfg(test)]
mod tests {
    use super::{DrumEngine, DrumEvent};

    #[test]
    fn general_midi_kit_renders_sample_accurate_hits() {
        let mut engine = DrumEngine::new(48_000.0);
        let mut left = [0.0; 4_096];
        let mut right = [0.0; 4_096];
        let events = [
            DrumEvent::NoteOn {
                time: 0,
                note: 36,
                velocity: 0.95,
            },
            DrumEvent::NoteOn {
                time: 120,
                note: 38,
                velocity: 0.8,
            },
            DrumEvent::NoteOn {
                time: 180,
                note: 46,
                velocity: 0.75,
            },
        ];
        engine.process_block(4_096, &events, &mut left, &mut right);
        assert!(left.iter().all(|sample| sample.is_finite()));
        assert!(right.iter().all(|sample| sample.is_finite()));
        assert!(left.iter().any(|sample| sample.abs() > 1e-8));
        assert!(engine.energy().is_finite());
    }

    #[test]
    fn hat_choke_is_not_a_global_reset() {
        let mut engine = DrumEngine::new(48_000.0);
        engine.trigger(46, 1.0);
        for _ in 0..64 {
            let _ = engine.process_sample();
        }
        let before = engine.hats.energy();
        engine.trigger(42, 1.0);
        assert!(engine.hats.energy() < before + 1.0);
    }
}
