//! Electric guitar magnetic pickup spatial sampling and comb filtering.

use std::f64::consts::PI;
use crate::core::guitar_string::GuitarString;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PickupType {
    /// Single-coil pickup (bright, crisp, wide bandwidth)
    SingleCoil,
    /// Dual-coil Humbucker (fat, warm, cancel hum, comb-filtered)
    Humbucker,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PickupPosition {
    /// Near bridge (~0.08 L): biting, tight bass, rich in high harmonics
    Bridge,
    /// Middle position (~0.16 L): balanced scooped mids
    Middle,
    /// Near neck (~0.24 L): warm, singing sustain, full fundamental
    Neck,
}

#[derive(Debug, Clone)]
pub struct MagneticPickup {
    pub pickup_type: PickupType,
    pub position: PickupPosition,
    /// Normalized position on the string x_pu / L
    pub pos_ratio: f64,
    /// Magnetic pole spread sigma_w (meters)
    pub pole_width: f64,
    /// Dual pole spacing for humbucker d (meters, ~18mm)
    pub humbucker_spacing: f64,
}

impl MagneticPickup {
    pub fn new(pickup_type: PickupType, position: PickupPosition) -> Self {
        let pos_ratio = match position {
            PickupPosition::Bridge => 0.085,
            PickupPosition::Middle => 0.165,
            PickupPosition::Neck => 0.245,
        };
        Self {
            pickup_type,
            position,
            pos_ratio,
            pole_width: 0.0035, // 3.5 mm
            humbucker_spacing: 0.018, // 18 mm
        }
    }

    /// Evaluates the induced EMF voltage signal from a single guitar string.
    /// Velocity of string modes is converted to electrical signal via spatial filtering.
    pub fn sample_string(&self, string: &GuitarString) -> f64 {
        let length = string.effective_length;
        let x_pu = self.pos_ratio * string.params.scale_length;

        // If the fretted note puts the active string portion behind the pickup, output 0
        if x_pu > length {
            return 0.0;
        }

        let mut signal = 0.0;
        let sigma_w = self.pole_width;
        let d = self.humbucker_spacing;

        for m in 0..string.num_modes {
            let m_f = (m + 1) as f64;
            let k_m = m_f * PI / length;

            // Spatial sensitivity Gaussian roll-off
            let gauss_decay = (-0.5 * (k_m * sigma_w).powi(2)).exp();
            let spatial_factor = (k_m * x_pu).sin() * gauss_decay;

            let sensitivity = match self.pickup_type {
                PickupType::SingleCoil => spatial_factor,
                PickupType::Humbucker => {
                    // Humbucker dual-pole comb filter: sin(k_m * d / 2)
                    let comb = (k_m * d / 2.0).sin();
                    spatial_factor * comb * 2.0
                }
            };

            // Magnetic pickup captures string vertical velocity v_t primarily,
            // with slight coupling to horizontal velocity v_p (~15%)
            let v_eff = string.state_t[m].v + 0.15 * string.state_p[m].v;
            signal += v_eff * sensitivity;
        }

        signal
    }
}
