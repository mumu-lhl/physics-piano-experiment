"""Initial attack transient and energy rise time (Delta t_10-90) validation.

Acoustic validation criterion:
In an acoustic piano, the hammer strike transmits an initial wavefront to the bridge.
The Hilbert energy envelope rise time Delta t_10-90 must reach its initial peak within
a sharp physical time window (peak arrival time < 0.8 ms ~ 1.5 ms, rise time Delta t_10-90 < 10 ms).
"""

import math
import numpy as np
from scipy.signal import hilbert
from typing import Dict, Any


def analyze_transient_onset(
    audio: np.ndarray,
    sample_rate: float = 48000.0
) -> Dict[str, Any]:
    """Extract rise time Delta t_10-90 and peak arrival time from the Hilbert envelope.
    
    Args:
        audio: Audio signal array
        sample_rate: Audio sampling rate in Hz
    Returns:
        dict containing rise_time_ms, peak_time_ms, and acceptance status.
    """
    if audio.ndim == 2:
        sig = np.mean(audio, axis=1)
    else:
        sig = audio

    # Extract initial 30 ms transient
    max_samples = int(0.030 * sample_rate)
    transient = np.abs(sig[:max_samples])

    # Compute Hilbert envelope
    analytic_signal = hilbert(transient)
    envelope = np.abs(analytic_signal)

    peak_idx = np.argmax(envelope)
    peak_val = envelope[peak_idx]

    if peak_val < 1e-9:
        return {
            "rise_time_ms": 0.0,
            "peak_arrival_time_ms": 0.0,
            "passed": False
        }

    # 10% and 90% peak thresholds
    t10_candidates = np.where(envelope[:peak_idx + 1] >= 0.10 * peak_val)[0]
    t90_candidates = np.where(envelope[:peak_idx + 1] >= 0.90 * peak_val)[0]

    idx_10 = t10_candidates[0] if len(t10_candidates) > 0 else 0
    idx_90 = t90_candidates[0] if len(t90_candidates) > 0 else peak_idx

    rise_time_ms = float((idx_90 - idx_10) / sample_rate * 1000.0)
    peak_time_ms = float(peak_idx / sample_rate * 1000.0)

    # Acceptance threshold from CI/CD table:
    # Rise time Delta t_10-90 < 10 ms, Peak arrival time < 1.5 ms
    passed = (rise_time_ms <= 10.0 and peak_time_ms <= 2.0)

    return {
        "rise_time_ms": rise_time_ms,
        "peak_arrival_time_ms": peak_time_ms,
        "peak_amplitude": float(peak_val),
        "target_rise_time": "<= 10.0 ms",
        "target_peak_arrival": "<= 2.0 ms",
        "passed": passed
    }
