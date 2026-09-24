"""Physical parameter schemas for piano strings, hammers, and keys."""

from dataclasses import dataclass
import math
from typing import Optional, List


@dataclass
class StringPhysicalParameters:
    """Continuous mechanical parameters for a single stiff piano string.
    
    References Euler-Bernoulli damped stiff string wave equation:
      rho * A * d^2 u / dt^2 = T0 * d^2 u / dx^2 - E * I * d^4 u / dx^4
                               - 2 * rho * A * sigma0 * du/dt
                               + 2 * rho * A * sigma1 * d^3 u / (dt dx^2)
    """
    length: float                 # L: active vibrating length (meters)
    radius: float                 # r: string wire radius (meters)
    density: float                # rho: mass density (kg/m^3, e.g. 7850 for carbon steel)
    youngs_modulus: float         # E: Young's elasticity modulus (Pa, e.g. 2.0e11)
    tension: float                # T0: static longitudinal string tension (Newtons)
    sigma0: float                 # sigma0: air-viscosity damping coefficient (1/s)
    sigma1: float                 # sigma1: internal viscoelastic / thermal bending loss (m^2/s)
    strike_ratio: float = 0.12    # x_h / L: relative hammer strike position (~1/8)
    num_modes: int = 40           # M: modal expansion truncation order
    polarization_mistuning: float = 0.0012  # Relative detune between Vertical and Horizontal modes

    @property
    def cross_section_area(self) -> float:
        """A = pi * r^2"""
        return math.pi * (self.radius ** 2)

    @property
    def linear_density(self) -> float:
        """mu = rho * A"""
        return self.density * self.cross_section_area

    @property
    def moment_of_inertia(self) -> float:
        """I = pi * r^4 / 4"""
        return (math.pi * (self.radius ** 4)) / 4.0

    @property
    def fundamental_hz(self) -> float:
        """Ideal fundamental frequency: f0 = (1 / (2*L)) * sqrt(T0 / (rho * A))"""
        return (1.0 / (2.0 * self.length)) * math.sqrt(self.tension / self.linear_density)

    @property
    def inharmonicity_b(self) -> float:
        """Inharmonicity factor B = (pi^3 * E * r^4) / (4 * T0 * L^2)"""
        return (math.pi ** 3 * self.youngs_modulus * (self.radius ** 4)) / (4.0 * self.tension * (self.length ** 2))


@dataclass
class HammerPhysicalParameters:
    """Nonlinear Hunt-Crossley felt hammer parameters.
    
    F_h(t) = max(0, K_h * [eta]^p + lambda_h * [eta]^p * d(eta)/dt)
    m_h * d^2 u_h / dt^2 = -F_h
    """
    mass: float                   # m_h: hammer effective mass (kg)
    stiffness: float              # K_h: felt compression stiffness constant (N/m^p)
    exponent: float               # p: nonlinear felt compression exponent (1.8 ~ 3.5)
    dissipation: float            # lambda_h: hysteresis dissipation coefficient
    max_velocity: float = 5.0     # v_max: peak hammer strike velocity (m/s)
    velocity_gamma: float = 1.6   # gamma_felt: MIDI velocity mapping curvature


@dataclass
class KeyParameters:
    """Composite physical parameters defining a piano key (1 of 88 keys)."""
    midi_note: int
    pitch_name: str
    target_f0: float
    strings: List[StringPhysicalParameters]
    hammer: HammerPhysicalParameters
    num_unisons: int = 3
    detuning_cents: Optional[List[float]] = None

    def __post_init__(self):
        if self.detuning_cents is None:
            if self.num_unisons == 1:
                self.detuning_cents = [0.0]
            elif self.num_unisons == 2:
                self.detuning_cents = [-0.25, 0.25]
            else:
                self.detuning_cents = [-0.4, 0.0, 0.4]
