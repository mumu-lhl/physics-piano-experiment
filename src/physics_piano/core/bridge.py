"""Bridge anisotropic admittance, cross-polarization coupling, and soundboard radiation model."""

import math
import numpy as np
from typing import Tuple, List


class BridgeSoundboard:
    """Piano bridge mechanical admittance tensor and acoustic soundboard radiation.
    
    Implements:
      [V_T; V_P] = [Y_TT, Y_TP; Y_PT, Y_PP] * [F_T; F_P]
    Where:
      Re(Y_TT) >> Re(Y_PP) gives rise to two-stage biexponential decay:
        - Prompt sound (vertical polarization, rapid radiation)
        - Aftersound (horizontal polarization, weak radiation, sustained decay)
    """

    def __init__(self, sample_rate: float = 48000.0):
        self.sample_rate = float(sample_rate)
        self.dt = 1.0 / self.sample_rate

        # Admittance coefficients (typical values for spruce piano soundboard at bridge)
        self.y_tt = 1.8e-4     # Vertical admittance (high mobility / prompt decay)
        self.y_pp = 2.2e-5     # Horizontal admittance (low mobility / aftersound)
        self.y_tp = 3.5e-5     # Cross-polarization coupling term (anisotropy)
        self.y_pt = 3.5e-5
        self.y_tl = 1.5e-5     # Longitudinal-transverse coupling (phantom partials)
        self.y_lt = 1.5e-5
        self.y_ll = 4.0e-5     # Longitudinal mobility
        self.y_pl = 0.5e-5
        self.y_lp = 0.5e-5

        # Radiation mode: "modal" (default low CPU biquad bank) or "upols" (partitioned FFT block convolution)
        self.radiation_mode = "modal"
        self._upols_convolver = None

        # Soundboard multi-modal body filter (approximating 2D orthotropic plate resonance)
        # Resonant frequencies (Hz), bandwidths (Hz), and relative modal gains
        self.body_freqs = np.array([85.0, 140.0, 220.0, 310.0, 480.0, 720.0, 1150.0, 1900.0, 3200.0])
        self.body_q = np.array([12.0, 14.0, 16.0, 18.0, 20.0, 22.0, 25.0, 28.0, 30.0])
        self.body_gains = np.array([1.2, 1.5, 1.4, 1.1, 0.9, 0.7, 0.5, 0.35, 0.2])

        self.num_body_modes = len(self.body_freqs)
        self._init_soundboard_filters()

    def set_radiation_mode(self, mode: str):
        """Switch acoustic soundboard radiation mode ('modal' or 'upols')."""
        if mode not in ("modal", "upols"):
            raise ValueError(f"Invalid radiation mode: {mode}. Must be 'modal' or 'upols'")
        self.radiation_mode = mode
        if mode == "upols" and self._upols_convolver is None:
            from physics_piano.dsp.upols import UPOLSConvolver, generate_orthotropic_soundboard_ir
            ir_l, ir_r = generate_orthotropic_soundboard_ir(self.sample_rate, duration=0.6)
            stereo_ir = np.column_stack([ir_l, ir_r])
            self._upols_convolver = UPOLSConvolver(stereo_ir, block_size=128)

    def _init_soundboard_filters(self):
        """Initialize 2nd-order resonator IIR filters for soundboard acoustic radiation."""
        # Transfer function: H(s) = omega_0^2 / (s^2 + (omega_0/Q)*s + omega_0^2)
        # Using standard digital biquad resonator formulas
        w0 = 2.0 * math.pi * self.body_freqs / self.sample_rate
        alpha = np.sin(w0) / (2.0 * self.body_q)
        cos_w0 = np.cos(w0)

        # Biquad coefficients: b0, b1, b2, a1, a2 (normalized by a0)
        a0 = 1.0 + alpha
        self.b0 = (alpha * self.body_gains) / a0
        self.b1 = np.zeros_like(self.b0)
        self.b2 = (-alpha * self.body_gains) / a0
        self.a1 = (-2.0 * cos_w0) / a0
        self.a2 = (1.0 - alpha) / a0

        # Filter states: shape (num_modes, 2) for x[n-1], x[n-2] and y[n-1], y[n-2]
        self.sb_x1 = np.zeros(self.num_body_modes, dtype=np.float64)
        self.sb_x2 = np.zeros(self.num_body_modes, dtype=np.float64)
        self.sb_y1 = np.zeros(self.num_body_modes, dtype=np.float64)
        self.sb_y2 = np.zeros(self.num_body_modes, dtype=np.float64)

    def calculate_coupling_forces(
        self,
        f_bridge_T: float,
        f_bridge_P: float,
        f_bridge_L: float = 0.0
    ) -> Tuple[float, float, float]:
        """Compute 3D cross-polarization reaction forces and soundboard driver force.
        
        Args:
            f_bridge_T: Total vertical force from all strings at the bridge
            f_bridge_P: Total horizontal force from all strings at the bridge
            f_bridge_L: Total longitudinal tension perturbation force
        Returns:
            f_react_T: Reaction force on vertical string modes
            f_react_P: Reaction force on horizontal string modes
            f_soundboard: Net scalar force driving soundboard acoustic radiation
        """
        # 3x3 Cross-admittance coupling velocities
        v_bridge_T = self.y_tt * f_bridge_T + self.y_tp * f_bridge_P + self.y_tl * f_bridge_L
        v_bridge_P = self.y_pt * f_bridge_T + self.y_pp * f_bridge_P + self.y_pl * f_bridge_L

        # Reaction forces opposing string motion at boundary
        f_react_T = -v_bridge_T * 8.0
        f_react_P = -v_bridge_P * 12.0

        # Radiated soundboard driving force combines vertical prompt energy, horizontal aftersound,
        # and longitudinal phantom partial excitation
        f_soundboard = 0.82 * f_bridge_T + 0.15 * f_bridge_P + 0.28 * f_bridge_L

        return f_react_T, f_react_P, f_soundboard

    def step_soundboard(self, f_in: float, pan: float = 0.5) -> Tuple[float, float]:
        """Process one sample through soundboard modal filters with stereo spatialization.
        
        Args:
            f_in: Soundboard driving force
            pan: Stereo panning (0.0=left/bass, 1.0=right/treble)
        Returns:
            (left, right): Stereo audio samples
        """
        if self.radiation_mode == "upols" and self._upols_convolver is not None:
            # Partitioned UPOLS FFT convolution
            stereo_out = self._upols_convolver.process_sample(f_in)
            left_sample = float(stereo_out[0])
            right_sample = float(stereo_out[1]) if len(stereo_out) > 1 else left_sample
            return left_sample, right_sample

        # Vectorized biquad evaluation
        y_modes = (self.b0 * f_in + self.b2 * self.sb_x2
                   - self.a1 * self.sb_y1 - self.a2 * self.sb_y2)

        # Update state buffers
        self.sb_x2[:] = self.sb_x1
        self.sb_x1[:] = f_in
        self.sb_y2[:] = self.sb_y1
        self.sb_y1[:] = y_modes

        # Acoustic radiation predominantly emanates from soundboard plate resonance
        modal_sound = float(np.sum(y_modes))
        soundboard_out = 0.92 * modal_sound + 0.08 * f_in * 1e-4

        # Equal-power stereo panning across piano width
        pan_clamped = max(0.0, min(1.0, float(pan)))
        left_gain = math.cos(pan_clamped * math.pi * 0.5)
        right_gain = math.sin(pan_clamped * math.pi * 0.5)

        return soundboard_out * left_gain, soundboard_out * right_gain
