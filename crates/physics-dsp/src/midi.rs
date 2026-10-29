//! Shared MIDI controller helpers: pitch-bend conversion and pedal thresholds.
//!
//! All three melodic instruments convert the normalized pitch-bend value
//! (`0.0..=1.0`, center `0.5`) with the same formula; only the range differs
//! per instrument, so the ranges live here next to the conversion instead of
//! being re-derived inline in each plugin. Ranges are the values the plugins
//! used before this module existed — unifying the code, not the feel:
//! piano bends ±2 semitones, guitar and bass bend ±12.

/// Sostenuto (middle) pedal controller number.
pub const SOSTENUTO_CC: u8 = 66;

/// Piano pitch-bend range: ±2 semitones = ±200 cents.
pub const PIANO_BEND_RANGE_CENTS: f64 = 200.0;
/// Guitar pitch-bend range: ±12 semitones (whammy range).
pub const GUITAR_BEND_RANGE_SEMITONES: f64 = 12.0;
/// Bass pitch-bend range: ±12 semitones.
pub const BASS_BEND_RANGE_SEMITONES: f64 = 12.0;

/// Normalized bend value → cents, for the given ±range in cents.
/// The input is clamped to `0.0..=1.0`, so out-of-range hosts can't push
/// the bend past the declared range.
pub fn bend_cents(normalized: f64, range_cents: f64) -> f64 {
    (normalized.clamp(0.0, 1.0) - 0.5) * 2.0 * range_cents
}

/// Normalized bend value → semitones, for the given ±range in semitones.
pub fn bend_semitones(normalized: f64, range_semitones: f64) -> f64 {
    (normalized.clamp(0.0, 1.0) - 0.5) * 2.0 * range_semitones
}

/// Whether a normalized CC value counts as pedal-down. Mirrors the piano
/// sustain pedal's existing `depth > 0.05` edge so both pedals agree.
pub fn cc_pedal_down(value: f64) -> bool {
    value > 0.05
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bend_center_is_zero() {
        assert_eq!(bend_cents(0.5, PIANO_BEND_RANGE_CENTS), 0.0);
        assert_eq!(
            bend_semitones(0.5, GUITAR_BEND_RANGE_SEMITONES),
            0.0
        );
    }

    #[test]
    fn bend_ends_hit_the_range() {
        assert_eq!(bend_cents(0.0, PIANO_BEND_RANGE_CENTS), -200.0);
        assert_eq!(bend_cents(1.0, PIANO_BEND_RANGE_CENTS), 200.0);
        assert_eq!(
            bend_semitones(0.0, BASS_BEND_RANGE_SEMITONES),
            -12.0
        );
        assert_eq!(bend_semitones(1.0, BASS_BEND_RANGE_SEMITONES), 12.0);
    }

    #[test]
    fn bend_clamps_out_of_range_input() {
        assert_eq!(bend_cents(1.5, PIANO_BEND_RANGE_CENTS), 200.0);
        assert_eq!(bend_semitones(-0.5, GUITAR_BEND_RANGE_SEMITONES), -12.0);
    }

    #[test]
    fn pedal_down_threshold_matches_sustain() {
        assert!(!cc_pedal_down(0.0));
        assert!(!cc_pedal_down(0.05));
        assert!(cc_pedal_down(0.06));
        assert!(cc_pedal_down(1.0));
    }
}
