//! Drum-specific physical modules exposed under the conventional `core` seam.

pub use crate::contact::HuntCrossleyExciter;
pub use crate::membrane::{HEAD_MODE_COUNT, MembraneHead};
pub use crate::voices::{CymbalVoice, DoubleHeadVoice, KickVoice, SnareVoice, TomVoice};
