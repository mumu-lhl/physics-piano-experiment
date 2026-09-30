//! Reduced-order circular drum-head model.
//!
//! The mode ratios are the first circular-membrane Bessel roots used in the
//! design document.  Each mode is an exact damped state transition, while the
//! strike and cavity terms remain local and deterministic.

use physics_dsp::{ModalTransition, OverdampedPolicy};
use std::f64::consts::PI;

pub const HEAD_MODE_COUNT: usize = 8;

const MEMBRANE_MODE_RATIOS: [f64; HEAD_MODE_COUNT] =
    [1.0, 1.593, 2.135, 2.295, 2.917, 3.155, 3.501, 3.634];

#[derive(Debug, Clone, Copy)]
struct HeadMode {
    transition: ModalTransition,
    q: f64,
    v: f64,
    frequency: f64,
    shape_gain: f64,
}

impl HeadMode {
    fn new(sample_rate: f64, frequency: f64, t60: f64, shape_gain: f64) -> Self {
        let sigma = (1000.0_f64.ln() / t60.max(0.03)).min(2.0 * PI * frequency * 0.9);
        Self {
            transition: ModalTransition::new(
                2.0 * PI * frequency,
                sigma,
                1.0 / sample_rate.max(1.0),
                OverdampedPolicy::ExponentialFallback,
            ),
            q: 0.0,
            v: 0.0,
            frequency,
            shape_gain,
        }
    }

    #[inline]
    fn step(&mut self, force: f64, nonlinear_amount: f64) -> (f64, f64) {
        let (p11, p12, p21, p22) = self.transition.phi;
        let (g1, g2) = self.transition.gamma;
        // A cubic restoring perturbation is an explicit reduced von-Kármán term.
        let nonlinear_force = -nonlinear_amount * self.q * self.q * self.q;
        let total_force = (force + nonlinear_force).clamp(-1.0e5, 1.0e5);
        let next_q = p11 * self.q + p12 * self.v + g1 * total_force;
        let next_v = p21 * self.q + p22 * self.v + g2 * total_force;
        self.q = next_q.clamp(-10.0, 10.0);
        self.v = next_v.clamp(-10.0, 10.0);
        (self.q * self.shape_gain, self.v * self.shape_gain)
    }
}

/// A modal circular membrane with a fixed number of Bessel modes.
#[derive(Debug, Clone)]
pub struct MembraneHead {
    sample_rate: f64,
    pub fundamental_hz: f64,
    pub t60: f64,
    modes: [HeadMode; HEAD_MODE_COUNT],
    strike_position: f64,
    last_displacement: f64,
    last_velocity: f64,
    pub geometry_nonlinearity: f64,
}

impl MembraneHead {
    pub fn new(sample_rate: f64, fundamental_hz: f64, t60: f64) -> Self {
        let f0 = fundamental_hz.max(20.0);
        let modes = std::array::from_fn(|index| {
            let frequency = f0 * MEMBRANE_MODE_RATIOS[index];
            let gain = if index == 0 {
                1.0
            } else {
                0.60 / (1.0 + index as f64 * 0.16)
            };
            HeadMode::new(
                sample_rate,
                frequency,
                t60 / (1.0 + index as f64 * 0.16),
                gain,
            )
        });
        Self {
            sample_rate: sample_rate.max(1.0),
            fundamental_hz: f0,
            t60: t60.max(0.03),
            modes,
            strike_position: 0.5,
            last_displacement: 0.0,
            last_velocity: 0.0,
            geometry_nonlinearity: 0.0,
        }
    }

    pub fn set_strike_position(&mut self, position: f64) {
        self.strike_position = position.clamp(0.05, 0.95);
    }

    /// Excites the modes using their radial/modal coupling at the strike point.
    pub fn excite(&mut self, impulse: f64) {
        let impulse = impulse.clamp(-100.0, 100.0);
        for (index, mode) in self.modes.iter_mut().enumerate() {
            let radial_shape = if index == 0 {
                1.0
            } else {
                (PI * self.strike_position * (index as f64 + 0.5)).sin()
            };
            mode.v += impulse * radial_shape * mode.shape_gain * 0.015;
        }
    }

    /// Advances all modes with a force density and returns displacement/velocity.
    #[inline]
    pub fn step(&mut self, force: f64) -> (f64, f64) {
        let mut displacement = 0.0;
        let mut velocity = 0.0;
        for mode in &mut self.modes {
            let (q, v) = mode.step(force, self.geometry_nonlinearity);
            displacement += q;
            velocity += v;
        }
        self.last_velocity = (velocity * 0.18).clamp(-10.0, 10.0);
        self.last_displacement = (displacement * 0.18).clamp(-10.0, 10.0);
        (self.last_displacement, self.last_velocity)
    }

    pub fn displacement(&self) -> f64 {
        self.last_displacement
    }

    pub fn velocity(&self) -> f64 {
        self.last_velocity
    }

    pub fn mode_frequencies(&self) -> [f64; HEAD_MODE_COUNT] {
        self.modes.map(|mode| mode.frequency)
    }

    pub fn energy(&self) -> f64 {
        self.modes
            .iter()
            .map(|mode| mode.q * mode.q * mode.frequency * mode.frequency + mode.v * mode.v)
            .sum::<f64>()
            .min(1.0e12)
    }

    pub fn reset(&mut self) {
        for mode in &mut self.modes {
            mode.q = 0.0;
            mode.v = 0.0;
        }
        self.last_displacement = 0.0;
        self.last_velocity = 0.0;
    }

    pub fn sample_rate(&self) -> f64 {
        self.sample_rate
    }
}

#[cfg(test)]
mod tests {
    use super::MembraneHead;

    #[test]
    fn modes_follow_circular_membrane_ratios() {
        let head = MembraneHead::new(48_000.0, 200.0, 0.3);
        let modes = head.mode_frequencies();
        assert!((modes[1] / modes[0] - 1.593).abs() < 0.01);
        assert!(modes.iter().all(|frequency| frequency.is_finite()));
    }

    #[test]
    fn nonlinear_head_remains_bounded() {
        let mut head = MembraneHead::new(48_000.0, 60.0, 0.6);
        head.geometry_nonlinearity = 0.04;
        head.excite(40.0);
        for _ in 0..24_000 {
            let (displacement, velocity) = head.step(0.0);
            assert!(displacement.is_finite() && velocity.is_finite());
        }
        assert!(head.energy().is_finite());
    }
}
