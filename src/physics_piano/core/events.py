"""CLAP-compatible sample-accurate event architecture and note expression definitions.

Implements sample-accurate event queuing, microtonal tuning, continuous parameters,
and voice lifecycle management as specified in the CLAP plugin standard.
"""

from enum import Enum, auto
from dataclasses import dataclass
from typing import List, Optional, Union, Dict, Any


class ClapEventType(Enum):
    NOTE_ON = auto()
    NOTE_OFF = auto()
    NOTE_END = auto()
    NOTE_EXPRESSION = auto()
    PARAM_VALUE = auto()


class ClapNoteExpressionType(Enum):
    TUNING = auto()       # Dynamic detuning in semitones / cents (tension modulation)
    VOLUME = auto()       # Gain in dB or normalized amplitude
    PAN = auto()          # Stereo pan position [-1.0, 1.0]
    VIBRATO = auto()      # Dynamic vibrato depth
    EXPRESSION = auto()   # Polyphonic expression
    PRESSURE = auto()     # Polyphonic aftertouch / continuous key pressure


class ClapParamId(Enum):
    SUSTAIN_PEDAL = auto()  # Sustain damper pedal [0.0 = up, 1.0 = fully down, continuous half-pedal]
    UNA_CORDA = auto()      # Soft pedal [0.0 = off, 1.0 = on]
    RADIATION_MODE = auto() # 0 = modal IIR, 1 = UPOLS FFT


@dataclass
class ClapEvent:
    time: int                   # Sample offset within current audio block [0, block_size)
    event_type: ClapEventType
    key: int = -1               # MIDI note number [21, 108] or -1 for global
    channel: int = 0
    note_id: int = -1           # Unique note ID for sample-accurate polyphonic expression


@dataclass
class ClapNoteOnEvent(ClapEvent):
    velocity: float = 0.8       # Normalized velocity in (0.0, 1.0]

    def __init__(self, time: int, key: int, velocity: float = 0.8, note_id: int = -1, channel: int = 0):
        super().__init__(time=time, event_type=ClapEventType.NOTE_ON, key=key, channel=channel, note_id=note_id)
        self.velocity = float(velocity)


@dataclass
class ClapNoteOffEvent(ClapEvent):
    velocity: float = 0.0

    def __init__(self, time: int, key: int, velocity: float = 0.0, note_id: int = -1, channel: int = 0):
        super().__init__(time=time, event_type=ClapEventType.NOTE_OFF, key=key, channel=channel, note_id=note_id)
        self.velocity = float(velocity)


@dataclass
class ClapNoteEndEvent(ClapEvent):
    """Emitted by synth when a voice's mechanical energy has completely decayed (-96 dB)."""
    def __init__(self, time: int, key: int, note_id: int = -1, channel: int = 0):
        super().__init__(time=time, event_type=ClapEventType.NOTE_END, key=key, channel=channel, note_id=note_id)


@dataclass
class ClapNoteExpressionEvent(ClapEvent):
    expression_type: ClapNoteExpressionType = ClapNoteExpressionType.TUNING
    value: float = 0.0          # Value depends on expression_type (cents for TUNING, ratio for VOLUME)

    def __init__(
        self,
        time: int,
        key: int,
        expression_type: ClapNoteExpressionType,
        value: float,
        note_id: int = -1,
        channel: int = 0
    ):
        super().__init__(time=time, event_type=ClapEventType.NOTE_EXPRESSION, key=key, channel=channel, note_id=note_id)
        self.expression_type = expression_type
        self.value = float(value)


@dataclass
class ClapParamValueEvent(ClapEvent):
    param_id: ClapParamId = ClapParamId.SUSTAIN_PEDAL
    value: float = 0.0

    def __init__(self, time: int, param_id: ClapParamId, value: float):
        super().__init__(time=time, event_type=ClapEventType.PARAM_VALUE, key=-1, channel=0, note_id=-1)
        self.param_id = param_id
        self.value = float(value)


class ClapEventQueue:
    """Sample-accurate priority queue for CLAP events within an audio processing block."""

    def __init__(self):
        self._events: List[ClapEvent] = []

    def push(self, event: ClapEvent):
        """Insert event into the queue maintaining timestamp ordering."""
        self._events.append(event)
        self._events.sort(key=lambda e: e.time)

    def clear(self):
        self._events.clear()

    def get_events_for_sample(self, sample_idx: int) -> List[ClapEvent]:
        """Retrieve and remove events scheduled at or before the given sample index."""
        ready = []
        remaining = []
        for e in self._events:
            if e.time <= sample_idx:
                ready.append(e)
            else:
                remaining.append(e)
        self._events = remaining
        return ready

    def peek_next_time(self) -> Optional[int]:
        """Return the timestamp of the earliest pending event, or None if empty."""
        return self._events[0].time if self._events else None

    def __len__(self):
        return len(self._events)
