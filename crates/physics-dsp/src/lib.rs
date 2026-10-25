//! Small, allocation-free DSP primitives shared by the physical-modeling instruments.
//!
//! The types in this crate are deliberately independent of either instrument so that
//! filters and deterministic signal sources keep the same implementation everywhere.

mod biquad;
mod fx;
mod modal;
mod noise;

pub use biquad::Biquad;
pub use fx::{EQ_HIGH_FREQ, EQ_LOW_FREQ, EQ_MID_FREQ, EQ_MID_Q, FxChain, Reverb};
pub use modal::{ModalTransition, OverdampedPolicy};
pub use noise::{XorShift32, XorShift64};
