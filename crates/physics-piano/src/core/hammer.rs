//! Hunt-Crossley and SAV (Scalar Auxiliary Variable) Nonlinear Hammer Contact Dynamics in Rust.

use crate::params::HammerPhysicalParams;

#[repr(C, align(64))]
#[derive(Debug, Clone)]
pub struct HuntCrossleyHammer {
    pub params: HammerPhysicalParams,
    pub sample_rate: f64,
    pub dt: f64,

    pub m_h: f64,
    pub k_h: f64,
    pub p: f64,
    pub lambda_h: f64,
    pub v_max: f64,
    pub gamma_felt: f64,

    // Dynamic states
    pub u_h: f64,
    pub v_h: f64,
    pub is_active: bool,
    pub contact_time: f64,
    pub has_struck: bool,

    // SAV (Scalar Auxiliary Variable) Energy Quadratisation state
    pub use_sav: bool,
    pub xi: f64,

    // Una Corda
    pub una_corda: bool,
}

impl HuntCrossleyHammer {
    pub fn new(params: HammerPhysicalParams, sample_rate: f64) -> Self {
        let dt = 1.0 / sample_rate;
        Self {
            m_h: params.mass,
            k_h: params.stiffness,
            p: params.exponent,
            lambda_h: params.dissipation,
            v_max: params.max_velocity,
            gamma_felt: params.velocity_gamma,
            params,
            sample_rate,
            dt,
            u_h: 0.0,
            v_h: 0.0,
            is_active: false,
            contact_time: 0.0,
            has_struck: false,
            use_sav: true,
            xi: 0.0,
            una_corda: false,
        }
    }

    pub fn set_una_corda(&mut self, enabled: bool) {
        self.una_corda = enabled;
    }

    pub fn strike(&mut self, velocity: f64, initial_u_string: f64) {
        let clamped_vel = velocity.clamp(0.001, 1.0);
        let v0 = self.v_max * clamped_vel.powf(self.gamma_felt);

        self.u_h = initial_u_string.min(0.0);
        self.v_h = v0;
        self.is_active = true;
        self.contact_time = 0.0;
        self.has_struck = false;
        self.xi = 0.0;
    }

    #[inline]
    pub fn compute_force(&mut self, u_string: f64, v_string: f64) -> f64 {
        if !self.is_active {
            return 0.0;
        }

        let eta = self.u_h - u_string;
        if eta <= 0.0 {
            self.xi = 0.0;
            if self.has_struck && self.v_h <= 0.0 {
                self.is_active = false;
            }
            return 0.0;
        }

        self.has_struck = true;
        self.contact_time += self.dt;

        let v_rel = self.v_h - v_string;
        let k_felt = if self.una_corda { self.k_h * 0.72 } else { self.k_h };
        let lambda_felt = if self.una_corda { self.lambda_h * 1.15 } else { self.lambda_h };

        let eta_p = eta.powf(self.p);
        let damping_term = lambda_felt * eta_p * v_rel;

        let elastic_term = if self.use_sav {
            let v_pot = (k_felt / (self.p + 1.0)) * eta.powf(self.p + 1.0);
            let exact_xi = v_pot.max(0.0).sqrt();
            if exact_xi > 1e-12 {
                let g_eta = (k_felt * eta_p) / (2.0 * exact_xi);
                self.xi = (self.xi + 0.5 * self.dt * g_eta * v_rel)
                    / (1.0 + 0.25 * self.dt.powi(2) * g_eta.powi(2) / self.m_h);
                self.xi = self.xi.max(0.0);
                2.0 * self.xi * g_eta
            } else {
                self.xi = 0.0;
                k_felt * eta_p
            }
        } else {
            k_felt * eta_p
        };

        (elastic_term + damping_term).max(0.0)
    }

    #[inline]
    pub fn advance(&mut self, force: f64) {
        if !self.is_active {
            return;
        }

        let acc = -force / self.m_h;
        self.u_h += self.v_h * self.dt + 0.5 * acc * self.dt.powi(2);
        self.v_h += acc * self.dt;

        if self.has_struck && self.u_h < 0.0 && self.v_h <= 0.0 {
            self.is_active = false;
        }
    }
}
