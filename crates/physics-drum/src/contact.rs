//! Bounded Hunt-Crossley contact used by mallets and membrane/snares.

#[derive(Debug, Clone, Copy)]
pub struct HuntCrossleyExciter {
    pub mallet_displacement: f64,
    pub mallet_velocity: f64,
    pub effective_mass: f64,
    pub stiffness: f64,
    pub exponent: f64,
    pub dissipation: f64,
    pub is_contacting: bool,
    pub max_force: f64,
}

impl Default for HuntCrossleyExciter {
    fn default() -> Self {
        Self {
            mallet_displacement: 0.0,
            mallet_velocity: 0.0,
            effective_mass: 0.02,
            stiffness: 1.0e6,
            exponent: 1.5,
            dissipation: 0.04,
            is_contacting: false,
            max_force: 150.0,
        }
    }
}

impl HuntCrossleyExciter {
    pub fn trigger(&mut self, velocity: f64, mass: f64, stiffness: f64, exponent: f64) {
        self.mallet_displacement = 0.0;
        self.mallet_velocity = velocity.clamp(0.01, 12.0);
        self.effective_mass = mass.max(1e-5);
        self.stiffness = stiffness.max(1.0);
        self.exponent = exponent.clamp(1.2, 2.5);
        self.is_contacting = true;
    }

    /// One explicit, bounded contact step. The mallet travels in the positive
    /// direction; positive penetration produces an opposing force on it and an
    /// equal force on the membrane.
    #[inline(always)]
    pub fn step(&mut self, surface_displacement: f64, surface_velocity: f64, dt: f64) -> f64 {
        if !self.is_contacting {
            return 0.0;
        }
        let penetration = self.mallet_displacement - surface_displacement;
        let relative_velocity = self.mallet_velocity - surface_velocity;
        let force = if penetration > 0.0 {
            let elastic = self.stiffness * penetration.powf(self.exponent);
            let dissipative = (self.dissipation * elastic * relative_velocity).max(0.0);
            (elastic + dissipative).clamp(0.0, self.max_force)
        } else {
            0.0
        };
        self.mallet_velocity -= force / self.effective_mass * dt;
        self.mallet_displacement += self.mallet_velocity * dt;
        if penetration <= 0.0 && self.mallet_velocity <= surface_velocity {
            self.is_contacting = false;
        }
        force
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::HuntCrossleyExciter;

    #[test]
    fn contact_force_is_finite_and_non_negative() {
        let mut exciter = HuntCrossleyExciter::default();
        exciter.trigger(2.0, 0.02, 1.0e6, 1.5);
        let mut peak: f64 = 0.0;
        for _ in 0..2_000 {
            let force = exciter.step(0.0, 0.0, 1.0 / 48_000.0);
            assert!(force.is_finite() && force >= 0.0);
            peak = peak.max(force);
        }
        assert!(peak > 0.0);
    }
}
