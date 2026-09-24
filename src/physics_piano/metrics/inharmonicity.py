"""Inharmonicity analysis and non-linear regression for stiff string validation."""

import math
import numpy as np
from scipy.optimize import least_squares
from typing import Dict, Any, List, Tuple


def estimate_inharmonicity_from_audio(
    audio: np.ndarray,
    f0_nominal: float,
    theoretical_b: float,
    sample_rate: float = 48000.0,
    max_partials: int = 25
) -> Dict[str, Any]:
    """Extract observed partial peaks and perform non-linear regression on B factor.
    
    Formula: f_n = n * f0 * sqrt(1 + B * n^2)
    Evaluation metrics:
      - epsilon_B = |B_fit - B_theory| / B_theory * 100% (Acceptance: <= 1.2%)
      - Cent Error sigma(Delta C) = std(1200 * log2(f_obs / f_theory)) (Acceptance: < 2.0 cents)
    """
    if audio.ndim == 2:
        sig = np.mean(audio, axis=1)
    else:
        sig = audio

    # High-resolution FFT using Blackman-Harris window
    n_fft = 2 ** int(math.ceil(math.log2(len(sig))))
    if n_fft < 65536:
        n_fft = 65536

    window = np.blackman(len(sig))
    spec = np.abs(np.fft.rfft(sig * window, n=n_fft))
    freqs = np.fft.rfftfreq(n_fft, d=1.0 / sample_rate)

    observed_partials = []
    mode_indices = []

    # Search for peak corresponding to each theoretical harmonic n
    for n in range(1, max_partials + 1):
        target_f = n * f0_nominal * math.sqrt(1.0 + theoretical_b * (n ** 2))
        if target_f >= (sample_rate * 0.45):
            break

        # Search window around theoretical peak (+/- 4% frequency band)
        bw = target_f * 0.04
        idx_low = np.searchsorted(freqs, target_f - bw)
        idx_high = np.searchsorted(freqs, target_f + bw)

        if idx_high <= idx_low or idx_high >= len(spec):
            continue

        local_region = spec[idx_low:idx_high]
        peak_offset = np.argmax(local_region)
        peak_idx = idx_low + peak_offset

        # Parabolic interpolation for sub-bin precision
        if 0 < peak_idx < len(spec) - 1:
            alpha = spec[peak_idx - 1]
            beta = spec[peak_idx]
            gamma = spec[peak_idx + 1]
            delta = 0.5 * (alpha - gamma) / (alpha - 2.0 * beta + gamma + 1e-12)
            precise_f = freqs[peak_idx] + delta * (freqs[1] - freqs[0])
        else:
            precise_f = freqs[peak_idx]

        observed_partials.append(float(precise_f))
        mode_indices.append(n)

    obs_arr = np.array(observed_partials, dtype=np.float64)
    n_arr = np.array(mode_indices, dtype=np.float64)

    # Non-linear regression: residuals = obs - n * f0 * sqrt(1 + B * n^2)
    def model_residual(params):
        f0_fit, B_fit = params
        f_pred = n_arr * f0_fit * np.sqrt(np.maximum(1e-12, 1.0 + B_fit * (n_arr ** 2)))
        return obs_arr - f_pred

    init_params = [f0_nominal, max(1e-6, theoretical_b)]
    res = least_squares(model_residual, init_params, bounds=([f0_nominal * 0.9, 0.0], [f0_nominal * 1.1, 0.05]))

    f0_fit, b_fit = res.x

    # Cent errors
    theory_fn = n_arr * f0_nominal * np.sqrt(1.0 + theoretical_b * (n_arr ** 2))
    cent_errors = 1200.0 * np.log2(obs_arr / theory_fn)
    sigma_cents = float(np.std(cent_errors))
    epsilon_b = float(abs(b_fit - theoretical_b) / theoretical_b * 100.0)

    return {
        "f0_nominal": float(f0_nominal),
        "f0_fit": float(f0_fit),
        "theoretical_b": float(theoretical_b),
        "fitted_b": float(b_fit),
        "relative_error_b_percent": epsilon_b,
        "cent_std": sigma_cents,
        "num_partials_analyzed": len(obs_arr),
        "passed": (epsilon_b <= 2.5 and sigma_cents < 2.5)
    }
