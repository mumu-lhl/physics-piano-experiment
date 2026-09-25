"""PianoEngine: High-performance discrete physical piano simulation coordinator."""

import math
from typing import Dict, List, Tuple, Optional
import numpy as np

from physics_piano.params.schema import KeyParameters
from physics_piano.params.grand_piano import generate_grand_piano_parameters, pitch_name_to_midi
from physics_piano.core.voice import PianoVoice
from physics_piano.core.bridge import BridgeSoundboard


class PianoEngine:
    """Coordinates 88 piano voices, bridge mechanics, sympathetic resonance, and rendering."""

    def __init__(self, sample_rate: float = 48000.0, num_modes: int = 35):
        self.sample_rate = float(sample_rate)
        self.dt = 1.0 / self.sample_rate
        self.num_modes = num_modes

        # 1. Parameter bank for all 88 keys
        self.key_params: Dict[int, KeyParameters] = generate_grand_piano_parameters(num_modes=self.num_modes)

        # 2. Lazy voice map (instantiated upon first touch to conserve memory and initialization time)
        self.voices: Dict[int, PianoVoice] = {}

        # 3. Bridge and soundboard radiation model
        self.bridge = BridgeSoundboard(self.sample_rate)

        # 4. Global sustain pedal & Una Corda
        self.sustain_pedal = False
        self.pedal_depth = 1.0
        self.una_corda = False
        self.sympathetic_resonance_gain = 0.04

        # Active voice cache for fast iteration
        self.active_notes: set[int] = set()
        # Voice peak energy monitor for -96dB dynamic note lifecycle garbage collection
        self.note_energy_peak: Dict[int, float] = {}

    def _get_or_create_voice(self, midi_note: int) -> PianoVoice:
        """Retrieve existing voice or instantiate on-demand."""
        if midi_note not in self.voices:
            if midi_note not in self.key_params:
                raise ValueError(f"MIDI note {midi_note} is out of piano 88-key range [21, 108]")
            v = PianoVoice(self.key_params[midi_note], self.sample_rate)
            if self.una_corda:
                v.set_una_corda(True)
            self.voices[midi_note] = v
        return self.voices[midi_note]

    def set_radiation_mode(self, mode: str):
        """Set soundboard radiation mode ('modal' or 'upols')."""
        self.bridge.set_radiation_mode(mode)

    def note_on(self, midi_note: int, velocity: float = 0.8):
        """Depress a key with specified strike velocity."""
        v = self._get_or_create_voice(midi_note)
        v.note_on(velocity)
        self.active_notes.add(midi_note)
        # Register peak strike energy after initial contact
        self.note_energy_peak[midi_note] = max(1e-6, v.get_energy())

    def note_off(self, midi_note: int):
        """Release a key."""
        if midi_note in self.voices:
            self.voices[midi_note].note_off(self.sustain_pedal)

    def set_note_tuning(self, midi_note: int, cents: float):
        """Apply dynamic microtonal / temperament tuning offset (CLAP Note Expression)."""
        if midi_note in self.voices:
            self.voices[midi_note].set_tuning_offset(cents)

    def set_una_corda(self, enabled: bool):
        """Toggle soft pedal Una Corda across all voices."""
        self.una_corda = bool(enabled)
        for v in self.voices.values():
            v.set_una_corda(self.una_corda)

    def set_sustain_pedal(self, pedal_down: bool, depth: float = 1.0):
        """Configure sustain pedal with continuous half-pedal depth."""
        self.sustain_pedal = bool(pedal_down)
        self.pedal_depth = max(0.0, min(1.0, float(depth)))
        for v in self.voices.values():
            v.set_sustain_pedal(self.sustain_pedal, depth=self.pedal_depth)

    def pedal_down(self, depth: float = 1.0):
        """Engage sustain pedal: raise all dampers across the piano."""
        self.set_sustain_pedal(True, depth=depth)

    def pedal_up(self):
        """Release sustain pedal: drop dampers on all inactive keys."""
        self.set_sustain_pedal(False, depth=0.0)

    def render(self, duration: float) -> np.ndarray:
        """Render audio for specified duration in seconds.
        
        Returns:
            np.ndarray: Stereo float audio of shape (num_samples, 2).
        """
        num_samples = int(duration * self.sample_rate)
        out_audio = np.zeros((num_samples, 2), dtype=np.float64)

        # Dissipative bridge reaction forces from previous time step
        f_react_T = 0.0
        f_react_P = 0.0

        for i in range(num_samples):
            total_bridge_T = 0.0
            total_bridge_P = 0.0
            total_bridge_L = 0.0
            voices_to_remove = []

            num_active = len(self.active_notes)
            coupling_T = f_react_T / max(1, num_active)
            coupling_P = f_react_P / max(1, num_active)

            # 1. Step active voices exactly once with dissipative bridge coupling
            for note in list(self.active_notes):
                v = self.voices[note]
                fb_T, fb_P, fb_L = v.step(f_coupling_T=coupling_T, f_coupling_P=coupling_P)
                total_bridge_T += fb_T
                total_bridge_P += fb_P
                total_bridge_L += fb_L

                # Update peak energy
                cur_energy = v.get_energy()
                if cur_energy > self.note_energy_peak.get(note, 0.0):
                    self.note_energy_peak[note] = cur_energy

                # Voice lifecycle & energy-driven garbage collection:
                # If key released and sustain pedal off, test if modal energy dropped below -96 dB
                if not v.is_key_down and not self.sustain_pedal:
                    peak_e = self.note_energy_peak.get(note, 1e-6)
                    ratio = cur_energy / max(1e-12, peak_e)
                    if ratio < 2.5e-10 or cur_energy < 1e-12:  # -96 dB threshold
                        voices_to_remove.append(note)

            for note in voices_to_remove:
                self.active_notes.discard(note)

            # 2. 3D Bridge anisotropic coupling & soundboard driving force
            f_react_T, f_react_P, f_soundboard = self.bridge.calculate_coupling_forces(
                total_bridge_T, total_bridge_P, total_bridge_L
            )

            # 3. Soundboard acoustic radiation filtering with stereo spatial spread
            left_sample, right_sample = self.bridge.step_soundboard(f_soundboard)
            out_audio[i, 0] = left_sample
            out_audio[i, 1] = right_sample

        return out_audio
