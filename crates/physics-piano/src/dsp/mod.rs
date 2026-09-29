pub mod lid;
pub mod upols;

pub use lid::{LidBaffle, LidPosition};
pub use upols::{
    MultiPerspectiveUPOLS, StereoIR, UPOLSConvolver, generate_multi_perspective_soundboard_irs,
    generate_orthotropic_soundboard_ir,
};
