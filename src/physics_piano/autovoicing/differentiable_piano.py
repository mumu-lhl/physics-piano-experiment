"""Parameterized and differentiable modal synthesis loop for piano string-hammer-bridge dynamics."""

import math
import numpy as np
from typing import Dict, Tuple, Optional


class ParameterizedModalString:
    """Fast parameterized modal string model suitable for gradient-based or derivative-free optimization."""

    def __init__(
        self,
        length: float,
        radius: float,
        density: float,
        youngs_modulus: float,
        tension: float,
        sigma0: float,
        sigma1: float,
        strike_ratio: float,
        sample_rate: float = 48000.0,
        max_modes: int = 35,
    ):
        self.L = float(length)
        self.r = float(radius)
        self.rho = float(density)
        self.E = float(youngs_modulus)
        self.T0 = float(tension)
        self.sigma0 = float(sigma0)
        self.sigma1 = float(sigma1)
        self.strike_ratio = float(strike_ratio)
        self.sample_rate = float(sample_rate)
        self.dt = 1.0 / self.sample_rate
        self.max_modes = max_modes

        self._update_coefficients()

    def _update_coefficients(self):
        self.area = math.pi * (self.r ** 2)
        self.mu = self.rho * self.area
        self.omega_0 = (math.pi / self.L) * math.sqrt(max(1.0, self.T0) / max(1e-9, self.mu))
        self.B = (math.pi ** 3 * self.E * (self.r ** 4)) / (4.0 * max(1.0, self.T0) * (self.L ** 2))

        # Nyquist culling
        max_omega = 0.95 * math.pi * self.sample_rate
        all_n = np.arange(1, self.max_modes + 1, dtype=np.float64)
        all_omega = all_n * self.omega_0 * np.sqrt(1.0 + self.B * (all_n ** 2))
        valid_mask = all_omega <= max_omega

        if not np.any(valid_mask):
            self.n_modes = np.array([1.0], dtype=np.float64)
        else:
            self.n_modes = all_n[valid_mask]

        self.num_modes = len(self.n_modes)

        # Finite strike window
        w_h = 0.015
        sinc_w = np.sinc((self.n_modes * w_h) / (2.0 * self.L))
        self.phi_h = np.sin(self.n_modes * math.pi * self.strike_ratio) * sinc_w

        # Bridge coupling coefficients
        signs = np.where((self.n_modes.astype(int) - 1) % 2 == 0, 1.0, -1.0)
        self.bridge_coeff = self.T0 * signs * (self.n_modes * math.pi / self.L)

        self.omega_t = all_omega[:self.num_modes]
        self.gamma_t = self.sigma0 + self.sigma1 * ((self.n_modes * math.pi / self.L) ** 2)

        # Discrete matrix transitions
        omega_d = np.sqrt(np.maximum(1e-6, self.omega_t ** 2 - self.gamma_t ** 2))
        decay = np.exp(-self.gamma_t * self.dt)
        sin_wd = np.sin(omega_d * self.dt)
        cos_wd = np.cos(omega_d * self.dt)

        self.phi_11 = decay * (cos_wd + (self.gamma_t / omega_d) * sin_wd)
        self.phi_12 = decay * (sin_wd / omega_d)
        self.phi_21 = -decay * ((self.omega_t ** 2 / omega_d) * sin_wd)
        self.phi_22 = decay * (cos_wd - (self.gamma_t / omega_d) * sin_wd)

        self.gamma_1 = (1.0 / (self.omega_t ** 2)) * (1.0 - self.phi_11)
        self.gamma_2 = self.phi_12

        self.force_scale = 2.0 / (self.mu * self.L)


class DifferentiablePianoNote:
    """Complete voice simulator for parameter auto-tuning against target reference audio."""

    def __init__(
        self,
        midi_note: int,
        target_f0: float,
        sample_rate: float = 48000.0,
        num_modes: int = 35,
    ):
        self.midi_note = midi_note
        self.target_f0 = target_f0
        self.sample_rate = sample_rate
        self.num_modes = num_modes

        norm_key = (midi_note - 21) / 87.0
        self.length = 1.85 - (midi_note - 21) * 0.045 if midi_note <= 40 else 0.065 + (0.95 - 0.065) * ((1.0 - norm_key) ** 1.35)
        self.radius = 0.00048 - norm_key * 0.00015
        self.density = 7850.0
        self.youngs_modulus = 2.0e11
        area = math.pi * (self.radius ** 2)
        mu = self.density * area
        self.tension = mu * ((2.0 * self.length * target_f0) ** 2)

        self.hammer_mass = 0.0115 - 0.0063 * norm_key
        self.hammer_stiffness = 1.5e9 * (10.0 ** (norm_key * 2.8))
        self.hammer_exponent = 2.0 + 1.1 * norm_key
        self.hammer_dissipation = 1.5e4 + 4.0e4 * norm_key

        self.sigma0 = 0.8 - 0.4 * norm_key
        self.sigma1 = 5.0e-6 + 3.0e-5 * norm_key

    def synthesize(
        self,
        duration: float,
        velocity: float = 0.75,
        params_override: Optional[Dict[str, float]] = None,
    ) -> np.ndarray:
        """Simulate piano note and return synthesized mono waveform."""
        p = {
            "length": self.length,
            "radius": self.radius,
            "density": self.density,
            "youngs_modulus": self.youngs_modulus,
            "tension": self.tension,
            "sigma0": self.sigma0,
            "sigma1": self.sigma1,
            "hammer_mass": self.hammer_mass,
            "hammer_stiffness": self.hammer_stiffness,
            "hammer_exponent": self.hammer_exponent,
            "hammer_dissipation": self.hammer_dissipation,
        }
        if params_override:
            p.update(params_override)

        string = ParameterizedModalString(
            length=p["length"],
            radius=p["radius"],
            density=p["density"],
            youngs_modulus=p["youngs_modulus"],
            tension=p["tension"],
            sigma0=p["sigma0"],
            sigma1=p["sigma1"],
            strike_ratio=0.12,
            sample_rate=self.sample_rate,
            max_modes=self.num_modes,
        )

        n_samples = int(duration * self.sample_rate)
        dt = 1.0 / self.sample_rate
        out = np.zeros(n_samples, dtype=np.float64)

        # Hammer states
        v_h = 5.2 * (velocity ** 1.55)
        u_h = 0.0
        m_h = p["hammer_mass"]
        k_h = p["hammer_stiffness"]
        p_exp = p["hammer_exponent"]
        lambda_h = p["hammer_dissipation"]
        hammer_active = True
        has_struck = False

        # Modal string states
        q_t = np.zeros(string.num_modes, dtype=np.float64)
        v_t = np.zeros(string.num_modes, dtype=np.float64)

        # Soundboard resonator (1-pole leak filter approximating radiation)
        sb_state = 0.0

        for s in range(n_samples):
            # 1. Hammer force
            if hammer_active:
                u_str = np.sum(q_t * string.phi_h)
                v_str = np.sum(v_t * string.phi_h)
                eta = u_h - u_str
                if eta > 0.0:
                    has_struck = True
                    v_rel = v_h - v_str
                    f_h = max(0.0, k_h * (eta ** p_exp) + lambda_h * (eta ** p_exp) * v_rel)
                else:
                    f_h = 0.0
                    if has_struck and v_h <= 0.0:
                        hammer_active = False

                # Advance hammer
                acc = -f_h / m_h
                u_h += v_h * dt + 0.5 * acc * (dt ** 2)
                v_h += acc * dt
            else:
                f_h = 0.0

            # 2. String modal update
            f_in = string.force_scale * f_h
            f_mode = f_in * string.phi_h
            new_q = string.phi_11 * q_t + string.phi_12 * v_t + string.gamma_1 * f_mode
            new_v = string.phi_21 * q_t + string.phi_22 * v_t + string.gamma_2 * f_mode
            q_t = new_q
            v_t = new_v

            # 3. Bridge force & Soundboard radiation
            f_bridge = np.sum(q_t * string.bridge_coeff)
            sb_state = 0.985 * sb_state + f_bridge * 1e-4
            out[s] = sb_state

        return out
