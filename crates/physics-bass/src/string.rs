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
pub const FRET_COUNT: usize = 24;
const MAX_ACTIVE_FRET_CONTACTS: usize = 4;
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
    /// High-velocity hook and snap producing an aggressive slap pop.
    Pop,
    /// Left-hand muted percussive strike with fast viscoelastic damping.
    Ghost,
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
    pub is_ghost: bool,
    pub fret_buzz: f64,
    pub current_fret: u8,
    pub last_contact_force: f64,
    pub current_delta_tension: f64,
    bridge_displacement: f64,
    previous_bridge_displacement: f64,

    excitation_index: usize,
    pluck_samples_left: usize,
    pluck_duration: usize,
    pluck_force: f64,
    pluck_style: PluckStyle,
    slap_active: bool,
    slap_position: f64,
    slap_velocity: f64,
    fret_indices: [usize; FRET_COUNT],
    fret_fractions: [f64; FRET_COUNT],
    fret_psi: [f64; FRET_COUNT],
    active_fret_count: usize,
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
            is_ghost: false,
            fret_buzz: 0.35,
            current_fret: 0,
            last_contact_force: 0.0,
            current_delta_tension: 0.0,
            bridge_displacement: 0.0,
            previous_bridge_displacement: 0.0,
            excitation_index: MIN_SEGMENTS / 5,
            pluck_samples_left: 0,
            pluck_duration: 0,
            pluck_force: 0.0,
            pluck_style: PluckStyle::Finger,
            slap_active: false,
            slap_position: 0.0,
            slap_velocity: 0.0,
            fret_indices: [0; FRET_COUNT],
            fret_fractions: [0.0; FRET_COUNT],
            fret_psi: [0.0; FRET_COUNT],
            active_fret_count: 0,
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

        // Use the physical wave speed and stiff-string coefficient. The string
        // parameter tables are tuned so sqrt(T/mu) matches the open-string
        // frequency; pitch bend scales the wave speed without reinterpreting
        // the material constants.
        let wave_speed = (self.params.tension / self.params.linear_density.max(1e-12)).sqrt();
        let c = wave_speed * 2.0_f64.powf(self.pitch_bend_semitones / 12.0);
        let max_c = wave_speed * 4.0; // reserve CFL margin for the +24-semitone bend range
        let bending_coefficient = (self.params.youngs_modulus * self.params.second_moment()
            / self.params.linear_density.max(1e-12))
        .sqrt()
            * self.params.bending_stiffness.abs().max(1e-5);
        let k = self.dt;
        let c2k2 = max_c * max_c * k * k;
        let h_min = ((c2k2 + (c2k2 * c2k2 + 16.0 * bending_coefficient.powi(2) * k * k).sqrt())
            * 0.5)
            .sqrt();
        let mut segments = (self.effective_length / h_min.max(1e-5)).floor() as usize;
        segments = segments.clamp(MIN_SEGMENTS, MAX_GRID_POINTS - 3);
        self.segments = segments;
        self.spatial_step = self.effective_length / segments as f64;

        let h2 = self.spatial_step * self.spatial_step;
        let h4 = h2 * h2;
        self.courant = c * c * self.dt * self.dt / h2;
        self.bending_courant = bending_coefficient.powi(2) * self.dt * self.dt / h4;
        self.damping_courant = 2.0 * self.params.sigma1 * self.dt / h2;
        self.excitation_index = (segments as f64 * 0.18).round() as usize;
        self.excitation_index = self.excitation_index.clamp(2, segments - 2);
        self.rebuild_fret_contacts();
    }

    /// Rebuilds the fixed spatial contact stencils for the currently fretted
    /// string. Open strings expose all frets; a stopped note exposes only the
    /// frets between the nut and the finger. This keeps contact distributed
    /// over the physical fret line instead of using one arbitrary grid node.
    fn rebuild_fret_contacts(&mut self) {
        self.fret_indices.fill(0);
        self.fret_fractions.fill(0.0);
        self.fret_psi.fill(0.0);
        self.active_fret_count = 0;
        let last_fret = if self.current_fret == 0 {
            FRET_COUNT
        } else {
            self.current_fret.min(FRET_COUNT as u8) as usize
        };
        for fret in 1..=last_fret {
            let physical_position =
                self.params.scale_length * (1.0 - 2.0_f64.powf(-(fret as f64) / 12.0));
            let relative_position = physical_position / self.effective_length.max(1e-6);
            if relative_position > 0.995 {
                continue;
            }
            let grid_position = relative_position * self.segments as f64;
            let index = (grid_position.floor() as usize).clamp(2, self.segments - 2);
            self.fret_indices[self.active_fret_count] = index;
            self.fret_fractions[self.active_fret_count] =
                (grid_position - index as f64).clamp(0.0, 1.0);
            self.active_fret_count += 1;
        }
    }

    /// Applies a continuous pitch bend without clearing the vibrating grid.
    pub fn set_pitch_bend(&mut self, semitones: f64) {
        self.pitch_bend_semitones = semitones.clamp(-24.0, 24.0);
        self.current_f0 = self.nominal_f0 * 2.0_f64.powf(self.pitch_bend_semitones / 12.0);
        let base_wave_speed = (self.params.tension / self.params.linear_density.max(1e-12)).sqrt();
        let wave_speed = base_wave_speed * 2.0_f64.powf(self.pitch_bend_semitones / 12.0);
        self.courant =
            wave_speed * wave_speed * self.dt * self.dt / (self.spatial_step * self.spatial_step);
    }

    /// Sets the current fret and rebuilds only the spatial coefficients.
    pub fn set_fret(&mut self, fret: u8) {
        self.current_fret = fret.min(FRET_COUNT as u8);
        self.configure_length(
            self.params.length_at_fret(self.current_fret),
            self.params.frequency_at_fret(self.current_fret),
        );
        self.reset();
    }

    /// Reconfigures physical string parameters (e.g. switching between Electric and Acoustic upright bass).
    pub fn reconfigure_params(&mut self, params: BassStringParams) {
        self.params = params;
        self.configure_length(
            params.length_at_fret(self.current_fret),
            params.frequency_at_fret(self.current_fret),
        );
        self.reset();
    }

    /// Plucks the string with a normalized MIDI velocity in `[0, 1]`.
    pub fn trigger(&mut self, velocity: f64, style: PluckStyle, position: f64) {
        self.reset();
        self.is_held = true;
        self.is_active = true;
        self.is_releasing = false;
        self.is_ghost = style == PluckStyle::Ghost;
        self.pluck_style = style;
        let velocity = velocity.clamp(0.001, 1.0);
        let position = position.clamp(0.06, 0.45);
        self.excitation_index = (self.segments as f64 * position).round() as usize;
        self.excitation_index = self.excitation_index.clamp(2, self.segments - 2);

        let amplitude = match style {
            PluckStyle::Finger => 0.00065 + 0.0017 * velocity,
            PluckStyle::Pick => 0.00045 + 0.00145 * velocity,
            PluckStyle::Slap => 0.0010 + 0.0030 * velocity,
            PluckStyle::Pop => 0.0016 + 0.0038 * velocity,
            PluckStyle::Ghost => 0.00035 + 0.00085 * velocity,
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

        self.pluck_duration = if style == PluckStyle::Ghost {
            (self.sample_rate * 0.0012).round() as usize
        } else {
            (self.sample_rate * 0.0035).round() as usize
        };
        self.pluck_duration = self.pluck_duration.max(1);
        self.pluck_samples_left = self.pluck_duration;
        self.pluck_force = match style {
            PluckStyle::Finger => 2.0 * velocity,
            PluckStyle::Pick => 3.5 * velocity,
            PluckStyle::Slap => 5.5 * velocity,
            PluckStyle::Pop => 7.5 * velocity,
            PluckStyle::Ghost => 2.5 * velocity,
        };
        self.slap_active = style == PluckStyle::Slap || style == PluckStyle::Pop;
        self.slap_position = 0.0;
        self.slap_velocity = match style {
            PluckStyle::Pop => 0.95 * velocity,
            _ => 0.65 * velocity,
        };
    }

    /// Transitions to a new fret on the vibrating string without clearing the FDTD grid.
    /// Preserves stored wave energy, interpolates spatial displacements across the new effective
    /// scale length, and injects hammer-on/pull-off fret contact transients.
    pub fn legato_to_fret(&mut self, new_fret: u8, velocity: f64) {
        let new_fret = new_fret.min(FRET_COUNT as u8);
        if new_fret == self.current_fret {
            return;
        }

        let is_hammer_on = new_fret > self.current_fret;
        let old_length = self.effective_length;
        let new_length = self.params.length_at_fret(new_fret);
        let ratio = new_length / old_length.max(1e-6);

        let n = self.segments;
        let mut new_curr = [0.0; MAX_GRID_POINTS];
        let mut new_prev = [0.0; MAX_GRID_POINTS];

        // Spatial wave mapping relative to the fixed bridge (index n)
        for i in 1..n {
            let i_old = n as f64 - (n - i) as f64 * ratio;
            if i_old >= 0.0 && i_old <= n as f64 {
                let idx = i_old.floor() as usize;
                let frac = i_old - idx as f64;
                let idx_next = (idx + 1).min(n);
                new_curr[i] =
                    self.u_curr.data[idx] * (1.0 - frac) + self.u_curr.data[idx_next] * frac;
                new_prev[i] =
                    self.u_prev.data[idx] * (1.0 - frac) + self.u_prev.data[idx_next] * frac;
            }
        }

        self.u_curr.data[1..n].copy_from_slice(&new_curr[1..n]);
        self.u_prev.data[1..n].copy_from_slice(&new_prev[1..n]);

        // Articulation transient:
        let velocity = velocity.clamp(0.01, 1.0);
        if is_hammer_on {
            // Metallic fret-strike impulse at the new stopping boundary (near node 1)
            let strike_impulse = 0.00030 * velocity;
            self.u_curr.data[1] += strike_impulse;
            self.u_curr.data[2] += strike_impulse * 0.5;
        } else {
            // Finger-pad release step for pull-off
            let pull_transient = 0.00015 * velocity;
            let mid = n / 4;
            self.u_curr.data[mid] += pull_transient;
        }

        self.current_fret = new_fret;
        self.configure_length(
            self.params.length_at_fret(new_fret),
            self.params.frequency_at_fret(new_fret),
        );
        self.is_held = true;
        self.is_active = true;
        self.is_releasing = false;
        self.is_ghost = false;
    }

    /// Releases the finger. The state is left alive and damped so a release is
    /// audible instead of abruptly truncating the string.
    pub fn release(&mut self) {
        self.is_held = false;
        self.is_releasing = true;
    }

    /// Updates the moving bridge boundary supplied by the coupled body model.
    #[inline]
    pub fn set_bridge_displacement(&mut self, displacement: f64) {
        let displacement = displacement.clamp(-0.001, 0.001);
        self.previous_bridge_displacement = self.bridge_displacement;
        self.bridge_displacement = displacement;
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
        self.u_curr.data[n] = self.bridge_displacement;
        self.u_prev.data[n] = self.previous_bridge_displacement;
        let mut slope_integral = 0.0;
        for i in 1..n {
            let slope = (self.u_curr.data[i + 1] - self.u_curr.data[i - 1]) / (2.0 * h);
            slope_integral += slope * slope * h;
        }
        let ea = self.params.youngs_modulus * self.params.area();
        let raw_delta_tension = ea / (2.0 * self.effective_length) * slope_integral;
        // Restrict geometric hardening only to the remaining CFL budget. The
        // grid is prepared for +24 semitones, so this bound is sample-invariant
        // and avoids the arbitrary fixed tension clamp used by the prototype.
        let max_courant = (1.0 - 4.0 * self.bending_courant).max(self.courant);
        let tension_ratio = (max_courant / self.courant.max(1e-12) - 1.0).max(0.0);
        let tension_limit = self.params.tension * tension_ratio;
        self.current_delta_tension = if tension_limit > 1e-9 {
            tension_limit * (raw_delta_tension / tension_limit).tanh()
        } else {
            0.0
        };
        let tension_ratio = self.current_delta_tension / self.params.tension.max(1e-9);

        let pulse_force = if self.pluck_samples_left > 0 {
            let elapsed = self.pluck_duration - self.pluck_samples_left;
            self.pluck_samples_left -= 1;
            let phase = (elapsed as f64 + 0.5) / self.pluck_duration as f64;
            self.pluck_force * (PI * phase).sin().powi(2)
        } else {
            0.0
        };

        // Detect unilateral fret contacts and retain the deepest four rows.
        // This explicit rank cap keeps the fixed-size Woodbury solve bounded:
        // O(N + P^3), P <= 4, with no audio-thread allocation.
        let clearance = 0.00085 * (1.15 - 0.55 * self.fret_buzz.clamp(0.0, 1.0));
        let contact_stiffness = 9.0e5 * self.fret_buzz.clamp(0.0, 1.0);
        let contact_exponent = CONTACT_ALPHA;
        let root_scale = (2.0 * contact_stiffness / (contact_exponent + 1.0)).sqrt();
        let gradient_scale = (contact_stiffness * (contact_exponent + 1.0) * 0.5).sqrt();
        let mut candidate_gradient = [0.0; FRET_COUNT];
        let mut contact_slots = [0usize; MAX_ACTIVE_FRET_CONTACTS];
        let mut contact_eta = [0.0; MAX_ACTIVE_FRET_CONTACTS];
        let mut contact_gradient = [0.0; MAX_ACTIVE_FRET_CONTACTS];
        let mut contact_count = 0;
        if contact_stiffness > 0.0 {
            for (contact, candidate_gradient_slot) in candidate_gradient
                .iter_mut()
                .enumerate()
                .take(self.active_fret_count)
            {
                let index = self.fret_indices[contact];
                let fraction = self.fret_fractions[contact];
                let displacement = self.u_curr.data[index] * (1.0 - fraction)
                    + self.u_curr.data[index + 1] * fraction;
                let previous_displacement = self.u_prev.data[index] * (1.0 - fraction)
                    + self.u_prev.data[index + 1] * fraction;
                let eta = -clearance - displacement;
                if eta <= 0.0 {
                    continue;
                }
                let previous_eta = -clearance - previous_displacement;
                let extrapolated = (1.5 * eta - 0.5 * previous_eta).max(eta * 0.5);
                *candidate_gradient_slot =
                    gradient_scale * extrapolated.powf((contact_exponent - 1.0) * 0.5);
                let insert_at = contact_eta[..contact_count]
                    .iter()
                    .position(|selected| eta > *selected)
                    .unwrap_or(contact_count);
                if insert_at < MAX_ACTIVE_FRET_CONTACTS {
                    let new_count = (contact_count + 1).min(MAX_ACTIVE_FRET_CONTACTS);
                    for move_to in (insert_at + 1..new_count).rev() {
                        contact_slots[move_to] = contact_slots[move_to - 1];
                        contact_eta[move_to] = contact_eta[move_to - 1];
                        contact_gradient[move_to] = contact_gradient[move_to - 1];
                    }
                    contact_slots[insert_at] = contact;
                    contact_eta[insert_at] = eta;
                    contact_gradient[insert_at] = *candidate_gradient_slot;
                    contact_count = new_count;
                }
            }
        }
        let mut selected = [false; FRET_COUNT];
        for slot in 0..contact_count {
            let contact = contact_slots[slot];
            selected[contact] = true;
            if self.fret_psi[contact] <= 1e-12 {
                self.fret_psi[contact] =
                    root_scale * contact_eta[slot].powf((contact_exponent + 1.0) * 0.5);
            }
        }
        for (contact, is_selected) in selected.iter().enumerate().take(self.active_fret_count) {
            if !is_selected {
                self.fret_psi[contact] = 0.0;
            }
        }

        // Slap is represented by a single explicit Hunt-Crossley-like impact
        // state. Its force is bounded and distributed at the excitation node.
        let mut slap_force = 0.0;
        if self.slap_active {
            self.slap_position += self.slap_velocity * dt;
            let penetration = self.slap_position - self.u_curr.data[self.excitation_index];
            let excitation_velocity = (self.u_curr.data[self.excitation_index]
                - self.u_prev.data[self.excitation_index])
                / dt;
            if penetration > 0.0 {
                let elastic = 1.8e6 * penetration.powf(1.5);
                let dissipative =
                    (elastic * 0.035 * (self.slap_velocity - excitation_velocity)).max(0.0);
                slap_force = (elastic + dissipative).min(24.0);
                self.slap_velocity -= slap_force / 0.018 * dt;
            }
            self.slap_velocity *= 0.9994;
            if self.slap_velocity <= 0.0 && penetration <= 0.0 {
                self.slap_active = false;
            }
        }
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
            if i == self.excitation_index {
                force += slap_force;
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
        self.u_next.data[n] = self.bridge_displacement;
        let fret_contact_total = self.apply_sav_fret_contacts(
            clearance,
            contact_count,
            &contact_slots,
            &contact_eta,
            &contact_gradient,
            denom,
        );
        self.last_contact_force = fret_contact_total + slap_force;
        if self.is_ghost {
            let ghost_damp = (-dt / 0.008).exp();
            for i in 1..n {
                self.u_next.data[i] *= ghost_damp;
            }
        } else if self.is_releasing {
            let release = (-dt / 0.15).exp();
            for i in 1..n {
                self.u_next.data[i] *= release;
            }
        }

        std::mem::swap(&mut self.u_prev, &mut self.u_curr);
        std::mem::swap(&mut self.u_curr, &mut self.u_next);
        self.u_curr.data[0] = 0.0;
        self.u_curr.data[n] = self.bridge_displacement;
        self.u_prev.data[n] = self.previous_bridge_displacement;

        if (self.is_releasing || self.is_ghost) && self.energy() < 1e-11 {
            self.clear_state();
            self.is_active = false;
            self.is_releasing = false;
            self.is_ghost = false;
        }

        self.bridge_force()
    }

    /// Applies the SAV/IEQ discrete-gradient contact update. The linear string
    /// step is already complete in `u_next`; contact is a rank-P update
    /// `(I + U Uᵀ) u = b`, solved via a fixed-size Woodbury/Cholesky system.
    /// No iteration, heap allocation, or unbounded active set is used.
    fn apply_sav_fret_contacts(
        &mut self,
        clearance: f64,
        count: usize,
        slots: &[usize; MAX_ACTIVE_FRET_CONTACTS],
        eta: &[f64; MAX_ACTIVE_FRET_CONTACTS],
        gradients: &[f64; MAX_ACTIVE_FRET_CONTACTS],
        denom: f64,
    ) -> f64 {
        if count == 0 {
            return 0.0;
        }
        let node_mass = self.params.linear_density * self.spatial_step;
        let update_scale = self.dt * self.dt / (node_mass.max(1e-12) * denom);
        let woodbury_scale = 0.5 * update_scale;
        let mut nodes = [[0usize; 2]; MAX_ACTIVE_FRET_CONTACTS];
        let mut basis = [[0.0; 2]; MAX_ACTIVE_FRET_CONTACTS];
        let mut matrix = [[0.0; MAX_ACTIVE_FRET_CONTACTS]; MAX_ACTIVE_FRET_CONTACTS];
        let mut cholesky = [[0.0; MAX_ACTIVE_FRET_CONTACTS]; MAX_ACTIVE_FRET_CONTACTS];
        let mut rhs = [0.0; MAX_ACTIVE_FRET_CONTACTS];
        let mut solution = [0.0; MAX_ACTIVE_FRET_CONTACTS];
        let mut correction = [[0.0; 2]; MAX_ACTIVE_FRET_CONTACTS];

        for row in 0..count {
            let contact = slots[row];
            let index = self.fret_indices[contact];
            let fraction = self.fret_fractions[contact];
            let gradient = gradients[row];
            nodes[row] = [index, index + 1];
            basis[row] = [gradient * (1.0 - fraction), gradient * fraction];
            let source = gradient * self.fret_psi[contact]
                + 0.5 * gradient * gradient * (-clearance - eta[row]);
            self.u_next.data[index] += update_scale * (1.0 - fraction) * source;
            self.u_next.data[index + 1] += update_scale * fraction * source;
            rhs[row] = basis[row][0] * self.u_next.data[index]
                + basis[row][1] * self.u_next.data[index + 1];
        }

        for row in 0..count {
            for column in 0..count {
                let mut gram = 0.0;
                for left in 0..2 {
                    for right in 0..2 {
                        if nodes[row][left] == nodes[column][right] {
                            gram += basis[row][left] * basis[column][right];
                        }
                    }
                }
                matrix[row][column] =
                    (if row == column { 1.0 } else { 0.0 }) + woodbury_scale * gram;
            }
        }

        // The Woodbury core is symmetric positive definite. A bounded Cholesky
        // solve is cheaper and more stable than pivoted general elimination.
        for row in 0..count {
            for column in 0..=row {
                let mut value = matrix[row][column];
                for (k, left) in cholesky[row][..column].iter().enumerate() {
                    value -= left * cholesky[column][k];
                }
                if row == column {
                    cholesky[row][column] = value.max(1e-12).sqrt();
                } else {
                    cholesky[row][column] = value / cholesky[column][column];
                }
            }
        }
        for row in 0..count {
            let mut value = rhs[row];
            for column in 0..row {
                value -= cholesky[row][column] * solution[column];
            }
            solution[row] = value / cholesky[row][row];
        }
        for row in (0..count).rev() {
            let mut value = solution[row];
            for column in row + 1..count {
                value -= cholesky[column][row] * solution[column];
            }
            solution[row] = value / cholesky[row][row];
        }

        for row in 0..count {
            correction[row][0] = -woodbury_scale * basis[row][0] * solution[row];
            correction[row][1] = -woodbury_scale * basis[row][1] * solution[row];
        }
        for row in 0..count {
            self.u_next.data[nodes[row][0]] += correction[row][0];
            self.u_next.data[nodes[row][1]] += correction[row][1];
        }

        let mut total_force = 0.0;
        for row in 0..count {
            let contact = slots[row];
            let index = self.fret_indices[contact];
            let fraction = self.fret_fractions[contact];
            let displacement =
                self.u_curr.data[index] * (1.0 - fraction) + self.u_curr.data[index + 1] * fraction;
            let velocity = (displacement
                - (self.u_prev.data[index] * (1.0 - fraction)
                    + self.u_prev.data[index + 1] * fraction))
                / self.dt;
            let penetration = eta[row];
            let damping = (-velocity * 90.0 * penetration.powf(CONTACT_ALPHA)).max(0.0);
            self.u_next.data[index] += update_scale * (1.0 - fraction) * damping;
            self.u_next.data[index + 1] += update_scale * fraction * damping;

            let next_displacement =
                self.u_next.data[index] * (1.0 - fraction) + self.u_next.data[index + 1] * fraction;
            let next_eta = -clearance - next_displacement;
            let old_psi = self.fret_psi[contact];
            let next_psi = if next_eta > 0.0 {
                (old_psi + gradients[row] * (next_eta - penetration)).max(0.0)
            } else {
                0.0
            };
            let elastic = gradients[row] * 0.5 * (old_psi + next_psi);
            self.fret_psi[contact] = next_psi;
            total_force += elastic + damping;
        }
        total_force
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
        let third = (self.u_curr.data[n] - 3.0 * self.u_curr.data[n - 1]
            + 3.0 * self.u_curr.data[n - 2]
            - self.u_curr.data[n - 3])
            / (h * h * h);
        -self.params.tension * slope
            + self.params.youngs_modulus * self.params.second_moment() * third
    }

    /// A bounded mechanical energy estimate useful for voice retirement and tests.
    pub fn energy(&self) -> f64 {
        let n = self.segments;
        let h = self.spatial_step;
        let mut energy = 0.0;
        let mut slope_integral = 0.0;
        for i in 1..n {
            let velocity = (self.u_curr.data[i] - self.u_prev.data[i]) / self.dt;
            let slope = (self.u_curr.data[i + 1] - self.u_curr.data[i - 1]) / (2.0 * h);
            let curvature = (self.u_curr.data[i + 1] - 2.0 * self.u_curr.data[i]
                + self.u_curr.data[i - 1])
                / (h * h);
            slope_integral += slope * slope * h;
            energy += 0.5
                * h
                * (self.params.linear_density * velocity * velocity
                    + self.params.tension * slope * slope
                    + self.params.youngs_modulus
                        * self.params.second_moment()
                        * curvature
                        * curvature);
        }
        let ea = self.params.youngs_modulus * self.params.area();
        let geometric_stiffness = ea / (2.0 * self.effective_length.max(1e-6));
        let raw_delta_tension = geometric_stiffness * slope_integral;
        let available_courant = (1.0 - 4.0 * self.bending_courant).max(self.courant);
        let tension_limit =
            self.params.tension * (available_courant / self.courant.max(1e-12) - 1.0).max(0.0);
        if tension_limit > 1e-9 {
            let x = raw_delta_tension / tension_limit;
            let abs_x = x.abs();
            let log_cosh = abs_x + (-2.0 * abs_x).exp().ln_1p() - std::f64::consts::LN_2;
            energy +=
                tension_limit * tension_limit / (2.0 * geometric_stiffness.max(1e-12)) * log_cosh;
        }
        energy += self.fret_psi[..self.active_fret_count]
            .iter()
            .map(|psi| 0.5 * psi * psi)
            .sum::<f64>();
        if self.slap_active {
            energy += 0.5 * 0.018 * self.slap_velocity * self.slap_velocity;
        }
        energy.min(1.0e12)
    }

    pub fn reset(&mut self) {
        self.clear_state();
        self.is_held = false;
        self.is_active = false;
        self.is_releasing = false;
        self.is_ghost = false;
        self.pluck_samples_left = 0;
        self.slap_active = false;
        self.last_contact_force = 0.0;
        self.current_delta_tension = 0.0;
        self.bridge_displacement = 0.0;
        self.previous_bridge_displacement = 0.0;
        self.fret_psi.fill(0.0);
    }

    fn clear_state(&mut self) {
        self.u_prev.clear();
        self.u_curr.clear();
        self.u_next.clear();
    }
}

#[inline(always)]
fn grid_sample(grid: &AlignedGrid, index: isize, segments: usize) -> f64 {
    if index == 0 {
        0.0
    } else if index == segments as isize {
        grid.data[segments]
    } else if index < 0 {
        let mirror = (-index) as usize;
        -grid.data[mirror.min(MAX_GRID_POINTS - 1)]
    } else if index > segments as isize {
        let mirror = (2 * segments as isize - index).max(0) as usize;
        2.0 * grid.data[segments] - grid.data[mirror.min(MAX_GRID_POINTS - 1)]
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
        let mut string = FdtdString::new(BassStringParams::electric_four()[0], 48_000.0);
        assert!(string.segments > 20);
        assert!(string.spatial_step.is_finite());
        assert!(string.courant.is_finite() && string.bending_courant.is_finite());
        assert!(string.bending_courant < 0.5);
        let prepared_segments = string.segments;
        string.set_pitch_bend(24.0);
        assert_eq!(string.segments, prepared_segments);
        assert!(string.courant + 4.0 * string.bending_courant <= 1.000_01);
        string.set_pitch_bend(-24.0);
        assert!(string.courant + 4.0 * string.bending_courant <= 1.000_01);
    }

    #[test]
    fn fretted_contact_stencils_cover_every_fret_up_to_the_stopping_fret() {
        let mut string = FdtdString::new(BassStringParams::electric_four()[0], 48_000.0);
        string.set_fret(12);
        assert_eq!(string.current_fret, 12);
        assert_eq!(string.active_fret_count, 11);
        assert!(
            string.fret_fractions[..11]
                .iter()
                .all(|fraction| (0.0..=1.0).contains(fraction))
        );

        string.trigger(1.0, PluckStyle::Pick, 0.19);
        let mut max_contact = 0.0_f64;
        for _ in 0..12_000 {
            let _ = string.step();
            max_contact = max_contact.max(string.last_contact_force);
            assert!(string.energy().is_finite());
        }
        assert!(max_contact.is_finite() && max_contact > 0.0);
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
