"""Audio utilities for float32 processing, stereo normalization, and WAV file export."""

import wave
import struct
import numpy as np


def normalize_audio(audio: np.ndarray, target_peak: float = 0.95) -> np.ndarray:
    """Normalize audio array so its maximum absolute peak matches target_peak."""
    max_val = np.max(np.abs(audio))
    if max_val > 1e-7:
        return audio * (target_peak / max_val)
    return audio


def write_wav(filename: str, audio: np.ndarray, sample_rate: int = 48000):
    """Write 1D or 2D NumPy array to a standard 16-bit PCM WAV file.
    
    Args:
        filename: Target path
        audio: 1D array (mono) or 2D array of shape (samples, 2) (stereo)
        sample_rate: Sampling rate in Hz (default: 48000)
    """
    if audio.ndim == 1:
        # Mono
        channels = 1
        samples = len(audio)
        audio_int16 = np.int16(np.clip(audio, -1.0, 1.0) * 32767.0)
        raw_bytes = audio_int16.tobytes()
    elif audio.ndim == 2:
        # Stereo
        samples, channels = audio.shape
        audio_int16 = np.int16(np.clip(audio, -1.0, 1.0) * 32767.0)
        # Interleave channels
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
