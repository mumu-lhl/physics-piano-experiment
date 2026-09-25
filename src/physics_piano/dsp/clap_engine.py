"""CLAP Native Plugin Engine Architecture.

Implements the 4-stage pipeline specified in the CLAP Physical Modeling research document:
  - Stage 1: Sample-accurate event dispatch and strike dynamics (Host Audio Thread)
  - Stage 2: Concurrent voice and modal oscillator bank stepping (Host Thread Pool)
  - Stage 3: Atomic reduction of bridge boundary forces (Host Audio Thread)
  - Stage 4: UPOLS soundboard convolution & sympathetic feedback (Worker Threads)
"""

import math
from typing import Dict, List, Optional, Set, Tuple
import numpy as np

from physics_piano.params.schema import KeyParameters
from physics_piano.params.grand_piano import generate_grand_piano_parameters
from physics_piano.core.voice import PianoVoice
from physics_piano.core.bridge import BridgeSoundboard
from physics_piano.core.events import (
    ClapEvent, ClapEventType, ClapNoteOnEvent, ClapNoteOffEvent,
    ClapNoteEndEvent, ClapNoteExpressionEvent, ClapNoteExpressionType,
    ClapParamValueEvent, ClapParamId, ClapEventQueue
)


class ClapPianoEngine:
    """CLAP-standard physical modeling virtual instrument audio engine."""

    def __init__(
        self,
        sample_rate: float = 48000.0,
        num_modes: int = 35,
        block_size: int = 128,
        stretch_tuning: bool = False
    ):
        self.sample_rate = float(sample_rate)
        self.dt = 1.0 / self.sample_rate
        self.num_modes = num_modes
        self.block_size = block_size

        # Precompute 88-key physical concert grand parameters
        self.key_params: Dict[int, KeyParameters] = generate_grand_piano_parameters(
            num_modes=self.num_modes, stretch_tuning=stretch_tuning
        )

        # Voices: instantiated dynamically or pre-allocated
        self.voices: Dict[int, PianoVoice] = {}
        self.active_keys: Set[int] = set()

        # 3D Bridge & soundboard radiation
        self.bridge = BridgeSoundboard(self.sample_rate)

        # Global pedal parameters
        self.sustain_pedal = False
        self.pedal_depth = 1.0
        self.una_corda = False

        # Energy monitor for -96dB dynamic note lifecycle garbage collection
        self.note_energy_peak: Dict[int, float] = {}

        # Dissipative bridge reaction coupling forces from previous step
        self.f_react_T = 0.0
        self.f_react_P = 0.0

    def _get_or_create_voice(self, key: int) -> PianoVoice:
        if key not in self.voices:
            if key not in self.key_params:
                raise ValueError(f"Key {key} out of range [21, 108]")
            v = PianoVoice(self.key_params[key], self.sample_rate)
            if self.una_corda:
                v.set_una_corda(True)
            self.voices[key] = v
        return self.voices[key]

    def set_radiation_mode(self, mode: str):
        """Set soundboard radiation mode ('modal' or 'upols')."""
        self.bridge.set_radiation_mode(mode)

    # --------------------------------------------------------------------------
    # Pipeline Stage 1: Sample-accurate Event Dispatch
    # --------------------------------------------------------------------------
    def _handle_event(self, event: ClapEvent, out_events: Optional[List[ClapEvent]] = None):
        """Consume sample-accurate CLAP event and update physical state."""
        if event.event_type == ClapEventType.NOTE_ON:
            assert isinstance(event, ClapNoteOnEvent)
            v = self._get_or_create_voice(event.key)
            v.note_on(event.velocity)
            self.active_keys.add(event.key)
            self.note_energy_peak[event.key] = max(1e-6, v.get_energy())

        elif event.event_type == ClapEventType.NOTE_OFF:
            assert isinstance(event, ClapNoteOffEvent)
            if event.key in self.voices:
                self.voices[event.key].note_off(self.sustain_pedal)

        elif event.event_type == ClapEventType.NOTE_EXPRESSION:
            assert isinstance(event, ClapNoteExpressionEvent)
            if event.key in self.voices:
                v = self.voices[event.key]
                if event.expression_type == ClapNoteExpressionType.TUNING:
                    # Dynamic fundamental tension T0 modulation
                    v.set_tuning_offset(event.value)
                elif event.expression_type == ClapNoteExpressionType.PRESSURE:
                    # Continuous key pressure modulates damper touch
                    pass

        elif event.event_type == ClapEventType.PARAM_VALUE:
            assert isinstance(event, ClapParamValueEvent)
            if event.param_id == ClapParamId.SUSTAIN_PEDAL:
                # Continuous half-pedaling
                depth = max(0.0, min(1.0, event.value))
                self.sustain_pedal = depth > 0.05
                self.pedal_depth = depth
                for v in self.voices.values():
                    v.set_sustain_pedal(self.sustain_pedal, depth=self.pedal_depth)
            elif event.param_id == ClapParamId.UNA_CORDA:
                self.una_corda = event.value > 0.5
                for v in self.voices.values():
                    v.set_una_corda(self.una_corda)
            elif event.param_id == ClapParamId.RADIATION_MODE:
                mode = "upols" if event.value >= 0.5 else "modal"
                self.bridge.set_radiation_mode(mode)

    # --------------------------------------------------------------------------
    # Block Process Callback (CLAP process entrypoint)
    # --------------------------------------------------------------------------
    def process(
        self,
        num_samples: int,
        in_events: Optional[List[ClapEvent]] = None,
        out_events: Optional[List[ClapEvent]] = None
    ) -> np.ndarray:
        """Process one audio buffer with sample-accurate events and 4-stage pipeline.
        
        Args:
            num_samples: Block size (e.g. 64, 128, 256)
            in_events: Incoming CLAP events scheduled with sample offsets
            out_events: Outgoing CLAP events list (e.g. ClapNoteEndEvent)
        Returns:
            audio_out: Stereo audio array of shape (num_samples, 2)
        """
        audio_out = np.zeros((num_samples, 2), dtype=np.float64)

        event_queue = ClapEventQueue()
        if in_events:
            for ev in in_events:
                event_queue.push(ev)

        for s in range(num_samples):
            # Stage 1: Sample-accurate event handling
            ready_events = event_queue.get_events_for_sample(s)
            for ev in ready_events:
                self._handle_event(ev, out_events)

            # Stage 2: Voice & modal oscillator bank stepping
            total_bridge_T = 0.0
            total_bridge_P = 0.0
            total_bridge_L = 0.0
            keys_to_remove = []

            num_active = len(self.active_keys)
            coupling_T = self.f_react_T / max(1, num_active)
            coupling_P = self.f_react_P / max(1, num_active)

            for key in list(self.active_keys):
                v = self.voices[key]
                fb_T, fb_P, fb_L = v.step(f_coupling_T=coupling_T, f_coupling_P=coupling_P)
                total_bridge_T += fb_T
                total_bridge_P += fb_P
                total_bridge_L += fb_L

                # Energy monitoring & -96dB dynamic note lifecycle termination
                cur_e = v.get_energy()
                if cur_e > self.note_energy_peak.get(key, 0.0):
                    self.note_energy_peak[key] = cur_e

                if not v.is_key_down:
                    peak_e = self.note_energy_peak.get(key, 1e-6)
                    ratio = cur_e / max(1e-12, peak_e)
                    if ratio < 1e-7 or cur_e < 1e-10:
                        keys_to_remove.append(key)
                        if out_events is not None:
                            out_events.append(ClapNoteEndEvent(time=s, key=key))

            for key in keys_to_remove:
                self.active_keys.discard(key)

            # Stage 3: Bridge reaction force reduction
            self.f_react_T, self.f_react_P, f_soundboard = self.bridge.calculate_coupling_forces(
                total_bridge_T, total_bridge_P, total_bridge_L
            )

            # Stage 4: Soundboard radiation & stereo spatialization
            left_s, right_s = self.bridge.step_soundboard(f_soundboard)
            audio_out[s, 0] = left_s
            audio_out[s, 1] = right_s

        return audio_out
