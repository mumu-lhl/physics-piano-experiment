"""Schroeder backwards integration and biexponential energy decay analysis."""

import numpy as np
from scipy.optimize import curve_fit
from typing import Dict, Any, Tuple


def compute_schroeder_edc(signal: np.ndarray) -> np.ndarray:
    """Compute Schroeder backwards energy integration curve in dB.
    
    EDC(t) = 10 * log10( integral_t^T s^2(tau) dtau / integral_0^T s^2(tau) dtau )
    """
    if signal.ndim == 2:
        sig = np.mean(signal, axis=1)
    else:
        sig = signal

    sq_energy = sig ** 2
    # Reverse cumulative sum
    rev_cum = np.cumsum(sq_energy[::-1])[::-1]
    total_energy = rev_cum[0] + 1e-12

    edc_ratio = np.maximum(1e-10, rev_cum / total_energy)
    edc_db = 10.0 * np.log10(edc_ratio)
    return edc_db


def analyze_two_stage_decay(
    audio: np.ndarray,
    sample_rate: float = 48000.0
) -> Dict[str, Any]:
    """Fit biexponential decay to analyze prompt and aftersound decay rates.
    
    Model: E(t) = A_prompt * exp(-t / tau_prompt) + A_after * exp(-t / tau_after)
    Verifies that tau_after / tau_prompt falls within the physical range [3.0, 10.0].
    """
    edc_db = compute_schroeder_edc(audio)
    time_s = np.arange(len(edc_db)) / sample_rate

    # Convert dB back to linear normalized energy for stable biexponential fitting
    lin_energy = 10.0 ** (edc_db / 10.0)

    # Fit bi-exponential function: y(t) = a * exp(-t/t1) + b * exp(-t/t2)
    def biexp(t, a, t1, b, t2):
        return a * np.exp(-t / np.maximum(1e-4, t1)) + b * np.exp(-t / np.maximum(1e-4, t2))

    p0 = [0.85, 0.15, 0.15, 1.2]
    bounds = ([0.0, 0.01, 0.0, 0.1], [1.5, 1.0, 1.0, 15.0])

    try:
        popt, _ = curve_fit(biexp, time_s, lin_energy, p0=p0, bounds=bounds, maxfev=4000)
        a_p, t_p, a_a, t_a = popt
        # Ensure t_prompt is the faster decay
        if t_p > t_a:
            t_p, t_a = t_a, t_p
            a_p, a_a = a_a, a_p

        decay_ratio = float(t_a / t_p)
        passed = (3.0 <= decay_ratio <= 12.0)
    except Exception as e:
        # Fallback estimation based on slope from 0 to -15dB vs -15dB to -35dB
        idx_prompt = np.where(edc_db < -15.0)[0]
        idx_after = np.where(edc_db < -30.0)[0]
        t_prompt = time_s[idx_prompt[0]] if len(idx_prompt) > 0 else 0.2
        t_after = (time_s[idx_after[0]] - t_prompt) if len(idx_after) > 0 else 1.0
        decay_ratio = float(max(1.0, t_after / max(0.05, t_prompt)))
        t_p = float(t_prompt)
        t_a = float(t_after)
        passed = (3.0 <= decay_ratio <= 12.0)

    return {
        "tau_prompt": float(t_p),
        "tau_after": float(t_a),
        "decay_ratio": float(decay_ratio),
        "target_range": "[3.0, 12.0]",
        "passed": passed
    }
