//! Reduced-order circular drum-head model.
//!
//! The mode ratios are the circular-membrane Bessel roots. Each mode follows an
//! exact damped state transition, with Avanzini geometric nonlinear tension
//! modulation, normalized physical modal masses, and dipole/multipole acoustic
//! radiation cancellation.

use physics_dsp::{ModalTransition, OverdampedPolicy};
use std::f64::consts::PI;

pub const HEAD_MODE_COUNT: usize = 59;
const MEMBRANE_ROOT_COUNT: usize = 32;

/// `(angular order m, radial index n, J_m zero)` for the first useful
/// circular-membrane modes. Keeping the roots explicit avoids a runtime root
/// search while retaining the actual Bessel spectrum from the design.
const MEMBRANE_MODE_SPECS: [(u8, u8, f64); MEMBRANE_ROOT_COUNT] = [
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
pub struct HeadMode {
    pub transition: ModalTransition,
    pub q: f64,
    pub v: f64,
    pub base_omega: f64,
    pub frequency: f64,
    pub t60: f64,
    pub modal_mass_kg: f64,
    pub angular_order: u8,
    pub radial_root: f64,
    pub sine_orientation: bool,
    pub strike_shape: f64,
    pub area_average_shape: f64,
    pub radiation_shape: f64,
}

impl HeadMode {
    pub fn new(
        sample_rate: f64,
        frequency: f64,
        t60: f64,
        radius_m: f64,
        contact_patch_radius_m: f64,
        surface_density_kg_m2: f64,
        angular_order: u8,
        radial_root: f64,
        sine_orientation: bool,
    ) -> Self {
        let omega = 2.0 * PI * frequency;
        let sigma = (1000.0_f64.ln() / t60.max(0.02)).min(omega * 0.9);

        let total_head_mass = surface_density_kg_m2 * PI * radius_m * radius_m;
        // Normalized modal mass: For a circular membrane clamped at boundary,
        // m=0 modes occupy ~25% of head mass, m>0 modes occupy ~15%.
        let modal_mass_kg = if angular_order == 0 {
            total_head_mass * 0.25
        } else {
            total_head_mass * 0.15
        };

        let area_average_shape = if angular_order == 0 && !sine_orientation {
            2.0 * bessel_j(1, radial_root) / radial_root
        } else {
            0.0
        };

        // Acoustic radiation efficiency:
        // Axisymmetric modes (m=0) generate net volume displacement (monopole).
        // Non-axisymmetric modes (m>0) cancel out in far field (dipole/multipole).
        let alpha0 = MEMBRANE_MODE_SPECS[0].2;
        let radiation_shape = if angular_order == 0 {
            1.0 / (1.0 + (radial_root / alpha0 - 1.0) * 0.4)
        } else {
            0.22 / (1.0 + angular_order as f64 * 0.7 + (radial_root / alpha0) * 0.25)
        };

        let patch_filter = (-0.5 * (radial_root * contact_patch_radius_m / radius_m).powi(2)).exp();
        let initial_strike_shape = mode_shape(angular_order, radial_root, sine_orientation, 0.5, 0.0) * patch_filter;

        Self {
            transition: ModalTransition::new(
                omega,
                sigma,
                1.0 / sample_rate.max(1.0),
                OverdampedPolicy::ExponentialFallback,
            ),
            q: 0.0,
            v: 0.0,
            base_omega: omega,
            frequency,
            t60,
            modal_mass_kg: modal_mass_kg.max(1e-9),
            angular_order,
            radial_root,
            sine_orientation,
            strike_shape: initial_strike_shape,
            area_average_shape,
            radiation_shape,
        }
    }

    #[inline]
    pub fn shape_at(&self, radius: f64, angle: f64) -> f64 {
        mode_shape(
            self.angular_order,
            self.radial_root,
            self.sine_orientation,
            radius,
            angle,
        )
    }

    pub fn set_t60(&mut self, sample_rate: f64, t60: f64) {
        self.t60 = t60.max(0.02);
        let sigma = (1000.0_f64.ln() / self.t60).min(self.base_omega * 0.9);
        self.transition = ModalTransition::new(
            self.base_omega,
            sigma,
            1.0 / sample_rate.max(1.0),
            OverdampedPolicy::ExponentialFallback,
        );
    }
}

/// A modal circular membrane with a fixed number of Bessel modes.
#[derive(Debug, Clone)]
pub struct MembraneHead {
    sample_rate: f64,
    dt: f64,
    radius_m: f64,
    pub fundamental_hz: f64,
    pub t60: f64,
    pub modes: [HeadMode; HEAD_MODE_COUNT],
    strike_position: f64,
    strike_angle: f64,
    contact_patch_radius_m: f64,
    last_displacement: f64,
    last_velocity: f64,
    pub geometry_nonlinearity: f64,
    pub tension_modulation_gain: f64,
    pub smoothed_energy: f64,
    pub current_scale: f64,
    step_counter: usize,
}

impl MembraneHead {
    pub fn new(sample_rate: f64, fundamental_hz: f64, t60: f64) -> Self {
        Self::new_with_geometry(sample_rate, fundamental_hz, t60, 0.1778, 0.30)
    }

    pub fn new_with_geometry(
        sample_rate: f64,
        fundamental_hz: f64,
        t60: f64,
        radius_m: f64,
        surface_density_kg_m2: f64,
    ) -> Self {
        let sample_rate = sample_rate.max(1.0);
        let dt = 1.0 / sample_rate;
        let radius_m = radius_m.max(0.02);
        let contact_patch_radius_m = 0.015;
        let surface_density_kg_m2 = surface_density_kg_m2.max(0.02);
        let f0 = fundamental_hz.max(20.0);
        let fundamental_root = MEMBRANE_MODE_SPECS[0].2;

        let modes = std::array::from_fn(|index| {
            let (angular_order, _radial_index, radial_root, sine_orientation) =
                expanded_mode_spec(index);
            let frequency = f0 * radial_root / fundamental_root;
            // Physical viscoelastic damping: Mylar drumhead loss increases with frequency
            let mode_t60 = (t60 / (1.0 + (frequency / 220.0).powf(1.6)) + 0.03).max(0.02);
            HeadMode::new(
                sample_rate,
                frequency,
                mode_t60,
                radius_m,
                contact_patch_radius_m,
                surface_density_kg_m2,
                angular_order,
                radial_root,
                sine_orientation,
            )
        });

        let mut head = Self {
            sample_rate,
            dt,
            radius_m,
            fundamental_hz: f0,
            t60: t60.max(0.03),
            modes,
            strike_position: 0.35,
            strike_angle: 0.0,
            contact_patch_radius_m,
            last_displacement: 0.0,
            last_velocity: 0.0,
            geometry_nonlinearity: 0.0,
            tension_modulation_gain: 12.0,
            smoothed_energy: 0.0,
            current_scale: 1.0,
            step_counter: 0,
        };
        head.set_strike_point(0.35, 0.0);
        head
    }

    pub fn set_strike_position(&mut self, position: f64) {
        self.set_strike_point(position, 0.0);
    }

    pub fn set_strike_point(&mut self, radius: f64, angle: f64) {
        self.strike_position = radius.clamp(0.01, 0.95);
        self.strike_angle = angle;
        for mode in &mut self.modes {
            let shape = mode.shape_at(self.strike_position, angle);
            let patch_filter = (-0.5 * (mode.radial_root * self.contact_patch_radius_m / self.radius_m).powi(2)).exp();
            mode.strike_shape = shape * patch_filter;
        }
    }

    /// Sets the standard deviation of the Gaussian mallet contact patch.
    pub fn set_strike_contact_radius(&mut self, radius_m: f64) {
        self.contact_patch_radius_m = radius_m.clamp(0.001, self.radius_m * 0.25);
        self.set_strike_point(self.strike_position, self.strike_angle);
    }

    /// Updates the membrane damping without disturbing current modal state.
    pub fn set_t60(&mut self, t60: f64) {
        self.t60 = t60.max(0.03);
        for mode in &mut self.modes {
            let mode_t60 = (self.t60 / (1.0 + (mode.frequency / 220.0).powf(1.6)) + 0.03).max(0.02);
            mode.set_t60(self.sample_rate, mode_t60);
        }
    }

    /// Excites the modes using their radial/modal coupling at the strike point.
    pub fn excite(&mut self, impulse: f64) {
        let impulse = impulse.clamp(-500.0, 500.0);
        for mode in &mut self.modes {
            mode.v += (impulse / mode.modal_mass_kg) * mode.strike_shape * 0.015;
        }
    }

    /// Advances point excitation and returns displacement/velocity.
    #[inline]
    pub fn step(&mut self, force: f64) -> (f64, f64) {
        self.step_coupled(force, 0.0)
    }

    /// Advances point excitation plus a spatially uniform acoustic-cavity load.
    #[inline]
    pub fn step_coupled(&mut self, point_force: f64, uniform_pressure: f64) -> (f64, f64) {
        self.step_with_modal_forces(point_force, uniform_pressure, &[0.0; HEAD_MODE_COUNT])
    }

    /// Step modal coordinates with point strike, cavity uniform pressure, and additional modal forces.
    #[inline]
    pub fn step_with_modal_forces(
        &mut self,
        point_force: f64,
        uniform_pressure: f64,
        additional_forces: &[f64; HEAD_MODE_COUNT],
    ) -> (f64, f64) {
        // Instantaneous mechanical energy for Avanzini tension modulation
        let mut instant_energy = 0.0;
        for mode in &self.modes {
            instant_energy += 0.5 * mode.modal_mass_kg
                * (mode.v * mode.v + mode.base_omega * mode.base_omega * mode.q * mode.q);
        }

        // Smooth energy with ~3ms time constant
        let alpha = (-self.dt / 0.003).exp();
        self.smoothed_energy = alpha * self.smoothed_energy + (1.0 - alpha) * instant_energy;

        // Sub-block control rate (every 16 samples) update frequency scale
        if self.step_counter % 16 == 0 {
            let gain = if self.geometry_nonlinearity > 0.0 {
                self.geometry_nonlinearity * 400.0
            } else {
                self.tension_modulation_gain
            };
            if gain > 0.0 && self.smoothed_energy > 1e-4 {
                let target_scale = (1.0 + gain * self.smoothed_energy).sqrt().min(1.5);
                if (target_scale - self.current_scale).abs() > 0.005 {
                    self.current_scale = target_scale;
                    for mode in &mut self.modes {
                        let current_omega = mode.base_omega * self.current_scale;
                        let sigma = (1000.0_f64.ln() / mode.t60).min(current_omega * 0.9);
                        mode.transition = ModalTransition::new(
                            current_omega,
                            sigma,
                            self.dt,
                            OverdampedPolicy::ExponentialFallback,
                        );
                    }
                }
            } else if self.current_scale != 1.0 {
                self.current_scale = 1.0;
                for mode in &mut self.modes {
                    let current_omega = mode.base_omega;
                    let sigma = (1000.0_f64.ln() / mode.t60).min(current_omega * 0.9);
                    mode.transition = ModalTransition::new(
                        current_omega,
                        sigma,
                        self.dt,
                        OverdampedPolicy::ExponentialFallback,
                    );
                }
            }
        }
        self.step_counter += 1;

        let head_area = PI * self.radius_m * self.radius_m;
        let mut displacement = 0.0;
        let mut velocity = 0.0;

        for (index, mode) in self.modes.iter_mut().enumerate() {
            let (p11, p12, p21, p22) = mode.transition.phi;
            let (g1, g2) = mode.transition.gamma;

            let modal_force = point_force * mode.strike_shape
                + uniform_pressure * head_area * mode.area_average_shape
                + additional_forces[index];
            let force_over_m = modal_force / mode.modal_mass_kg;

            let next_q = p11 * mode.q + p12 * mode.v + g1 * force_over_m;
            let next_v = p21 * mode.q + p22 * mode.v + g2 * force_over_m;

            mode.q = next_q;
            mode.v = next_v;

            displacement += mode.q * mode.radiation_shape;
            velocity += mode.v * mode.radiation_shape;
        }

        self.last_displacement = displacement;
        self.last_velocity = velocity;
        (self.last_displacement, self.last_velocity)
    }

    /// Projects a localized contact force into all fixed modal coordinates.
    pub fn shapes_at_radius(&self, radius: f64) -> [f64; HEAD_MODE_COUNT] {
        self.shapes_at_polar(radius, 0.0)
    }

    pub fn shapes_at_polar(&self, radius: f64, angle: f64) -> [f64; HEAD_MODE_COUNT] {
        let radius = radius.clamp(0.0, 1.0);
        std::array::from_fn(|index| self.modes[index].shape_at(radius, angle))
    }

    pub fn shapes_at_xy(&self, x: f64, y: f64) -> [f64; HEAD_MODE_COUNT] {
        let radius = x.hypot(y).clamp(0.0, 1.0);
        self.shapes_at_polar(radius, y.atan2(x))
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
        (displacement, velocity)
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
        (displacement, velocity)
    }

    /// Effective modal mass for the area-average displacement coordinate.
    pub fn area_average_effective_mass(&self) -> f64 {
        let compliance = self
            .modes
            .iter()
            .map(|mode| mode.area_average_shape.powi(2) / mode.modal_mass_kg)
            .sum::<f64>();
        1.0 / compliance.max(1.0e-12)
    }

    pub fn strike_state(&self) -> (f64, f64) {
        let mut displacement = 0.0;
        let mut velocity = 0.0;
        for mode in &self.modes {
            displacement += mode.q * mode.strike_shape;
            velocity += mode.v * mode.strike_shape;
        }
        (displacement, velocity)
    }

    pub fn mode_frequencies(&self) -> [f64; HEAD_MODE_COUNT] {
        self.modes.map(|mode| mode.frequency)
    }

    pub fn energy(&self) -> f64 {
        self.modes
            .iter()
            .map(|mode| {
                0.5 * mode.modal_mass_kg
                    * (mode.q * mode.q * mode.base_omega * mode.base_omega + mode.v * mode.v)
            })
            .sum::<f64>()
            .min(1.0e12)
    }

    pub fn reset(&mut self) {
        for mode in &mut self.modes {
            mode.q = 0.0;
            mode.v = 0.0;
        }
        self.smoothed_energy = 0.0;
        self.current_scale = 1.0;
        self.step_counter = 0;
        self.last_displacement = 0.0;
        self.last_velocity = 0.0;
    }

    pub fn sample_rate(&self) -> f64 {
        self.sample_rate
    }

    pub fn radius_m(&self) -> f64 {
        self.radius_m
    }
}

fn expanded_mode_spec(index: usize) -> (u8, u8, f64, bool) {
    let mut expanded_index = 0;
    for (order, radial_index, root) in MEMBRANE_MODE_SPECS {
        if expanded_index == index {
            return (order, radial_index, root, false);
        }
        expanded_index += 1;
        if order > 0 {
            if expanded_index == index {
                return (order, radial_index, root, true);
            }
            expanded_index += 1;
        }
    }
    unreachable!("expanded membrane mode index out of range")
}

#[inline]
fn mode_shape(order: u8, root: f64, sine_orientation: bool, radius: f64, angle: f64) -> f64 {
    let angular = if order == 0 {
        1.0
    } else if sine_orientation {
        (order as f64 * angle).sin()
    } else {
        (order as f64 * angle).cos()
    };
    bessel_j(order, root * radius) * angular
}

/// Integer-order Bessel J_m evaluated by its convergent power series.
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
    use super::{MEMBRANE_MODE_SPECS, MembraneHead, PI, bessel_j};

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
    fn strike_location_and_gaussian_patch_control_modal_coupling() {
        let mut head = MembraneHead::new(48_000.0, 200.0, 0.3);
        assert_eq!(head.modes[2].strike_shape, 0.0);
        head.set_strike_point(0.43, PI / 2.0);
        let off_axis_strike = head.modes[2].strike_shape.abs();
        assert!(off_axis_strike > 0.01);

        head.set_strike_contact_radius(0.003);
        let narrow_patch_high_mode = head.modes[58].strike_shape.abs();
        head.set_strike_contact_radius(0.03);
        let broad_patch_high_mode = head.modes[58].strike_shape.abs();
        assert!(broad_patch_high_mode < narrow_patch_high_mode);
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
