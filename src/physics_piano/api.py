"""High-level Python API for physics-based piano synthesis and inspection."""

from typing import Union, List, Optional
import numpy as np

from physics_piano.params.grand_piano import pitch_name_to_midi, midi_to_pitch_name
from physics_piano.engine import PianoEngine
from physics_piano.dsp.audio import normalize_audio, write_wav


class PianoSynth:
    """Convenient high-level API for physical piano sound rendering and experimentation."""

    def __init__(self, sample_rate: int = 48000, num_modes: int = 35):
        self.sample_rate = sample_rate
        self.num_modes = num_modes
        self.engine = PianoEngine(sample_rate=self.sample_rate, num_modes=self.num_modes)

    def _resolve_midi(self, pitch: Union[str, int]) -> int:
        if isinstance(pitch, str):
            return pitch_name_to_midi(pitch)
        return int(pitch)

    def render_note(
        self,
        pitch: Union[str, int],
        velocity: float = 0.8,
        duration: float = 3.0,
        sustain: bool = False,
        normalize: bool = True
    ) -> np.ndarray:
        """Render a single piano note from physical principles."""
        midi = self._resolve_midi(pitch)
        engine = PianoEngine(sample_rate=self.sample_rate, num_modes=self.num_modes)
        if sustain:
            engine.pedal_down()
        engine.note_on(midi, velocity=velocity)

        # Allow key to stay down for most of duration, release near the end if not sustain
        if not sustain and duration > 1.2:
            chunk1 = engine.render(duration - 0.5)
            engine.note_off(midi)
            chunk2 = engine.render(0.5)
            audio = np.concatenate([chunk1, chunk2], axis=0)
        else:
            audio = engine.render(duration)

        if normalize:
            audio = normalize_audio(audio)
        return audio

    def render_chord(
        self,
        pitches: List[Union[str, int]],
        velocity: float = 0.8,
        duration: float = 3.5,
        sustain: bool = True,
        normalize: bool = True
    ) -> np.ndarray:
        """Render a polyphonic chord with sympathetic resonance."""
        engine = PianoEngine(sample_rate=self.sample_rate, num_modes=self.num_modes)
        if sustain:
            engine.pedal_down()

        for p in pitches:
            midi = self._resolve_midi(p)
            engine.note_on(midi, velocity=velocity)

        audio = engine.render(duration)
        if normalize:
            audio = normalize_audio(audio)
        return audio

    def export_wav(self, filename: str, audio: np.ndarray):
        """Save synthesized audio buffer to a 16-bit PCM WAV file."""
        write_wav(filename, audio, sample_rate=self.sample_rate)
