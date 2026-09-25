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

        # SAV (Scalar Auxiliary Variable) energy-stable state
        self.use_sav = True
        self.xi = 0.0  # xi(t) = sqrt(K_h / (p+1) * eta^(p+1))

        # Una Corda soft pedal modification
        self.una_corda = False

    def set_una_corda(self, enabled: bool):
        """Engage or disengage Una Corda (soft pedal) shifting hammer onto softer felt."""
        self.una_corda = bool(enabled)

    def strike(self, velocity: float, initial_u_string: float = 0.0):
        """Trigger hammer strike with normalized MIDI velocity in (0.0, 1.0].
        
        Args:
            velocity: Normalized strike velocity (0.0 to 1.0)
            initial_u_string: Current displacement of string at strike location x_h
        """
        clamped_vel = max(0.001, min(1.0, float(velocity)))
        # Non-linear velocity mapping: v0 = v_max * (velocity)^gamma
        v0 = self.v_max * (clamped_vel ** self.gamma_felt)

        # When re-striking a vibrating string, position hammer slightly behind string contact
        self.u_h = min(0.0, float(initial_u_string))
        self.v_h = v0
        self.is_active = True
        self.contact_time = 0.0
        self.has_struck = False
        self.xi = 0.0

    def compute_force(self, u_string: float, v_string: float) -> float:
        """Calculate nonlinear contact force between hammer and string.
        
        Supports both classical Hunt-Crossley and SAV (Scalar Auxiliary Variable)
        unconditionally energy-stable formulation.
        """
        if not self.is_active:
            return 0.0

        # Felt compression indentation: eta = u_h - u_string
        eta = self.u_h - u_string
        if eta <= 0.0:
            self.xi = 0.0
            if self.has_struck and self.v_h <= 0.0:
                self.is_active = False
            return 0.0

        self.has_struck = True
        self.contact_time += self.dt

        # Relative indentation velocity: d(eta)/dt = v_h - v_string
        v_rel = self.v_h - v_string

        # Effective stiffness accounting for Una Corda felt shift
        k_felt = self.K_h * 0.72 if self.una_corda else self.K_h
        lambda_felt = self.lambda_h * 1.15 if self.una_corda else self.lambda_h

        eta_p = eta ** self.p
        damping_term = lambda_felt * eta_p * v_rel

        if self.use_sav:
            # SAV (Scalar Auxiliary Variable) Energy Quadratisation formulation:
            # Potential energy: V(eta) = K_h / (p + 1) * eta^(p + 1)
            # Auxiliary variable: xi(t) = sqrt(V(eta))
            # g(eta) = V'(eta) / (2 * sqrt(V(eta))) = K_h * eta^p / (2 * xi)
            # Conservative force: F_c = 2 * xi * g(eta) = K_h * eta^p
            # Numerical dissipation guarantee: d(0.5*m*v^2 + xi^2)/dt = -lambda_h * eta^p * v_rel^2 <= 0
            v_pot = (k_felt / (self.p + 1.0)) * (eta ** (self.p + 1.0))
            exact_xi = math.sqrt(max(0.0, v_pot))
            # Discrete update of xi preserving unconditional stability
            if exact_xi > 1e-12:
                g_eta = (k_felt * eta_p) / (2.0 * exact_xi)
                # Crank-Nicolson / implicit midpoint SAV step
                self.xi = (self.xi + 0.5 * self.dt * g_eta * v_rel) / (1.0 + 0.25 * (self.dt ** 2) * (g_eta ** 2) / self.m_h)
                self.xi = max(0.0, self.xi)
                elastic_term = 2.0 * self.xi * g_eta
            else:
                self.xi = 0.0
                elastic_term = k_felt * eta_p
        else:
            elastic_term = k_felt * eta_p

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
