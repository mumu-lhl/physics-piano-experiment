/// Behavior when modal damping reaches or exceeds the natural angular frequency.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OverdampedPolicy {
    /// Use a bounded exponential transition suitable for heavily damped modes.
    ExponentialFallback,
    /// Preserve an underdamped-form transition by flooring its damped frequency.
    ClampDampedFrequency(f64),
}

/// Exact discrete-time state transition for a damped second-order mode.
///
/// `phi` maps `[position, velocity]` from one sample to the next. `gamma` maps a
/// constant force over the sample interval into the same state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModalTransition {
    pub phi: (f64, f64, f64, f64),
    pub gamma: (f64, f64),
}

impl ModalTransition {
    pub fn new(omega: f64, sigma: f64, dt: f64, overdamped: OverdampedPolicy) -> Self {
        let omega_d_sq = omega.powi(2) - sigma.powi(2);
        let decay = (-sigma * dt).exp();

        let omega_d = match overdamped {
            OverdampedPolicy::ExponentialFallback if omega_d_sq <= 0.0 => {
                return Self {
                    phi: (decay, dt * decay, 0.0, decay),
                    gamma: (0.0, dt),
                };
            }
            OverdampedPolicy::ExponentialFallback => omega_d_sq.sqrt(),
            OverdampedPolicy::ClampDampedFrequency(minimum) => omega_d_sq
                .max(minimum.abs().max(f64::EPSILON).powi(2))
                .sqrt(),
        };

        let cos_d = (omega_d * dt).cos();
        let sin_d = (omega_d * dt).sin();
        let phi11 = decay * (cos_d + (sigma / omega_d) * sin_d);
        let phi12 = decay * (sin_d / omega_d);
        let phi21 = -decay * (omega.powi(2) / omega_d) * sin_d;
        let phi22 = decay * (cos_d - (sigma / omega_d) * sin_d);
        let gamma1 = (1.0 - phi11) / omega.powi(2);
        let gamma2 = -phi21 / omega.powi(2);

        Self {
            phi: (phi11, phi12, phi21, phi22),
            gamma: (gamma1, gamma2),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ModalTransition, OverdampedPolicy};

    #[test]
    fn underdamped_transition_has_expected_state_and_force_coefficients() {
        let transition =
            ModalTransition::new(10.0, 1.0, 0.01, OverdampedPolicy::ExponentialFallback);
        let (phi11, phi12, phi21, phi22) = transition.phi;
        let (gamma1, gamma2) = transition.gamma;

        assert!(phi11.is_finite() && phi12.is_finite());
        assert!(phi21.is_finite() && phi22.is_finite());
        assert!((gamma1 - (1.0 - phi11) / 100.0).abs() < 1e-15);
        assert!((gamma2 + phi21 / 100.0).abs() < 1e-15);
        assert!((gamma2 - phi12).abs() < 1e-15);
    }

    #[test]
    fn overdamped_policies_preserve_their_distinct_fallbacks() {
        let fallback = ModalTransition::new(1.0, 2.0, 0.1, OverdampedPolicy::ExponentialFallback);
        assert_eq!(fallback.phi.2, 0.0);
        assert_eq!(fallback.gamma, (0.0, 0.1));

        let clamped =
            ModalTransition::new(1.0, 2.0, 0.1, OverdampedPolicy::ClampDampedFrequency(1e-3));
        assert!(clamped.phi.0.is_finite());
        assert!(clamped.phi.1.is_finite());
        assert!(clamped.gamma.0.is_finite());
        assert!(clamped.gamma.1.is_finite());
    }
}
