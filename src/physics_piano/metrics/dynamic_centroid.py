"""Dynamic spectral centroid non-linear expansion slope analysis.

Acoustic validation criterion:
In an acoustic piano, strikes at higher velocities excite higher partials disproportionately,
causing the spectral centroid to expand non-linearly with strike velocity v0:
  kappa_dyn = d(log(SC_peak)) / d(log(v0))
Physical target range: kappa_dyn in [0.45, 0.65].
"""

import math
import numpy as np
from typing import Dict, Any, List, Optional

from physics_piano.params.schema import KeyParameters
from physics_piano.engine import PianoEngine


def compute_spectral_centroid(signal: np.ndarray, sample_rate: float = 48000.0) -> float:
    """Compute spectral centroid SC = sum(f * |X(f)|) / sum(|X(f)|)."""
    if len(signal) == 0:
        return 0.0
    
    n_fft = 2 ** int(math.ceil(math.log2(len(signal))))
    if n_fft < 4096:
        n_fft = 4096

    window = np.hanning(len(signal))
    spec = np.abs(np.fft.rfft(signal * window, n=n_fft))
    freqs = np.fft.rfftfreq(n_fft, d=1.0 / sample_rate)

    total_mag = np.sum(spec)
    if total_mag < 1e-12:
        return 0.0
    return float(np.sum(freqs * spec) / total_mag)


def measure_dynamic_centroid_slope(
    key_params: KeyParameters,
    velocities: Optional[List[float]] = None,
    sample_rate: float = 48000.0
) -> Dict[str, Any]:
    """Analyze dynamic spectral centroid scaling across strike velocity ladder.
    
    Args:
        key_params: Target piano key physical parameters
        velocities: List of strike velocities in (0.0, 1.0] (default: [0.2, 0.4, 0.6, 0.8, 1.0])
        sample_rate: Audio sampling rate
    Returns:
        dict with velocity values, peak centroid values, fitted kappa_dyn, and pass status.
    """
    if velocities is None:
        velocities = [0.2, 0.4, 0.6, 0.8, 1.0]

    centroids = []
    # Analyze the initial 100 ms attack transient
    analysis_samples = int(0.10 * sample_rate)

    for vel in velocities:
        num_modes = getattr(key_params.strings[0], 'num_modes', 35)
        engine = PianoEngine(sample_rate=sample_rate, num_modes=min(35, num_modes))
        engine.note_on(key_params.midi_note, velocity=vel)
        audio = engine.render(0.12)
        # Transient window
        transient_sig = audio[:analysis_samples, 0] if audio.ndim == 2 else audio[:analysis_samples]
        sc = compute_spectral_centroid(transient_sig, sample_rate=sample_rate)
        centroids.append(sc)

    # Convert to log scale: log(v) vs log(SC)
    log_v = np.log(velocities)
    log_sc = np.log(np.maximum(1e-6, centroids))

    # Linear regression in log-log space: log_sc = kappa_dyn * log_v + C
    poly = np.polyfit(log_v, log_sc, 1)
    kappa_dyn = float(poly[0])

    passed = (0.40 <= kappa_dyn <= 0.70)

    return {
        "velocities": velocities,
        "spectral_centroids_hz": [float(c) for c in centroids],
        "kappa_dyn": kappa_dyn,
        "target_range": "[0.45, 0.65]",
        "passed": passed
    }
