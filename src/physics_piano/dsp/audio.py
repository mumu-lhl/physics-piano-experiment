"""Audio utilities for float32 processing, stereo normalization, and WAV file export."""

import wave
import struct
import numpy as np


def normalize_audio(audio: np.ndarray, target_peak: float = 0.95) -> np.ndarray:
    """Normalize audio array so its maximum absolute peak matches target_peak.
    
    Includes safety guards against empty arrays, NaNs, and Infs.
    """
    if audio is None or len(audio) == 0:
        return audio

    # Sanitize NaNs and Infs
    clean_audio = np.nan_to_num(audio, nan=0.0, posinf=target_peak, neginf=-target_peak)
    max_val = np.max(np.abs(clean_audio))
    if max_val > 1e-7:
        return clean_audio * (target_peak / max_val)
    return clean_audio


def write_wav(filename: str, audio: np.ndarray, sample_rate: int = 48000):
    """Write 1D or 2D NumPy array to a standard 16-bit PCM WAV file.
    
    Args:
        filename: Target path
        audio: 1D array (mono) or 2D array of shape (samples, 2) (stereo)
        sample_rate: Sampling rate in Hz (default: 48000)
    """
    if audio is None or len(audio) == 0:
        audio = np.zeros((1, 2) if (audio is not None and audio.ndim == 2) else (1,), dtype=np.float32)

    # Sanitize and clip
    clean_audio = np.nan_to_num(audio, nan=0.0, posinf=1.0, neginf=-1.0)

    if clean_audio.ndim == 1:
        channels = 1
        samples = len(clean_audio)
        audio_int16 = np.int16(np.clip(clean_audio, -1.0, 1.0) * 32767.0)
        raw_bytes = audio_int16.tobytes()
    elif clean_audio.ndim == 2:
        samples, channels = clean_audio.shape
        audio_int16 = np.int16(np.clip(clean_audio, -1.0, 1.0) * 32767.0)
        interleaved = np.empty((samples * 2,), dtype=np.int16)
        interleaved[0::2] = audio_int16[:, 0]
        interleaved[1::2] = audio_int16[:, 1]
        raw_bytes = interleaved.tobytes()
    else:
        raise ValueError(f"Unsupported audio shape: {audio.shape}")

    with wave.open(filename, "wb") as wf:
        wf.setnchannels(channels)
        wf.setsampwidth(2)  # 16-bit
        wf.setframerate(sample_rate)
        wf.writeframes(raw_bytes)
