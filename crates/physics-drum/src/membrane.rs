//! Reduced-order circular drum-head model.
//!
//! The mode ratios are the first circular-membrane Bessel roots used in the
//! design document.  Each mode is an exact damped state transition, while the
//! strike and cavity terms remain local and deterministic.

use physics_dsp::{ModalTransition, OverdampedPolicy};
use std::f64::consts::PI;

pub const HEAD_MODE_COUNT: usize = 32;

/// `(angular order m, radial index n, J_m zero)` for the first useful
/// circular-membrane modes. Keeping the roots explicit avoids a runtime root
/// search while retaining the actual Bessel spectrum from the design.
const MEMBRANE_MODE_SPECS: [(u8, u8, f64); HEAD_MODE_COUNT] = [
    (0, 1, 2.404_825_557_7),
    (1, 1, 3.831_705_970_2),
    (2, 1, 5.135_622_301_8),
    (0, 2, 5.520_078_110_3),
    (3, 1, 6.380_161_895_9),
    (1, 2, 7.015_586_669_8),
    (4, 1, 7.588_342_434_5),
    (2, 2, 8.417_244_140_4),
    (0, 3, 8.653_727_912_9),
    (5, 1, 8.771_483_816_0),
    (3, 2, 9.761_023_130_0),
    (6, 1, 9.936_109_524_2),
    (1, 3, 10.173_468_135_1),
    (4, 2, 11.064_709_488_5),
    (7, 1, 11.086_370_019_2),
    (2, 3, 11.619_841_172_1),
    (0, 4, 11.791_534_439_0),
    (8, 1, 12.225_092_264_0),
    (5, 2, 12.338_604_197_5),
    (3, 3, 13.015_200_721_7),
    (1, 4, 13.323_691_936_3),
    (9, 1, 13.354_300_477_4),
    (6, 2, 13.589_290_170_5),
    (4, 3, 14.372_536_671_6),
    (10, 1, 14.475_500_686_6),
    (2, 4, 14.795_951_782_4),
    (7, 2, 14.821_268_727_0),
    (0, 5, 14.930_917_708_5),
    (11, 1, 15.589_847_884_5),
    (5, 3, 15.700_174_079_7),
    (8, 2, 16.037_774_190_9),
    (3, 4, 16.223_466_160_3),
];

#[derive(Debug, Clone, Copy)]
struct HeadMode {
    transition: ModalTransition,
    q: f64,
    v: f64,
    frequency: f64,
    shape_gain: f64,
    angular_order: u8,
    radial_root: f64,
    strike_shape: f64,
    sensor_shape: f64,
    area_average_shape: f64,
}

impl HeadMode {
    fn new(
        sample_rate: f64,
        frequency: f64,
        t60: f64,
        shape_gain: f64,
        angular_order: u8,
        radial_root: f64,
    ) -> Self {
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
            angular_order,
            radial_root,
            strike_shape: bessel_j(angular_order, radial_root * 0.5) * shape_gain,
            sensor_shape: bessel_j(angular_order, radial_root * 0.42) * shape_gain,
            area_average_shape: if angular_order == 0 {
                2.0 * bessel_j(1, radial_root) / radial_root * shape_gain
            } else {
                0.0
            },
        }
    }

    fn set_t60(&mut self, sample_rate: f64, t60: f64) {
        let sigma = (1000.0_f64.ln() / t60.max(0.03)).min(2.0 * PI * self.frequency * 0.9);
        self.transition = ModalTransition::new(
            2.0 * PI * self.frequency,
            sigma,
            1.0 / sample_rate.max(1.0),
            OverdampedPolicy::ExponentialFallback,
        );
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
        (self.q, self.v)
    }
}

/// A modal circular membrane with a fixed number of Bessel modes.
#[derive(Debug, Clone)]
pub struct MembraneHead {
    sample_rate: f64,
    modal_mass_kg: f64,
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
        let fundamental_root = MEMBRANE_MODE_SPECS[0].2;
        let modes = std::array::from_fn(|index| {
            let (angular_order, _radial_index, radial_root) = MEMBRANE_MODE_SPECS[index];
            let frequency = f0 * radial_root / fundamental_root;
            let gain = if index == 0 {
                1.0
            } else {
                0.60 / (1.0 + index as f64 * 0.16)
            };
            HeadMode::new(
                sample_rate,
                frequency,
                modal_t60(t60, index),
                gain,
                angular_order,
                radial_root,
            )
        });
        Self {
            sample_rate: sample_rate.max(1.0),
            // Effective modal mass for a standard polyester drum head.
            modal_mass_kg: 0.012,
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
        for mode in &mut self.modes {
            mode.strike_shape =
                bessel_j(mode.angular_order, mode.radial_root * self.strike_position)
                    * mode.shape_gain;
        }
    }

    /// Updates the membrane damping without disturbing current modal state.
    pub fn set_t60(&mut self, t60: f64) {
        self.t60 = t60.max(0.03);
        for (index, mode) in self.modes.iter_mut().enumerate() {
            mode.set_t60(self.sample_rate, modal_t60(self.t60, index));
        }
    }

    /// Excites the modes using their radial/modal coupling at the strike point.
    pub fn excite(&mut self, impulse: f64) {
        let impulse = impulse.clamp(-100.0, 100.0);
        for mode in &mut self.modes {
            // The strike is at theta=0, so angular coupling is one and radial
            // coupling is J_m(alpha_mn r/a).
            mode.v += impulse / self.modal_mass_kg * mode.strike_shape * 0.015;
        }
    }

    /// Advances point excitation and returns displacement/velocity.
    #[inline]
    pub fn step(&mut self, force: f64) -> (f64, f64) {
        self.step_coupled(force, 0.0)
    }

    /// Advances point excitation plus a spatially uniform acoustic-cavity load.
    /// The latter projects only onto axisymmetric modes through their area
    /// integral, as required by circular membrane orthogonality.
    #[inline]
    pub fn step_coupled(&mut self, point_force: f64, uniform_pressure: f64) -> (f64, f64) {
        self.step_with_modal_forces(point_force, uniform_pressure, &[0.0; HEAD_MODE_COUNT])
    }

    /// Adds externally computed point-contact modal forces, used for the
    /// spatially distributed snare-wire reactions.
    #[inline]
    pub fn step_with_modal_forces(
        &mut self,
        point_force: f64,
        uniform_pressure: f64,
        additional_forces: &[f64; HEAD_MODE_COUNT],
    ) -> (f64, f64) {
        for (index, mode) in self.modes.iter_mut().enumerate() {
            let modal_force = point_force * mode.strike_shape
                + uniform_pressure * mode.area_average_shape
                + additional_forces[index];
            mode.step(modal_force / self.modal_mass_kg, self.geometry_nonlinearity);
        }
        let mut displacement = 0.0;
        let mut velocity = 0.0;
        for mode in &self.modes {
            displacement += mode.q * mode.sensor_shape;
            velocity += mode.v * mode.sensor_shape;
        }
        self.last_velocity = (velocity * 0.18).clamp(-10.0, 10.0);
        self.last_displacement = (displacement * 0.18).clamp(-10.0, 10.0);
        (self.last_displacement, self.last_velocity)
    }

    /// Projects a localized contact force into all fixed modal coordinates.
    pub fn shapes_at_radius(&self, radius: f64) -> [f64; HEAD_MODE_COUNT] {
        let radius = radius.clamp(0.0, 1.0);
        std::array::from_fn(|index| {
            let mode = self.modes[index];
            mode.shape_gain * bessel_j(mode.angular_order, mode.radial_root * radius)
        })
    }

    pub fn accumulate_modal_force(
        &self,
        shapes: &[f64; HEAD_MODE_COUNT],
        force: f64,
        modal_forces: &mut [f64; HEAD_MODE_COUNT],
    ) {
        for (mode_force, shape) in modal_forces.iter_mut().zip(shapes.iter()) {
            *mode_force += force * shape;
        }
    }

    pub fn state_with_shapes(&self, shapes: &[f64; HEAD_MODE_COUNT]) -> (f64, f64) {
        let mut displacement = 0.0;
        let mut velocity = 0.0;
        for (mode, shape) in self.modes.iter().zip(shapes) {
            displacement += mode.q * shape;
            velocity += mode.v * shape;
        }
        (displacement * 0.18, velocity * 0.18)
    }

    pub fn displacement(&self) -> f64 {
        self.last_displacement
    }

    pub fn velocity(&self) -> f64 {
        self.last_velocity
    }

    /// Membrane displacement and velocity at a radial contact point.
    pub fn state_at_radius(&self, radius: f64) -> (f64, f64) {
        let shapes = self.shapes_at_radius(radius);
        self.state_with_shapes(&shapes)
    }

    /// Area-average displacement/velocity; non-axisymmetric modes integrate
    /// to zero and only m=0 modes compress the enclosed air volume.
    pub fn area_average_state(&self) -> (f64, f64) {
        let mut displacement = 0.0;
        let mut velocity = 0.0;
        for mode in &self.modes {
            displacement += mode.q * mode.area_average_shape;
            velocity += mode.v * mode.area_average_shape;
        }
        (displacement * 0.18, velocity * 0.18)
    }

    pub fn strike_state(&self) -> (f64, f64) {
        let mut displacement = 0.0;
        let mut velocity = 0.0;
        for mode in &self.modes {
            displacement += mode.q * mode.strike_shape;
            velocity += mode.v * mode.strike_shape;
        }
        (displacement * 0.18, velocity * 0.18)
    }

    pub fn mode_frequencies(&self) -> [f64; HEAD_MODE_COUNT] {
        self.modes.map(|mode| mode.frequency)
    }

    pub fn energy(&self) -> f64 {
        self.modes
            .iter()
            .map(|mode| {
                0.5 * self.modal_mass_kg
                    * (mode.q * mode.q * mode.frequency * mode.frequency + mode.v * mode.v)
            })
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

#[inline]
fn modal_t60(base_t60: f64, index: usize) -> f64 {
    (base_t60 / (1.0 + index as f64 * 0.4)).max(0.03)
}

/// Integer-order Bessel J_m evaluated by its convergent power series. The
/// fixed 32-term loop is deterministic and allocation-free on the audio path.
#[inline]
fn bessel_j(order: u8, x: f64) -> f64 {
    let order = order as usize;
    let half = x * 0.5;
    let mut term = half.powi(order as i32) / factorial(order);
    let mut sum = term;
    let half_squared = half * half;
    for k in 0..32 {
        let denominator = (k + 1) as f64 * (k + order + 1) as f64;
        term *= -half_squared / denominator;
        sum += term;
    }
    sum
}

#[inline]
fn factorial(n: usize) -> f64 {
    let mut value = 1.0;
    for i in 2..=n {
        value *= i as f64;
    }
    value
}

#[cfg(test)]
mod tests {
    use super::{MEMBRANE_MODE_SPECS, MembraneHead, bessel_j};

    #[test]
    fn modes_follow_circular_membrane_ratios() {
        for (order, _, root) in MEMBRANE_MODE_SPECS {
            assert!(bessel_j(order, root).abs() < 1e-7);
        }
        let head = MembraneHead::new(48_000.0, 200.0, 0.3);
        let modes = head.mode_frequencies();
        assert!((modes[1] / modes[0] - 1.593).abs() < 0.01);
        assert!(modes.iter().all(|frequency| frequency.is_finite()));
    }

    #[test]
    fn uniform_cavity_pressure_only_excites_axisymmetric_modes() {
        let mut head = MembraneHead::new(48_000.0, 200.0, 0.3);
        head.step_coupled(0.0, 1.0);
        assert!(head.modes[0].q.abs() > 0.0);
        assert!(
            head.modes
                .iter()
                .filter(|mode| mode.angular_order != 0)
                .all(|mode| mode.q == 0.0)
        );
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
