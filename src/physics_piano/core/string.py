"""Euler-Bernoulli stiff string model with dual-polarization modal state-space dynamics."""

import math
import numpy as np
from typing import Tuple

from physics_piano.params.schema import StringPhysicalParameters


class StiffStringModal:
    """Discrete modal representation of a stiff piano string with dual polarization.
    
    Equations:
      rho * A * d^2 u / dt^2 = T0 * d^2 u / dx^2 - E * I * d^4 u / dx^4
                               - 2 * rho * A * sigma0 * du/dt
                               + 2 * rho * A * sigma1 * d^3 u / (dt dx^2)
    Modal angular frequencies:
      omega_n = n * omega_0 * sqrt(1 + B * n^2)
    Modal damping:
      gamma_n = sigma0 + sigma1 * (n * pi / L)^2
    """

    def __init__(self, params: StringPhysicalParameters, sample_rate: float = 48000.0):
        self.params = params
        self.sample_rate = float(sample_rate)
        self.dt = 1.0 / self.sample_rate

        self.L = params.length
        self.r = params.radius
        self.rho = params.density
        self.E = params.youngs_modulus
        self.T0 = params.tension
        self.sigma0 = params.sigma0
        self.sigma1 = params.sigma1
        self.x_h = params.strike_ratio * self.L

        self.mu = self.rho * (math.pi * (self.r ** 2))
        self.omega_0 = (math.pi / self.L) * math.sqrt(self.T0 / self.mu)
        self.B = (math.pi ** 3 * self.E * (self.r ** 4)) / (4.0 * self.T0 * (self.L ** 2))

        # Dynamic mode truncation at Nyquist frequency (0.95 * fs / 2) to eliminate ultrasonic aliasing
        max_omega = 0.95 * math.pi * self.sample_rate
        all_n = np.arange(1, params.num_modes + 1, dtype=np.float64)
        all_omega = all_n * self.omega_0 * np.sqrt(1.0 + self.B * (all_n ** 2))
        valid_mask = all_omega <= max_omega

        # Always keep at least the fundamental mode
        if not np.any(valid_mask):
            self.n_modes = np.array([1.0], dtype=np.float64)
        else:
            self.n_modes = all_n[valid_mask]

        self.M = len(self.n_modes)

        # Finite felt width sinc spatial windowing:
        # felt width: ~2.0cm in bass down to ~1.0cm in high treble
        norm_k = min(1.0, max(0.0, (params.fundamental_hz - 27.5) / (4186.0 - 27.5)))
        w_h = 0.020 - (0.010 * norm_k)
        sinc_window = np.sinc((self.n_modes * w_h) / (2.0 * self.L))

        # Spatial basis functions at hammer position: phi_n(x_h) = sin(n * pi * x_h / L) * sinc
        self.phi_h = np.sin(self.n_modes * math.pi * self.x_h / self.L) * sinc_window
        
        # Spatial derivative coefficients at bridge x = L:
        # F_bridge = T0 * sum((-1)^(n-1) * (n * pi / L) * q_n)
        self.bridge_coeff = self.T0 * ((-1.0) ** (self.n_modes - 1)) * (self.n_modes * math.pi / self.L)

        # Modal force projection factor: 2 / (mu * L)
        self.force_scale = 2.0 / (self.mu * self.L)

        # Precompute state-space transition matrices for Vertical (T) and Horizontal (P)
        self._init_polarizations()

        # States: shape (M, 2) where col 0 is q_n (disp), col 1 is dq_n (velocity)
        self.state_T = np.zeros((self.M, 2), dtype=np.float64)
        self.state_P = np.zeros((self.M, 2), dtype=np.float64)

        # Damper active status (increases damping)
        self.damper_active = True
        self.damper_decay_mult = 1.0

    def _init_polarizations(self):
        """Precompute discrete state-space transition matrices Phi and Gamma."""
        # Vertical frequencies and damping
        omega_T = self.n_modes * self.omega_0 * np.sqrt(1.0 + self.B * (self.n_modes ** 2))
        gamma_T = self.sigma0 + self.sigma1 * ((self.n_modes * math.pi / self.L) ** 2)

        # Horizontal frequencies (slightly detuned due to bridge anisotropy)
        omega_P = omega_T * (1.0 + self.params.polarization_mistuning)
        # Horizontal polarization has slightly less internal loss and far less radiation loss
        gamma_P = self.sigma0 * 0.7 + self.sigma1 * 0.8 * ((self.n_modes * math.pi / self.L) ** 2)

        self.Phi_T, self.Gamma_T = self._compute_transition_matrices(omega_T, gamma_T)
        self.Phi_P, self.Gamma_P = self._compute_transition_matrices(omega_P, gamma_P)

        self.omega_T = omega_T
        self.gamma_T = gamma_T
        self.omega_P = omega_P
        self.gamma_P = gamma_P

    def _compute_transition_matrices(self, omega: np.ndarray, gamma: np.ndarray) -> Tuple[np.ndarray, np.ndarray]:
        """Compute exact continuous-to-discrete matrix exponential state transition.
        
        System:
          d/dt [q; v] = [0, 1; -omega^2, -2*gamma] [q; v] + [0; 1] f
        """
        omega_d = np.sqrt(np.maximum(1e-6, omega ** 2 - gamma ** 2))
        dt = self.dt
        decay = np.exp(-gamma * dt)

        sin_wd = np.sin(omega_d * dt)
        cos_wd = np.cos(omega_d * dt)

        # Phi elements for each mode
        phi_11 = decay * (cos_wd + (gamma / omega_d) * sin_wd)
        phi_12 = decay * (sin_wd / omega_d)
        phi_21 = -decay * ((omega ** 2 / omega_d) * sin_wd)
        phi_22 = decay * (cos_wd - (gamma / omega_d) * sin_wd)

        # Gamma (input integral)
        gamma_1 = (1.0 / (omega ** 2)) * (1.0 - decay * (cos_wd + (gamma / omega_d) * sin_wd))
        gamma_2 = phi_12

        # Reshape to (M, 2, 2) and (M, 2)
        Phi = np.zeros((self.M, 2, 2), dtype=np.float64)
        Phi[:, 0, 0] = phi_11
        Phi[:, 0, 1] = phi_12
        Phi[:, 1, 0] = phi_21
        Phi[:, 1, 1] = phi_22

        Gamma = np.zeros((self.M, 2), dtype=np.float64)
        Gamma[:, 0] = gamma_1
        Gamma[:, 1] = gamma_2

        return Phi, Gamma

    def set_damper(self, active: bool, depth: float = 1.0):
        """Configure damper state."""
        self.damper_active = active
        self.damper_decay_mult = 1.0 + (15.0 * depth if active else 0.0)

    def get_strike_displacement_and_velocity(self) -> Tuple[float, float]:
        """Compute transverse displacement and velocity at hammer contact point x_h."""
        u_h = float(np.dot(self.state_T[:, 0], self.phi_h))
        v_h = float(np.dot(self.state_T[:, 1], self.phi_h))
        return u_h, v_h

    def get_bridge_forces(self) -> Tuple[float, float]:
        """Compute boundary transverse forces exerted on the bridge."""
        f_bridge_T = float(np.dot(self.state_T[:, 0], self.bridge_coeff))
        f_bridge_P = float(np.dot(self.state_P[:, 0], self.bridge_coeff))
        return f_bridge_T, f_bridge_P

    def step(self, f_hammer: float, f_coupling_T: float = 0.0, f_coupling_P: float = 0.0):
        """Advance modal states by one time step dt."""
        f_modal_T = (self.force_scale * self.phi_h * f_hammer) + (self.force_scale * f_coupling_T)
        f_modal_P = self.force_scale * f_coupling_P

        # Advance state_T
        q_T = self.state_T[:, 0]
        v_T = self.state_T[:, 1]
        new_q_T = self.Phi_T[:, 0, 0] * q_T + self.Phi_T[:, 0, 1] * v_T + self.Gamma_T[:, 0] * f_modal_T
        new_v_T = self.Phi_T[:, 1, 0] * q_T + self.Phi_T[:, 1, 1] * v_T + self.Gamma_T[:, 1] * f_modal_T

        # Advance state_P
        q_P = self.state_P[:, 0]
        v_P = self.state_P[:, 1]
        new_q_P = self.Phi_P[:, 0, 0] * q_P + self.Phi_P[:, 0, 1] * v_P + self.Gamma_P[:, 0] * f_modal_P
        new_v_P = self.Phi_P[:, 1, 0] * q_P + self.Phi_P[:, 1, 1] * v_P + self.Gamma_P[:, 1] * f_modal_P

        # Damper friction damping when active
        if self.damper_active and self.damper_decay_mult > 1.0:
            damper_damping = np.exp(-self.damper_decay_mult * 30.0 * self.dt)
            new_q_T *= damper_damping
            new_v_T *= damper_damping
            new_q_P *= damper_damping
            new_v_P *= damper_damping

        self.state_T[:, 0] = new_q_T
        self.state_T[:, 1] = new_v_T
        self.state_P[:, 0] = new_q_P
        self.state_P[:, 1] = new_v_P
