//! Small, allocation-free DSP primitives shared by the physical-modeling instruments.
//!
//! The types in this crate are deliberately independent of either instrument so that
//! filters and deterministic signal sources keep the same implementation everywhere.

mod biquad;
mod fx;
mod midi;
mod modal;
mod noise;

pub use biquad::Biquad;
pub use fx::{EQ_HIGH_FREQ, EQ_LOW_FREQ, EQ_MID_FREQ, EQ_MID_Q, FxChain, Reverb};
pub use midi::{
    BASS_BEND_RANGE_SEMITONES, GUITAR_BEND_RANGE_SEMITONES, PIANO_BEND_RANGE_CENTS, SOSTENUTO_CC,
    bend_cents, bend_semitones, cc_pedal_down,
};
pub use modal::{ModalTransition, OverdampedPolicy};
pub use noise::{XorShift32, XorShift64};

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

/// Flush a subnormal (denormal) value to zero, leaving every other value
/// (including zero, NaN and infinities) unchanged.
///
/// Decaying filter/comb states pass through the denormal range on long
/// quiet tails; without FTZ/DAZ (or on hosts that clear it) those tiny
/// values cost disproportionate CPU in the multiply chain. Flushing the
/// stored state on every sample keeps the tail exactly silent instead of
/// merely inaudible.
#[inline]
pub(crate) fn flush_denormal(value: f64) -> f64 {
    if value.is_subnormal() { 0.0 } else { value }
}

#[cfg(test)]
mod tests {
    use super::flush_denormal;

    #[test]
    fn flush_denormal_only_touches_subnormals() {
        assert_eq!(flush_denormal(f64::MIN_POSITIVE / 2.0), 0.0);
        assert_eq!(flush_denormal(-f64::MIN_POSITIVE / 2.0), 0.0);
        assert_eq!(flush_denormal(0.0), 0.0);
        assert_eq!(flush_denormal(-0.0), -0.0);
        // The smallest normal stays untouched, as do ordinary values.
        assert_eq!(flush_denormal(f64::MIN_POSITIVE), f64::MIN_POSITIVE);
        assert_eq!(flush_denormal(1e-300), 1e-300);
        assert_eq!(flush_denormal(1.0), 1.0);
        assert_eq!(flush_denormal(-42.5), -42.5);
        assert!(flush_denormal(f64::NAN).is_nan());
        assert_eq!(flush_denormal(f64::INFINITY), f64::INFINITY);
    }
}
