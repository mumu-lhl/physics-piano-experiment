//! Sample-accurate bass instrument engine.

use crate::acoustic::{AcousticBassBody, BassPickup};
use crate::params::BassStringParams;
use crate::string::{FdtdString, PluckStyle};

const STRING_COUNT: usize = 5;
const FOUR_STRING_FIRST: usize = 1;
const SOFT_LIMIT_KNEE: f64 = 0.88;

/// Choose the electric pickup or the wooden bass-body radiation path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BassMode {
    Electric,
    Acoustic,
}

/// A compact event interface shared by the offline renderer and plugin adapter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BassEvent {
    NoteOn {
        time: usize,
        note: u8,
        velocity: f64,
    },
    NoteOff {
        time: usize,
        note: u8,
    },
    PitchBend {
        time: usize,
        semitones: f64,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct BassFrame {
    pub pickup: f64,
    pub bridge_force: f64,
    pub energy: f64,
    pub active: bool,
}

/// One playable string and its current note assignment.
#[derive(Debug, Clone)]
pub struct BassStringVoice {
    pub string: FdtdString,
    pub open_midi: u8,
    pub current_note: Option<u8>,
    pub current_fret: u8,
}

impl BassStringVoice {
    pub fn new(params: BassStringParams, sample_rate: f64) -> Self {
        Self {
            string: FdtdString::new(params, sample_rate),
            open_midi: params.open_midi,
            current_note: None,
            current_fret: 0,
        }
    }

    pub fn trigger(&mut self, note: u8, velocity: f64, style: PluckStyle, position: f64) {
        let fret = note.saturating_sub(self.open_midi).min(24);
        self.current_fret = fret;
        self.string.set_fret(fret);
        self.string.trigger(velocity, style, position);
        self.current_note = Some(note);
    }

    pub fn release(&mut self) {
        self.string.release();
    }

    #[inline]
    pub fn step(&mut self) -> BassFrame {
        let bridge_force = self.string.step();
        BassFrame {
            pickup: 0.0,
            bridge_force,
            energy: self.string.energy(),
            active: self.string.is_active,
        }
    }

    pub fn reset(&mut self) {
        self.string.reset();
        self.current_note = None;
        self.current_fret = 0;
    }

    pub fn reconfigure_params(&mut self, params: BassStringParams) {
        self.open_midi = params.open_midi;
        self.string.reconfigure_params(params);
        self.current_note = None;
        self.current_fret = 0;
    }
}

/// Four- or five-string physical bass engine.
///
/// The interface intentionally keeps MIDI routing, sample-accurate events and
/// stereo rendering outside the FDTD module.  All per-sample work below uses the
/// arrays prepared by `new`; `process_sample` does not allocate or acquire locks.
#[derive(Debug, Clone)]
pub struct BassEngine {
    pub sample_rate: f64,
    pub dt: f64,
    pub mode: BassMode,
    pub five_string: bool,
    pub strings: [BassStringVoice; STRING_COUNT],
    pub pickup: BassPickup,
    pub body: AcousticBassBody,
    pub pluck_style: PluckStyle,
    pub pluck_position: f64,
    pub fret_buzz: f64,
    pub body_mix: f64,
    pub master_gain: f64,
    pub pitch_bend_semitones: f64,
}

impl BassEngine {
    pub fn new(sample_rate: f64, mode: BassMode, five_string: bool) -> Self {
        let sample_rate = sample_rate.max(1.0);
        let params = match mode {
            BassMode::Electric => BassStringParams::electric_five(),
            BassMode::Acoustic => BassStringParams::acoustic_five(),
        };
        let strings = std::array::from_fn(|index| BassStringVoice::new(params[index], sample_rate));
        Self {
            sample_rate,
            dt: 1.0 / sample_rate,
            mode,
            five_string,
            strings,
            pickup: BassPickup::new(sample_rate),
            body: AcousticBassBody::new(sample_rate),
            pluck_style: PluckStyle::Finger,
            pluck_position: 0.18,
            fret_buzz: 0.30,
            body_mix: 0.75,
            master_gain: 0.82,
            pitch_bend_semitones: 0.0,
        }
    }

    /// MIDI-to-string routing chooses the highest suitable open string, leaving
    /// the lower strings available for low notes and avoiding fret 24+.
    pub fn route_note(&self, note: u8) -> Option<(usize, u8)> {
        let first = if self.five_string {
            0
        } else {
            FOUR_STRING_FIRST
        };
        let mut selected = None;
        for index in first..STRING_COUNT {
            let open = self.strings[index].open_midi;
            if note >= open {
                let fret = note - open;
                if fret <= 24 {
                    selected = Some((index, fret));
                }
            }
        }
        selected
    }

    pub fn note_on(&mut self, note: u8, velocity: f64) {
        if let Some((index, _fret)) = self.route_note(note) {
            self.note_on_string(
                index,
                note.saturating_sub(self.strings[index].open_midi),
                velocity,
            );
        }
    }

    /// Triggers the physical string selected by an interactive fretboard.
    /// MIDI routing cannot be used here: the same pitch can exist on multiple
    /// strings, and a GUI click must preserve the user's chosen string.
    pub fn note_on_string(&mut self, index: usize, fret: u8, velocity: f64) {
        let first = if self.five_string {
            0
        } else {
            FOUR_STRING_FIRST
        };
        if index < first || index >= STRING_COUNT {
            return;
        }
        let fret = fret.min(24);
        let note = self.strings[index].open_midi.saturating_add(fret);
        if self.strings[index].current_note.is_some() {
            self.strings[index].reset();
        }
        self.strings[index].string.fret_buzz = self.fret_buzz;
        self.strings[index].trigger(note, velocity, self.pluck_style, self.pluck_position);
    }

    pub fn note_off(&mut self, note: u8) {
        for voice in &mut self.strings {
            if voice.current_note == Some(note) {
                voice.release();
            }
        }
    }

    pub fn note_off_string(&mut self, index: usize) {
        if let Some(voice) = self.strings.get_mut(index) {
            voice.release();
        }
    }

    pub fn set_mode(&mut self, mode: BassMode) {
        if self.mode == mode {
            return;
        }
        self.mode = mode;
        let params = match mode {
            BassMode::Electric => BassStringParams::electric_five(),
            BassMode::Acoustic => BassStringParams::acoustic_five(),
        };
        for (index, p) in params.iter().enumerate() {
            self.strings[index].reconfigure_params(*p);
            self.strings[index].string.fret_buzz = self.fret_buzz;
        }
    }

    pub fn set_five_string(&mut self, enabled: bool) {
        if self.five_string == enabled {
            return;
        }
        self.five_string = enabled;
        if !enabled {
            self.strings[0].reset();
        }
    }

    pub fn set_pluck_style(&mut self, style: PluckStyle) {
        self.pluck_style = style;
    }

    pub fn set_pluck_position(&mut self, position: f64) {
        self.pluck_position = position.clamp(0.06, 0.45);
    }

    pub fn set_fret_buzz(&mut self, buzz: f64) {
        let buzz = buzz.clamp(0.0, 1.0);
        if (self.fret_buzz - buzz).abs() <= 1e-5 {
            return;
        }
        self.fret_buzz = buzz;
        for voice in &mut self.strings {
            voice.string.fret_buzz = self.fret_buzz;
        }
    }

    pub fn set_tone(&mut self, tone: f64) {
        self.pickup.set_tone(tone);
    }

    pub fn set_pickup_position(&mut self, position: f64) {
        self.pickup.set_position(position);
    }

    pub fn set_master_gain(&mut self, gain: f64) {
        self.master_gain = gain.clamp(0.0, 2.0);
    }

    pub fn set_body_mix(&mut self, mix: f64) {
        self.body_mix = mix.clamp(0.0, 1.0);
    }

    pub fn set_pitch_bend(&mut self, semitones: f64) {
        self.pitch_bend_semitones = semitones.clamp(-24.0, 24.0);
        for voice in &mut self.strings {
            voice.string.set_pitch_bend(self.pitch_bend_semitones);
        }
    }

    /// Advances one sample and returns a small fixed stereo field.
    #[inline]
    pub fn process_sample(&mut self) -> (f64, f64) {
        let mut pickup_signal = 0.0;
        let mut bridge_force = 0.0;
        let mut total_energy = 0.0;
        let first = if self.five_string {
            0
        } else {
            FOUR_STRING_FIRST
        };
        let bridge_displacement = self.body.bridge_displacement();
        for index in first..STRING_COUNT {
            self.strings[index]
                .string
                .set_bridge_displacement(bridge_displacement);
            let frame = self.strings[index].step();
            bridge_force += frame.bridge_force;
            total_energy += frame.energy;
            if self.mode == BassMode::Electric {
                pickup_signal += self.pickup.process_string(&self.strings[index].string);
            }
            if !frame.active && !self.strings[index].string.is_held {
                self.strings[index].current_note = None;
            }
        }

        let body_signal = self.body.process(bridge_force);
        let signal = match self.mode {
            BassMode::Electric => pickup_signal + body_signal * self.body_mix * 0.08,
            BassMode::Acoustic => body_signal * (0.55 + 0.45 * self.body_mix),
        } * self.master_gain;
        let signal = soft_limit(signal);
        let pan = match self.mode {
            BassMode::Electric => (bridge_force * 0.00001).tanh() * 0.035,
            BassMode::Acoustic => (bridge_force * 0.00002).tanh() * 0.065,
        };
        let _ = total_energy; // Kept in the local for branch-friendly diagnostics.
        (signal * (1.0 - pan), signal * (1.0 + pan))
    }

    /// Renders a block and applies events at exact sample offsets.
    pub fn process_block(
        &mut self,
        num_samples: usize,
        events: &[BassEvent],
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
        for voice in &mut self.strings {
            voice.reset();
            voice.string.set_pitch_bend(0.0);
        }
        self.body.reset(self.sample_rate);
        self.pickup.reset();
        self.pitch_bend_semitones = 0.0;
    }

    pub fn total_energy(&self) -> f64 {
        self.strings.iter().map(|voice| voice.string.energy()).sum()
    }

    fn dispatch(&mut self, event: BassEvent) {
        match event {
            BassEvent::NoteOn { note, velocity, .. } => self.note_on(note, velocity),
            BassEvent::NoteOff { note, .. } => self.note_off(note),
            BassEvent::PitchBend { semitones, .. } => self.set_pitch_bend(semitones),
        }
    }
}

/// Enables FTZ/DAZ on x86 audio threads. Other targets already use their
/// normal finite-value handling and need no platform-specific instruction.
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
fn event_time(event: BassEvent) -> usize {
    match event {
        BassEvent::NoteOn { time, .. }
        | BassEvent::NoteOff { time, .. }
        | BassEvent::PitchBend { time, .. } => time,
    }
}

#[inline(always)]
fn soft_limit(sample: f64) -> f64 {
    let magnitude = sample.abs();
    if magnitude <= SOFT_LIMIT_KNEE {
        sample
    } else {
        sample.signum()
            * (SOFT_LIMIT_KNEE
                + (1.0 - SOFT_LIMIT_KNEE)
                    * ((magnitude - SOFT_LIMIT_KNEE) / (1.0 - SOFT_LIMIT_KNEE)).tanh())
    }
}

#[cfg(test)]
mod tests {
    use super::{BassEngine, BassEvent, BassMode};
    use crate::string::PluckStyle;

    #[test]
    fn routes_four_and_five_string_ranges() {
        let four = BassEngine::new(48_000.0, BassMode::Electric, false);
        assert!(four.route_note(28).is_some());
        assert!(four.route_note(23).is_none());
        let five = BassEngine::new(48_000.0, BassMode::Electric, true);
        assert_eq!(five.route_note(23), Some((0, 0)));
    }

    #[test]
    fn gui_string_selection_does_not_reroute_to_another_string() {
        let mut engine = BassEngine::new(48_000.0, BassMode::Electric, true);
        engine.note_on_string(1, 5, 0.8); // A string, E2
        assert_eq!(engine.strings[1].current_note, Some(33));
        assert!(engine.strings[0].current_note.is_none());
        engine.note_off_string(1);
        assert!(engine.strings[1].string.is_releasing);
    }

    #[test]
    fn sample_accurate_note_has_finite_nonzero_output() {
        let mut engine = BassEngine::new(48_000.0, BassMode::Electric, false);
        engine.set_pluck_style(PluckStyle::Pick);
        let mut left = [0.0; 2_048];
        let mut right = [0.0; 2_048];
        let events = [
            BassEvent::NoteOn {
                time: 17,
                note: 28,
                velocity: 0.9,
            },
            BassEvent::NoteOff {
                time: 900,
                note: 28,
            },
        ];
        engine.process_block(2_048, &events, &mut left, &mut right);
        assert!(left.iter().all(|sample| sample.is_finite()));
        assert!(left.iter().skip(17).any(|sample| sample.abs() > 1e-9));
        assert!(engine.total_energy().is_finite());
    }
}
