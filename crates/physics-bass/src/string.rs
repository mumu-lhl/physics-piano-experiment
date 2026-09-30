//! One-dimensional stiff-string FDTD model for a bass string.
//!
//! The implementation deliberately keeps the working grid in fixed-size arrays.  A
//! host can prepare/reconfigure the string away from the callback; stepping the
//! model never grows or shrinks a collection and the unilateral fret contact is an
//! explicit, bounded force (there is no Newton iteration in the audio path).

use crate::params::BassStringParams;
use std::f64::consts::PI;

/// Maximum number of spatial samples supported by one prepared string.
pub const MAX_GRID_POINTS: usize = 256;
const MIN_SEGMENTS: usize = 8;
const CONTACT_ALPHA: f64 = 1.35;

/// Cache-line aligned fixed grid used by the three time levels.
#[repr(align(64))]
#[derive(Debug, Clone, Copy)]
pub struct AlignedGrid {
    pub data: [f64; MAX_GRID_POINTS],
}

impl AlignedGrid {
    pub const fn zero() -> Self {
        Self {
            data: [0.0; MAX_GRID_POINTS],
        }
    }

    #[inline]
    pub fn clear(&mut self) {
        self.data.fill(0.0);
    }
}

/// The articulation used to excite a string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluckStyle {
    /// Soft initial displacement and a short force pulse.
    Finger,
    /// Sharper displacement with more high-frequency content.
    Pick,
    /// A stronger initial displacement and a bounded fret/slap collision.
    Slap,
}

/// One prepared bass-string physical model.
#[derive(Debug, Clone)]
pub struct FdtdString {
    pub params: BassStringParams,
    pub sample_rate: f64,
    pub dt: f64,
    /// Number of spatial segments. Valid nodes are `0..=segments`.
    pub segments: usize,
    pub spatial_step: f64,
    pub effective_length: f64,
    pub current_f0: f64,
    pub nominal_f0: f64,
    pub pitch_bend_semitones: f64,

    // Dimensionless update coefficients. They are exposed for diagnostics/tests.
    pub courant: f64,
    pub bending_courant: f64,
    pub damping_courant: f64,

    pub u_prev: AlignedGrid,
    pub u_curr: AlignedGrid,
    pub u_next: AlignedGrid,

    pub is_held: bool,
    pub is_active: bool,
    pub is_releasing: bool,
    pub fret_buzz: f64,
    pub last_contact_force: f64,
    pub current_delta_tension: f64,

    excitation_index: usize,
    pluck_samples_left: usize,
    pluck_duration: usize,
    pluck_force: f64,
    pluck_style: PluckStyle,
    slap_active: bool,
    slap_position: f64,
    slap_velocity: f64,
}

impl FdtdString {
    pub fn new(params: BassStringParams, sample_rate: f64) -> Self {
        let mut string = Self {
            params,
            sample_rate: sample_rate.max(1.0),
            dt: 1.0 / sample_rate.max(1.0),
            segments: MIN_SEGMENTS,
            spatial_step: params.scale_length / MIN_SEGMENTS as f64,
            effective_length: params.scale_length,
            current_f0: params.open_f0,
            nominal_f0: params.open_f0,
            pitch_bend_semitones: 0.0,
            courant: 0.0,
            bending_courant: 0.0,
            damping_courant: 0.0,
            u_prev: AlignedGrid::zero(),
            u_curr: AlignedGrid::zero(),
            u_next: AlignedGrid::zero(),
            is_held: false,
            is_active: false,
            is_releasing: false,
            fret_buzz: 0.35,
            last_contact_force: 0.0,
            current_delta_tension: 0.0,
            excitation_index: MIN_SEGMENTS / 5,
            pluck_samples_left: 0,
            pluck_duration: 0,
            pluck_force: 0.0,
            pluck_style: PluckStyle::Finger,
            slap_active: false,
            slap_position: 0.0,
            slap_velocity: 0.0,
        };
        string.configure_length(params.scale_length, params.open_f0);
        string
    }

    /// Rebuilds CFL coefficients for a new effective string length.
    ///
    /// The prescribed lower bound is the stiff-string CFL bound from the design
    /// document.  Taking `floor(L / h_min)` makes the actual grid spacing no
    /// smaller than that bound while retaining at least a small useful grid.
    pub fn configure_length(&mut self, length: f64, f0: f64) {
        self.effective_length = length.max(0.02);
        self.nominal_f0 = f0.max(1.0);
        self.current_f0 = self.nominal_f0 * 2.0_f64.powf(self.pitch_bend_semitones / 12.0);

        // The published tension and line-density values are intentionally
        // approximate. Calibrate the transverse wave speed to the tuning target
        // so the first FDTD mode remains at `current_f0` after changing fret.
        let c = 2.0 * self.effective_length * self.current_f0;
        let kappa = self.params.bending_stiffness.abs().max(1e-5);
        let k = self.dt;
        let c2k2 = c * c * k * k;
        let h_min = ((c2k2 + (c2k2 * c2k2 + 16.0 * kappa * kappa * k * k).sqrt()) * 0.5).sqrt();
        let mut segments = (self.effective_length / h_min.max(1e-5)).floor() as usize;
        segments = segments.clamp(MIN_SEGMENTS, MAX_GRID_POINTS - 3);
        self.segments = segments;
        self.spatial_step = self.effective_length / segments as f64;

        let h2 = self.spatial_step * self.spatial_step;
        let h4 = h2 * h2;
        self.courant = c * c * self.dt * self.dt / h2;
        self.bending_courant = kappa * kappa * self.dt * self.dt / h4;
        self.damping_courant = 2.0 * self.params.sigma1 * self.dt / h2;
        self.excitation_index = (segments as f64 * 0.18).round() as usize;
        self.excitation_index = self.excitation_index.clamp(2, segments - 2);
    }

    /// Applies a continuous pitch bend without clearing the vibrating grid.
    pub fn set_pitch_bend(&mut self, semitones: f64) {
        self.pitch_bend_semitones = semitones.clamp(-24.0, 24.0);
        let length = self.effective_length;
        let nominal_f0 = self.nominal_f0;
        self.configure_length(length, nominal_f0);
    }

    /// Sets the current fret and rebuilds only the spatial coefficients.
    pub fn set_fret(&mut self, fret: u8) {
        self.configure_length(
            self.params.length_at_fret(fret),
            self.params.frequency_at_fret(fret),
        );
        self.reset();
    }

    /// Plucks the string with a normalized MIDI velocity in `[0, 1]`.
    pub fn trigger(&mut self, velocity: f64, style: PluckStyle, position: f64) {
        self.reset();
        self.is_held = true;
        self.is_active = true;
        self.is_releasing = false;
        self.pluck_style = style;
        let velocity = velocity.clamp(0.001, 1.0);
        let position = position.clamp(0.06, 0.45);
        self.excitation_index = (self.segments as f64 * position).round() as usize;
        self.excitation_index = self.excitation_index.clamp(2, self.segments - 2);

        let amplitude = match style {
            PluckStyle::Finger => 0.00065 + 0.0017 * velocity,
            PluckStyle::Pick => 0.00045 + 0.00145 * velocity,
            PluckStyle::Slap => 0.0010 + 0.0030 * velocity,
        };
        let p = self.excitation_index as f64 / self.segments as f64;
        for i in 1..self.segments {
            let x = i as f64 / self.segments as f64;
            let shape = if x <= p {
                x / p.max(1e-6)
            } else {
                (1.0 - x) / (1.0 - p).max(1e-6)
            };
            self.u_curr.data[i] = amplitude * shape;
            self.u_prev.data[i] = self.u_curr.data[i];
        }

        self.pluck_duration = (self.sample_rate * 0.0035).round() as usize;
        self.pluck_duration = self.pluck_duration.max(1);
        self.pluck_samples_left = self.pluck_duration;
        self.pluck_force = match style {
            PluckStyle::Finger => 2.0 * velocity,
            PluckStyle::Pick => 3.5 * velocity,
            PluckStyle::Slap => 5.5 * velocity,
        };
        self.slap_active = style == PluckStyle::Slap;
        self.slap_position = 0.0;
        self.slap_velocity = 0.65 * velocity;
    }

    /// Releases the finger. The state is left alive and damped so a release is
    /// audible instead of abruptly truncating the string.
    pub fn release(&mut self) {
        self.is_held = false;
        self.is_releasing = true;
    }

    /// Advances the FDTD grid by one sample and returns the bridge force.
    #[inline]
    pub fn step(&mut self) -> f64 {
        if !self.is_active {
            return 0.0;
        }

        let n = self.segments;
        let h = self.spatial_step;
        let dt = self.dt;
        let mut slope_integral = 0.0;
        for i in 1..n {
            let slope = (self.u_curr.data[i + 1] - self.u_curr.data[i - 1]) / (2.0 * h);
            slope_integral += slope * slope * h;
        }
        let ea = self.params.youngs_modulus * self.params.area();
        let raw_delta_tension = ea / (2.0 * self.effective_length) * slope_integral;
        // The explicit update remains passive for the prepared grid. The small
        // clamp models the practical pitch glide without allowing an extreme
        // slap to violate the linear CFL margin.
        self.current_delta_tension = raw_delta_tension.min(self.params.tension * 0.04).max(0.0);
        let tension_ratio = (self.current_delta_tension / self.params.tension.max(1e-9)).min(0.04);

        let pulse_force = if self.pluck_samples_left > 0 {
            let elapsed = self.pluck_duration - self.pluck_samples_left;
            self.pluck_samples_left -= 1;
            let phase = (elapsed as f64 + 0.5) / self.pluck_duration as f64;
            self.pluck_force * (PI * phase).sin().powi(2)
        } else {
            0.0
        };

        let contact_index = (n * 3 / 4).clamp(2, n - 2);
        let contact_disp = self.u_curr.data[contact_index];
        let contact_vel = (self.u_curr.data[contact_index] - self.u_prev.data[contact_index]) / dt;
        let clearance = 0.00085 * (1.15 - 0.55 * self.fret_buzz.clamp(0.0, 1.0));
        let penetration = (-contact_disp - clearance).max(0.0);
        let mut contact_force = if penetration > 0.0 {
            let elastic = 9.0e5 * self.fret_buzz.clamp(0.0, 1.0) * penetration.powf(CONTACT_ALPHA);
            let damping = (-contact_vel * 90.0 * penetration.powf(CONTACT_ALPHA)).max(0.0);
            (elastic + damping).min(18.0)
        } else {
            0.0
        };

        // Slap is represented by a single explicit Hunt-Crossley-like impact
        // state. Its force is bounded and distributed over three nodes.
        let mut slap_force = 0.0;
        if self.slap_active {
            self.slap_position += self.slap_velocity * dt;
            let penetration = self.slap_position - self.u_curr.data[self.excitation_index];
            if penetration > 0.0 {
                let elastic = 1.8e6 * penetration.powf(1.5);
                let dissipative = (elastic * 0.035 * (self.slap_velocity - contact_vel)).max(0.0);
                slap_force = (elastic + dissipative).min(24.0);
                self.slap_velocity -= slap_force / 0.018 * dt;
            }
            self.slap_velocity *= 0.9994;
            if self.slap_velocity <= 0.0 && penetration <= 0.0 {
                self.slap_active = false;
            }
        }
        contact_force += slap_force;
        self.last_contact_force = contact_force;

        let denom = 1.0 + self.params.sigma0 * dt;
        let lap_prev_center = |idx: usize| -> f64 {
            grid_sample(&self.u_prev, idx as isize + 1, n)
                - 2.0 * grid_sample(&self.u_prev, idx as isize, n)
                + grid_sample(&self.u_prev, idx as isize - 1, n)
        };

        for i in 1..n {
            let ii = i as isize;
            let curr = grid_sample(&self.u_curr, ii, n);
            let prev = grid_sample(&self.u_prev, ii, n);
            let lap = grid_sample(&self.u_curr, ii + 1, n) - 2.0 * curr
                + grid_sample(&self.u_curr, ii - 1, n);
            let lap_old = lap_prev_center(i);
            let biharm = grid_sample(&self.u_curr, ii + 2, n)
                - 4.0 * grid_sample(&self.u_curr, ii + 1, n)
                + 6.0 * curr
                - 4.0 * grid_sample(&self.u_curr, ii - 1, n)
                + grid_sample(&self.u_curr, ii - 2, n);

            let mut force = 0.0;
            if i == self.excitation_index {
                force += pulse_force;
            }
            if i == contact_index {
                force += contact_force;
            }
            if i + 1 == self.excitation_index {
                force += 0.5 * pulse_force;
            }
            if i == self.excitation_index + 1 {
                force += 0.5 * pulse_force;
            }
            let force_density = force / h;
            let external = dt * dt * force_density / self.params.linear_density.max(1e-8);
            let nonlinear = self.courant * tension_ratio * lap;
            let linear = self.courant * lap - self.bending_courant * biharm;
            let frequency_damping = self.damping_courant * (lap - lap_old);
            self.u_next.data[i] = (2.0 * curr - (1.0 - self.params.sigma0 * dt) * prev
                + linear
                + nonlinear
                + frequency_damping
                + external)
                / denom;
        }
        self.u_next.data[0] = 0.0;
        self.u_next.data[n] = 0.0;
        if self.is_releasing {
            let release = (-dt / 0.15).exp();
            for i in 1..n {
                self.u_next.data[i] *= release;
            }
        }

        std::mem::swap(&mut self.u_prev, &mut self.u_curr);
        std::mem::swap(&mut self.u_curr, &mut self.u_next);
        self.u_curr.data[0] = 0.0;
        self.u_curr.data[n] = 0.0;

        if self.is_releasing && self.energy() < 1e-11 {
            self.clear_state();
            self.is_active = false;
            self.is_releasing = false;
        }

        self.bridge_force()
    }

    /// Velocity sampled through a finite spatial pickup aperture.
    #[inline]
    pub fn weighted_velocity(&self, position: f64, aperture: f64) -> f64 {
        if !self.is_active {
            return 0.0;
        }
        let center = position.clamp(0.03, 0.97) * self.segments as f64;
        let width = (aperture.clamp(0.005, 0.25) * self.segments as f64).max(0.75);
        let center_i = center.round() as isize;
        let mut sum = 0.0;
        let mut weight_sum = 0.0;
        for offset in -3..=3 {
            let index = (center_i + offset).clamp(1, self.segments as isize - 1) as usize;
            let distance = (index as f64 - center) / width;
            let weight = (-0.5 * distance * distance).exp();
            let velocity = (self.u_curr.data[index] - self.u_prev.data[index]) / self.dt;
            sum += weight * velocity;
            weight_sum += weight;
        }
        sum / weight_sum.max(1e-12)
    }

    /// Displacement sampled through the same finite pickup aperture as velocity.
    #[inline]
    pub fn weighted_displacement(&self, position: f64, aperture: f64) -> f64 {
        if !self.is_active {
            return 0.0;
        }
        let center = position.clamp(0.03, 0.97) * self.segments as f64;
        let width = (aperture.clamp(0.005, 0.25) * self.segments as f64).max(0.75);
        let center_i = center.round() as isize;
        let mut sum = 0.0;
        let mut weight_sum = 0.0;
        for offset in -3..=3 {
            let index = (center_i + offset).clamp(1, self.segments as isize - 1) as usize;
            let distance = (index as f64 - center) / width;
            let weight = (-0.5 * distance * distance).exp();
            sum += weight * self.u_curr.data[index];
            weight_sum += weight;
        }
        sum / weight_sum.max(1e-12)
    }

    /// Approximate transverse force transmitted to the bridge.
    #[inline]
    pub fn bridge_force(&self) -> f64 {
        let n = self.segments;
        let h = self.spatial_step;
        let slope = (self.u_curr.data[n] - self.u_curr.data[n - 1]) / h;
        let third = (self.u_curr.data[n - 1] - 2.0 * self.u_curr.data[n - 2]
            + self.u_curr.data[n - 3])
            / (h * h * h);
        -self.params.tension * slope
            + self.params.youngs_modulus * self.params.area() * 1e-10 * third
    }

    /// A bounded mechanical energy estimate useful for voice retirement and tests.
    pub fn energy(&self) -> f64 {
        let n = self.segments;
        let h = self.spatial_step;
        let mut energy = 0.0;
        for i in 1..n {
            let velocity = (self.u_curr.data[i] - self.u_prev.data[i]) / self.dt;
            let slope = (self.u_curr.data[i + 1] - self.u_curr.data[i - 1]) / (2.0 * h);
            energy += 0.5
                * self.params.linear_density
                * h
                * (velocity * velocity
                    + (self.params.tension / self.params.linear_density) * slope * slope);
        }
        energy.min(1.0e12)
    }

    pub fn reset(&mut self) {
        self.clear_state();
        self.is_held = false;
        self.is_active = false;
        self.is_releasing = false;
        self.pluck_samples_left = 0;
        self.slap_active = false;
        self.last_contact_force = 0.0;
        self.current_delta_tension = 0.0;
    }

    fn clear_state(&mut self) {
        self.u_prev.clear();
        self.u_curr.clear();
        self.u_next.clear();
    }
}

#[inline(always)]
fn grid_sample(grid: &AlignedGrid, index: isize, segments: usize) -> f64 {
    if index == 0 || index == segments as isize {
        0.0
    } else if index < 0 {
        let mirror = (-index) as usize;
        -grid.data[mirror.min(MAX_GRID_POINTS - 1)]
    } else if index > segments as isize {
        let mirror = (2 * segments as isize - index).max(0) as usize;
        -grid.data[mirror.min(MAX_GRID_POINTS - 1)]
    } else {
        grid.data[index as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::{FdtdString, PluckStyle};
    use crate::params::BassStringParams;

    #[test]
    fn cfl_grid_is_prepared_and_finite() {
        let string = FdtdString::new(BassStringParams::electric_four()[0], 48_000.0);
        assert!(string.segments > 20);
        assert!(string.spatial_step.is_finite());
        assert!(string.courant.is_finite() && string.bending_courant.is_finite());
        assert!(string.bending_courant < 0.5);
    }

    #[test]
    fn slap_and_release_stay_bounded() {
        let mut string = FdtdString::new(BassStringParams::electric_four()[0], 48_000.0);
        string.trigger(1.0, PluckStyle::Slap, 0.18);
        let mut peak: f64 = 0.0;
        for _ in 0..12_000 {
            peak = peak.max(string.step().abs());
            assert!(string.energy().is_finite());
        }
        assert!(peak.is_finite());
        let before_release = string.energy();
        string.release();
        for _ in 0..24_000 {
            let _ = string.step();
        }
        assert!(string.energy() < before_release);
    }
}
