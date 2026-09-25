"""Multi-Resolution STFT Loss (MRSL) for objective spectral-temporal fidelity evaluation.

Computes multi-scale spectral convergence and log-magnitude STFT loss across multiple analysis windows:
  M in {512, 1024, 2048, 4096} with hop R = M // 4.
Used in modern neuro-acoustic and physical modeling validation pipelines to compare synthesized
audio against acoustic reference targets (e.g. MAPS / VSL recordings).
"""

import math
import numpy as np
from typing import Dict, Any, List, Tuple, Optional


def compute_stft_magnitude(signal: np.ndarray, fft_size: int, hop_size: int) -> np.ndarray:
    """Compute STFT magnitude spectrogram using Hann window."""
    window = np.hanning(fft_size)
    num_samples = len(signal)
    if num_samples < fft_size:
        padded = np.pad(signal, (0, fft_size - num_samples))
        num_frames = 1
        frames = [padded]
    else:
        num_frames = 1 + (num_samples - fft_size) // hop_size
        frames = [signal[i * hop_size : i * hop_size + fft_size] for i in range(num_frames)]

    frames_matrix = np.array(frames) * window
    spec = np.abs(np.fft.rfft(frames_matrix, n=fft_size, axis=-1))
    return spec


def compute_mrsl(
    synthetic: np.ndarray,
    reference: np.ndarray,
    fft_sizes: Optional[List[int]] = None
) -> Dict[str, Any]:
    """Compute Multi-Resolution STFT Loss between synthetic render and reference.
    
    Args:
        synthetic: Synthesized audio array (1D or 2D)
        reference: Target reference audio array (1D or 2D)
        fft_sizes: List of FFT analysis window sizes (default: [512, 1024, 2048, 4096])
    Returns:
        dict containing total_loss, per_scale_losses, spectral_convergence, and log_magnitude_loss.
    """
    if fft_sizes is None:
        fft_sizes = [512, 1024, 2048, 4096]

    # Convert to mono
    s_syn = np.mean(synthetic, axis=1) if synthetic.ndim == 2 else synthetic
    s_ref = np.mean(reference, axis=1) if reference.ndim == 2 else reference

    # Align lengths
    min_len = min(len(s_syn), len(s_ref))
    s_syn = s_syn[:min_len]
    s_ref = s_ref[:min_len]

    # Energy normalization
    norm_syn = s_syn / (np.max(np.abs(s_syn)) + 1e-9)
    norm_ref = s_ref / (np.max(np.abs(s_ref)) + 1e-9)

    total_sc = 0.0
    total_mag = 0.0
    scale_details = {}

    for M in fft_sizes:
        hop = M // 4
        mag_syn = compute_stft_magnitude(norm_syn, fft_size=M, hop_size=hop)
        mag_ref = compute_stft_magnitude(norm_ref, fft_size=M, hop_size=hop)

        # Spectral convergence loss: || |S| - |S_hat| ||_F / || |S_ref| ||_F
        diff = mag_syn - mag_ref
        frob_diff = np.linalg.norm(diff, 'fro')
        frob_ref = np.linalg.norm(mag_ref, 'fro') + 1e-9
        sc_loss = float(frob_diff / frob_ref)

        # Log-magnitude L1 loss: (1 / N_frames) * || log(|S_hat|) - log(|S_ref|) ||_1
        log_syn = np.log(np.maximum(1e-6, mag_syn))
        log_ref = np.log(np.maximum(1e-6, mag_ref))
        mag_loss = float(np.mean(np.abs(log_syn - log_ref)))

        total_sc += sc_loss
        total_mag += mag_loss
        scale_details[f"scale_{M}"] = {
            "spectral_convergence": sc_loss,
            "log_magnitude": mag_loss,
            "combined": sc_loss + mag_loss
        }

    k = len(fft_sizes)
    avg_sc = total_sc / k
    avg_mag = total_mag / k
    total_loss = (total_sc + total_mag) / k

    return {
        "total_mrsl_loss": float(total_loss),
        "avg_spectral_convergence": float(avg_sc),
        "avg_log_magnitude": float(avg_mag),
        "scale_details": scale_details
    }
