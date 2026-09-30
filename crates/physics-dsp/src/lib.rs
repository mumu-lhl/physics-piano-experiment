//! Small, allocation-free DSP primitives shared by the physical-modeling instruments.
//!
//! The types in this crate are deliberately independent of either instrument so that
//! filters and deterministic signal sources keep the same implementation everywhere.

mod biquad;
mod noise;

pub use biquad::Biquad;
pub use noise::{XorShift32, XorShift64};
