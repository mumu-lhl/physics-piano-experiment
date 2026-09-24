"""Hunt-Crossley nonlinear hysteretic piano hammer contact dynamics solver."""

import math
from typing import Tuple, Optional
from physics_piano.params.schema import HammerPhysicalParameters


class HuntCrossleyHammer:
    """Nonlinear felt hammer with Hunt-Crossley contact mechanics.
    
    Model:
      F_h(t) = max(0, K_h * [eta]^p + lambda_h * [eta]^p * d(eta)/dt)
      m_h * d^2 u_h / dt^2 = -F_h
    where:
      eta(t) = u_h(t) - u_string(x_h, t)  (compression indentation)
    """

    def __init__(self, params: HammerPhysicalParameters, sample_rate: float = 48000.0):
        self.params = params
        self.sample_rate = float(sample_rate)
        self.dt = 1.0 / self.sample_rate

        self.m_h = params.mass
        self.K_h = params.stiffness
        self.p = params.exponent
        self.lambda_h = params.dissipation
        self.v_max = params.max_velocity
        self.gamma_felt = params.velocity_gamma

        # Dynamic state
        self.u_h = 0.0          # Hammer displacement (m)
        self.v_h = 0.0          # Hammer velocity (m/s)
        self.is_active = False  # True during active stroke / contact
        self.contact_time = 0.0 # Cumulative time in contact (seconds)
        self.has_struck = False

    def strike(self, velocity: float):
        """Trigger hammer strike with normalized MIDI velocity in (0.0, 1.0]."""
        clamped_vel = max(0.001, min(1.0, float(velocity)))
        # Non-linear velocity mapping: v0 = v_max * (velocity)^gamma
        v0 = self.v_max * (clamped_vel ** self.gamma_felt)

        self.u_h = 0.0
        self.v_h = v0
        self.is_active = True
        self.contact_time = 0.0
        self.has_struck = False

    def compute_force(self, u_string: float, v_string: float) -> float:
        """Calculate nonlinear contact force between hammer and string.
        
        Args:
            u_string: Transverse displacement of string at strike location x_h
            v_string: Transverse velocity of string at strike location x_h
        Returns:
            F_h: Non-negative contact force exerted on string
        """
        if not self.is_active:
            return 0.0

        # Felt compression indentation: eta = u_h - u_string
        eta = self.u_h - u_string
        if eta <= 0.0:
            if self.has_struck and self.v_h <= 0.0:
                self.is_active = False
            return 0.0

        self.has_struck = True
        self.contact_time += self.dt

        # Relative indentation velocity: d(eta)/dt = v_h - v_string
        v_rel = self.v_h - v_string

        # Hunt-Crossley force formulation:
        # F = K_h * (eta^p) + lambda_h * (eta^p) * v_rel
        eta_p = eta ** self.p
        elastic_term = self.K_h * eta_p
        damping_term = self.lambda_h * eta_p * v_rel
        raw_force = elastic_term + damping_term

        # Physically, contact force is strictly compressive (non-adhesive)
        force = max(0.0, raw_force)
        return float(force)

    def advance(self, force: float):
        """Advance hammer state using velocity-Verlet numerical scheme."""
        if not self.is_active:
            return

        # Acceleration: m_h * a_h = -force
        acc = -force / self.m_h

        # Update position and velocity
        self.u_h += self.v_h * self.dt + 0.5 * acc * (self.dt ** 2)
        self.v_h += acc * self.dt

        # If hammer has rebounded behind resting plane and is traveling away, deactivate
        if self.has_struck and self.u_h < 0.0 and self.v_h <= 0.0:
            self.is_active = False
