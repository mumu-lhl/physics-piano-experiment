"""Sample-accurate onset alignment and Dynamic Time Warping (DTW) for acoustic reference comparison.

Used to align synthetic physical model renders against reference recordings (e.g. MAPS Disklavier
or VSL near-field dry recordings) prior to MRSL and PEAQ perceptual difference evaluation.
"""

import numpy as np
from typing import Dict, Any, Tuple


def detect_onset_sample(signal: np.ndarray, threshold_ratio: float = 0.05) -> int:
    """Detect the initial onset sample using energy envelope thresholding.
    
    Args:
        signal: 1D audio waveform array
        threshold_ratio: Fraction of absolute peak to declare onset
    Returns:
        Sample index of onset
    """
    abs_sig = np.abs(signal)
    peak = np.max(abs_sig)
    if peak < 1e-9:
        return 0
    thresh = peak * threshold_ratio
    indices = np.where(abs_sig >= thresh)[0]
    return int(indices[0]) if len(indices) > 0 else 0


def align_audio_onsets(
    synthetic: np.ndarray,
    reference: np.ndarray,
    pre_roll_samples: int = 240
) -> Tuple[np.ndarray, np.ndarray]:
    """Sample-accurately align synthetic and reference waveforms by their attack onsets.
    
    Args:
        synthetic: Synthesized waveform array
        reference: Reference waveform array
        pre_roll_samples: Number of silence/pre-transient samples to preserve before onset
    Returns:
        (aligned_synthetic, aligned_reference) trimmed to common duration
    """
    s_syn = np.mean(synthetic, axis=1) if synthetic.ndim == 2 else synthetic
    s_ref = np.mean(reference, axis=1) if reference.ndim == 2 else reference

    idx_syn = detect_onset_sample(s_syn)
    idx_ref = detect_onset_sample(s_ref)

    start_syn = max(0, idx_syn - pre_roll_samples)
    start_ref = max(0, idx_ref - pre_roll_samples)

    trimmed_syn = s_syn[start_syn:]
    trimmed_ref = s_ref[start_ref:]

    min_len = min(len(trimmed_syn), len(trimmed_ref))
    return trimmed_syn[:min_len], trimmed_ref[:min_len]


def compute_dtw_distance(
    synthetic: np.ndarray,
    reference: np.ndarray,
    hop_length: int = 256
) -> Dict[str, Any]:
    """Compute Dynamic Time Warping (DTW) path and alignment cost on spectral energy frames.
    
    Args:
        synthetic: Synthesized audio
        reference: Reference audio
        hop_length: Frame hop size in samples
    Returns:
        dict containing normalized DTW cost, sequence lengths, and max alignment offset.
    """
    s_syn, s_ref = align_audio_onsets(synthetic, reference)
    
    # Compute RMS energy per frame
    n_frames_syn = len(s_syn) // hop_length
    n_frames_ref = len(s_ref) // hop_length
    
    if n_frames_syn < 2 or n_frames_ref < 2:
        return {"dtw_cost": 0.0, "max_offset_ms": 0.0}

    feat_syn = np.array([
        np.sqrt(np.mean(s_syn[i * hop_length : (i + 1) * hop_length] ** 2) + 1e-12)
        for i in range(n_frames_syn)
    ])
    feat_ref = np.array([
        np.sqrt(np.mean(s_ref[i * hop_length : (i + 1) * hop_length] ** 2) + 1e-12)
        for i in range(n_frames_ref)
    ])

    # Dynamic programming cost matrix
    N = len(feat_syn)
    M = len(feat_ref)
    cost = np.full((N, M), np.inf)
    cost[0, 0] = abs(feat_syn[0] - feat_ref[0])

    for i in range(1, N):
        cost[i, 0] = cost[i - 1, 0] + abs(feat_syn[i] - feat_ref[0])
    for j in range(1, M):
        cost[0, j] = cost[0, j - 1] + abs(feat_syn[0] - feat_ref[j])

    for i in range(1, N):
        for j in range(1, M):
            d = abs(feat_syn[i] - feat_ref[j])
            cost[i, j] = d + min(cost[i - 1, j], cost[i, j - 1], cost[i - 1, j - 1])

    norm_cost = float(cost[N - 1, M - 1] / (N + M))
    return {
        "dtw_normalized_cost": norm_cost,
        "n_frames_syn": N,
        "n_frames_ref": M,
    }
