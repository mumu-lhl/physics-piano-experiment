"""Automated Voicing Optimizer: Inverse calibration of physical piano parameters against target audio recordings."""

import json
import math
import numpy as np
import scipy.optimize as opt
from typing import Dict, Any, Optional

from physics_piano.metrics.alignment import align_audio_onsets
from physics_piano.metrics.mrsl import compute_mrsl
from physics_piano.metrics.inharmonicity import estimate_inharmonicity_from_audio
from .differentiable_piano import DifferentiablePianoNote


class VoicingCalibrationResult:
    """Stores the result of automated voicing parameter calibration."""

    def __init__(
        self,
        midi_note: int,
        target_f0: float,
        initial_loss: float,
        final_loss: float,
        calibrated_params: Dict[str, float],
    ):
        self.midi_note = midi_note
        self.target_f0 = target_f0
        self.initial_loss = initial_loss
        self.final_loss = final_loss
        self.calibrated_params = calibrated_params

    def to_dict(self) -> Dict[str, Any]:
        return {
            "midi_note": self.midi_note,
            "target_f0": self.target_f0,
            "initial_loss": self.initial_loss,
            "final_loss": self.final_loss,
            "calibrated_params": self.calibrated_params,
        }

    def save_json(self, path: str):
        with open(path, "w", encoding="utf-8") as f:
            json.dump(self.to_dict(), f, indent=2)


class AutoVoicer:
    """Calibrates physical model parameters to match target audio recordings using Multi-Resolution Spectral Loss."""

    def __init__(self, sample_rate: float = 48000.0, num_modes: int = 35):
        self.sample_rate = sample_rate
        self.num_modes = num_modes

    def calibrate_note(
        self,
        target_audio: np.ndarray,
        target_sr: float,
        midi_note: int,
        target_f0: float,
        velocity: float = 0.75,
        max_iterations: int = 25,
    ) -> VoicingCalibrationResult:
        """Run iterative optimization to calibrate physical parameters to match target audio."""
        # 1. Normalize target audio
        if target_audio.ndim > 1:
            target_audio = target_audio.mean(axis=1)
        ref = target_audio.astype(np.float64)
        peak = np.max(np.abs(ref))
        if peak > 0:
            ref /= peak

        # Resample to simulation sample_rate if needed
        if target_sr != self.sample_rate:
            from scipy import signal
            target_len = int(len(ref) * self.sample_rate / target_sr)
            ref = signal.resample(ref, target_len)

        eval_dur = min(1.5, len(ref) / self.sample_rate)
        n_eval = int(eval_dur * self.sample_rate)
        ref_segment = ref[:n_eval]

        # 2. Setup differentiable model
        note = DifferentiablePianoNote(
            midi_note=midi_note,
            target_f0=target_f0,
            sample_rate=self.sample_rate,
            num_modes=self.num_modes,
        )

        # Baseline parameters: scale multipliers [k_h_scale, p_scale, inharm_scale, damping_scale]
        # x = [k_h_mult, p_mult, inharm_mult, damping_mult] centered at 1.0
        x0 = np.array([1.0, 1.0, 1.0, 1.0], dtype=np.float64)
        bounds = [(0.3, 3.0), (0.7, 1.5), (0.2, 2.5), (0.3, 3.0)]

        def loss_fn(x: np.ndarray) -> float:
            k_h_mult, p_mult, inharm_mult, damp_mult = x
            overrides = {
                "hammer_stiffness": note.hammer_stiffness * k_h_mult,
                "hammer_exponent": np.clip(note.hammer_exponent * p_mult, 1.5, 3.5),
                "youngs_modulus": note.youngs_modulus * inharm_mult,
                "sigma0": note.sigma0 * damp_mult,
                "sigma1": note.sigma1 * damp_mult,
            }
            synth = note.synthesize(eval_dur, velocity=velocity, params_override=overrides)
            synth_peak = np.max(np.abs(synth))
            if synth_peak > 0:
                synth = synth / synth_peak

            aligned_synth, aligned_ref = align_audio_onsets(synth[:n_eval], ref_segment)
            mrsl = compute_mrsl(aligned_synth, aligned_ref, fft_sizes=[512, 1024])
            return float(mrsl["total_mrsl_loss"])

        # Compute initial loss
        initial_loss = loss_fn(x0)

        # 3. Minimize loss using Nelder-Mead simplex optimization
        res = opt.minimize(
            loss_fn,
            x0,
            method="Nelder-Mead",
            bounds=bounds,
            options={"maxiter": max_iterations, "disp": False},
        )

        final_loss = float(res.fun)
        opt_x = res.x

        calibrated = {
            "hammer_stiffness_multiplier": float(opt_x[0]),
            "hammer_exponent_multiplier": float(opt_x[1]),
            "inharmonicity_multiplier": float(opt_x[2]),
            "damping_multiplier": float(opt_x[3]),
            "calibrated_hammer_stiffness": float(note.hammer_stiffness * opt_x[0]),
            "calibrated_hammer_exponent": float(np.clip(note.hammer_exponent * opt_x[1], 1.5, 3.5)),
            "calibrated_youngs_modulus": float(note.youngs_modulus * opt_x[2]),
            "calibrated_sigma0": float(note.sigma0 * opt_x[3]),
            "calibrated_sigma1": float(note.sigma1 * opt_x[3]),
        }

        return VoicingCalibrationResult(
            midi_note=midi_note,
            target_f0=target_f0,
            initial_loss=initial_loss,
            final_loss=final_loss,
            calibrated_params=calibrated,
        )
