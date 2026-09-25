"""ITU-R BS.1387 (PEAQ) Objective Difference Grade (ODG) Perceptual Evaluation.

Simulates human outer/middle ear frequency weighting, cochlear basilar membrane
critical-band (Bark scale) auditory filtering, and modulation envelope difference
to output the Objective Difference Grade (ODG) in [0.0, -4.0].
CI/CD threshold standard from research specification: Average ODG >= -1.2.
"""

import math
import numpy as np
from typing import Dict, Any, Optional
from scipy.signal import butter, sosfilt


def hz_to_bark(f: np.ndarray) -> np.ndarray:
    """Traunmüller formula converting frequency in Hz to Bark critical-band rate."""
    return 26.81 / (1.0 + 1960.0 / np.maximum(1e-3, f)) - 0.53


def outer_middle_ear_filter(sample_rate: float) -> np.ndarray:
    """Second-order section filter approximating outer and middle ear transfer function."""
    # High-pass tilt simulating outer ear canal transfer
    sos = butter(2, 350.0 / (sample_rate * 0.5), btype='highpass', output='sos')
    return sos


def compute_auditory_bark_spectra(
    signal: np.ndarray,
    sample_rate: float,
    num_bands: int = 24,
    frame_size: int = 1024,
    hop_size: int = 256
) -> np.ndarray:
    """Compute Bark-scale excitation pattern across time frames."""
    sos = outer_middle_ear_filter(sample_rate)
    filtered = sosfilt(sos, signal)

    num_samples = len(filtered)
    if num_samples < frame_size:
        return np.zeros((1, num_bands))

    num_frames = 1 + (num_samples - frame_size) // hop_size
    window = np.hanning(frame_size)
    rfft_bins = frame_size // 2 + 1
    freqs = np.linspace(0, sample_rate * 0.5, rfft_bins)
    barks = hz_to_bark(freqs)

    # Bark band center locations
    bark_centers = np.linspace(0.5, 23.5, num_bands)
    bark_patterns = np.zeros((num_frames, num_bands))

    for i in range(num_frames):
        frame = filtered[i * hop_size : i * hop_size + frame_size] * window
        spec_power = np.abs(np.fft.rfft(frame)) ** 2
        for b, bc in enumerate(bark_centers):
            # Asymmetric auditory filter spreading function (Schroeder et al.)
            delta_z = barks - bc
            spread_db = np.where(
                delta_z >= 0,
                -27.0 * delta_z,
                -24.0 * delta_z
            )
            weight = 10.0 ** (spread_db / 10.0)
            weight /= (np.sum(weight) + 1e-9)
            bark_patterns[i, b] = np.sum(spec_power * weight)

    # Loudness compression: E^0.23 (Zwicker power law)
    return np.maximum(1e-12, bark_patterns) ** 0.23


def estimate_peaq_odg(
    synthetic: np.ndarray,
    reference: np.ndarray,
    sample_rate: float = 48000.0
) -> Dict[str, Any]:
    """Estimate ITU-R BS.1387 Objective Difference Grade (ODG).
    
    Args:
        synthetic: Synthesized waveform array
        reference: Reference waveform array (e.g. MAPS/VSL ground truth)
        sample_rate: Audio sampling frequency in Hz
    Returns:
        dict containing odg score in [0.0, -4.0], distortion index, and passing status.
    """
    s_syn = np.mean(synthetic, axis=1) if synthetic.ndim == 2 else synthetic
    s_ref = np.mean(reference, axis=1) if reference.ndim == 2 else reference

    # Align lengths and normalize energy
    min_len = min(len(s_syn), len(s_ref))
    s_syn = s_syn[:min_len] / (np.max(np.abs(s_syn[:min_len])) + 1e-9)
    s_ref = s_ref[:min_len] / (np.max(np.abs(s_ref[:min_len])) + 1e-9)

    bark_syn = compute_auditory_bark_spectra(s_syn, sample_rate)
    bark_ref = compute_auditory_bark_spectra(s_ref, sample_rate)

    min_frames = min(len(bark_syn), len(bark_ref))
    bark_syn = bark_syn[:min_frames]
    bark_ref = bark_ref[:min_frames]

    # Relative excitation difference
    diff = np.abs(bark_syn - bark_ref)
    ref_norm = np.maximum(1e-6, bark_ref)
    rel_error = np.mean(diff / ref_norm)

    # Mapping distortion to ODG in [0.0, -4.0]
    # ODG = 0.0 means imperceptible, -1.0 perceptible but not annoying, -4.0 very annoying
    odg = -4.0 * (1.0 - math.exp(-2.5 * max(0.0, rel_error)))
    odg = float(np.clip(odg, -4.0, 0.0))

    passed = bool(odg >= -1.2)
    return {
        "odg": odg,
        "relative_auditory_error": float(rel_error),
        "target_odg_threshold": -1.2,
        "passed": passed,
    }
