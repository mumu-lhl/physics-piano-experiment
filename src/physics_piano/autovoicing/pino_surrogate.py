"""Physics-Informed Surrogate / Operator for 2D/3D Orthotropic Soundboard Dynamics."""

import math
import numpy as np
from typing import Dict, List, Tuple


class SoundboardPINOSurrogate:
    """Physics-informed surrogate for orthotropic spruce soundboard plate resonances.
    
    Predicts 2D modal frequencies, bridge driving point mobility, and radiation
    efficiency based on Mindlin-Timoshenko orthotropic plate equations.
    """

    def __init__(
        self,
        length_x: float = 1.95,  # Soundboard length (along wood grain)
        length_y: float = 1.45,  # Soundboard width (across grain)
        thickness: float = 0.009, # 9mm tapered spruce
        density: float = 430.0,   # Sitka spruce density (kg/m^3)
        youngs_modulus_x: float = 1.1e10, # Along grain (Pa)
        youngs_modulus_y: float = 6.5e8,  # Cross grain (Pa)
        poisson_ratio: float = 0.38,
    ):
        self.Lx = length_x
        self.Ly = length_y
        self.h = thickness
        self.rho = density
        self.Ex = youngs_modulus_x
        self.Ey = youngs_modulus_y
        self.nu = poisson_ratio

        # Flexural rigidities
        factor = (self.h ** 3) / (12.0 * (1.0 - self.nu ** 2))
        self.Dx = self.Ex * factor
        self.Dy = self.Ey * factor
        self.Gxy = 7.5e8  # Shear modulus
        self.Dxy = (self.nu * math.sqrt(self.Ex * self.Ey) + 2.0 * self.Gxy * (1.0 - self.nu ** 2)) * factor

        self.mass_per_area = self.rho * self.h

    def predict_modal_frequencies(self, max_m: int = 8, max_n: int = 8) -> List[Tuple[int, int, float, float]]:
        """Compute 2D orthotropic plate modes (m, n, freq_hz, radiation_factor)."""
        modes = []
        for m in range(1, max_m + 1):
            for n in range(1, max_n + 1):
                kx = (m * math.pi) / self.Lx
                ky = (n * math.pi) / self.Ly

                d_term = self.Dx * (kx ** 4) + 2.0 * self.Dxy * (kx ** 2) * (ky ** 2) + self.Dy * (ky ** 4)
                omega_sq = d_term / self.mass_per_area
                omega = math.sqrt(max(1.0, omega_sq))
                f_hz = omega / (2.0 * math.pi)

                # Acoustic radiation efficiency increases above coincidence frequency (~1.2 kHz)
                rad_factor = min(1.0, max(0.15, (f_hz / 1200.0) ** 1.2))

                if f_hz <= 8000.0:
                    modes.append((m, n, f_hz, rad_factor))

        modes.sort(key=lambda x: x[2])
        return modes

    def compute_driving_point_mobility(self, freqs: np.ndarray, x_drive: float = 0.65, y_drive: float = 0.40) -> np.ndarray:
        """Surrogate computation of bridge driving point mechanical mobility Y(f) = v / F."""
        modes = self.predict_modal_frequencies(max_m=10, max_n=10)
        mobility = np.zeros(len(freqs), dtype=np.complex128)

        total_mass = self.mass_per_area * self.Lx * self.Ly

        for m, n, fn, _ in modes:
            # Mode shape phi_mn at drive point
            phi = math.sin((m * math.pi * x_drive) / self.Lx) * math.sin((n * math.pi * y_drive) / self.Ly)
            wn = 2.0 * math.pi * fn
            modal_mass = total_mass * 0.25 # For simply supported plate
            zeta = 0.025 + 0.015 * (fn / 2000.0) # Modal damping ratio

            for i, f in enumerate(freqs):
                w = 2.0 * math.pi * f
                # Mechanical admittance for harmonic oscillator
                denom = modal_mass * (wn ** 2 - w ** 2 + 2j * zeta * wn * w)
                mobility[i] += (1j * w * (phi ** 2)) / denom

        return mobility
