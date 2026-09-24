"""PianoVoice: Unison string group, shared hammer, and individual damper control."""

import math
from typing import Tuple, List
from physics_piano.params.schema import KeyParameters, StringPhysicalParameters
from physics_piano.core.string import StiffStringModal
from physics_piano.core.hammer import HuntCrossleyHammer


class PianoVoice:
    """A single piano key containing unison strings (1 to 3), hammer, and damper."""

    def __init__(self, key_params: KeyParameters, sample_rate: float = 48000.0):
        self.key_params = key_params
        self.sample_rate = float(sample_rate)
        self.dt = 1.0 / self.sample_rate

        self.midi_note = key_params.midi_note
        self.pitch_name = key_params.pitch_name
        self.target_f0 = key_params.target_f0

        # Stereo pan position (0.1=bass/left, 0.9=treble/right)
        self.pan = (self.midi_note - 21) / (108 - 21) * 0.8 + 0.1

        # Instantiate unison strings with micro-detuning
        self.strings: List[StiffStringModal] = []
        for i, s_param in enumerate(key_params.strings):
            cents_detune = key_params.detuning_cents[i] if i < len(key_params.detuning_cents) else 0.0
            # Tension adjustment for micro-detuning: f ~ sqrt(T), so delta_T ~ 2 * delta_f
            freq_ratio = 2.0 ** (cents_detune / 1200.0)
            detuned_tension = s_param.tension * (freq_ratio ** 2)

            detuned_param = StringPhysicalParameters(
                length=s_param.length,
                radius=s_param.radius,
                density=s_param.density,
                youngs_modulus=s_param.youngs_modulus,
                tension=detuned_tension,
                sigma0=s_param.sigma0,
                sigma1=s_param.sigma1,
                strike_ratio=s_param.strike_ratio,
                num_modes=s_param.num_modes,
                polarization_mistuning=s_param.polarization_mistuning
            )
            self.strings.append(StiffStringModal(detuned_param, self.sample_rate))

        # Shared hammer
        self.hammer = HuntCrossleyHammer(key_params.hammer, self.sample_rate)

        self.is_key_down = False
        self.is_sounding = False

    def note_on(self, velocity: float):
        """Depress key and strike unison strings."""
        self.is_key_down = True
        self.is_sounding = True
        # Raise dampers on this voice
        for s in self.strings:
            s.set_damper(False)
        
        # Calculate current average displacement of strings under felt
        u_cur = sum(s.get_strike_displacement_and_velocity()[0] for s in self.strings) / len(self.strings)
        # Strike hammer with current string position context for smooth restrike
        self.hammer.strike(velocity, initial_u_string=u_cur)

    def note_off(self, sustain_pedal: bool = False):
        """Release key. If sustain pedal is not held, lower dampers."""
        self.is_key_down = False
        if not sustain_pedal:
            for s in self.strings:
                s.set_damper(True, depth=1.0)

    def set_sustain_pedal(self, pedal_down: bool):
        """Update damper state according to global sustain pedal."""
        if pedal_down:
            # Pedal up-lifts all dampers
            for s in self.strings:
                s.set_damper(False)
        else:
            # If pedal released and key is not held down, lower dampers
            if not self.is_key_down:
                for s in self.strings:
                    s.set_damper(True, depth=1.0)

    def step(self, f_coupling_T: float = 0.0, f_coupling_P: float = 0.0) -> Tuple[float, float]:
        """Compute one step of hammer interaction and unison string states.
        
        Returns:
            f_bridge_T_sum, f_bridge_P_sum: Forces exerted by this key's unison strings on bridge.
        """
        # 1. Average string displacement and velocity under hammer felt
        u_avg = 0.0
        v_avg = 0.0
        num_str = len(self.strings)
        for s in self.strings:
            u_i, v_i = s.get_strike_displacement_and_velocity()
            u_avg += u_i
            v_avg += v_i
        u_avg /= num_str
        v_avg /= num_str

        # 2. Compute nonlinear hammer force
        f_hammer = self.hammer.compute_force(u_avg, v_avg)
        self.hammer.advance(f_hammer)

        # Split hammer force equally among unison strings
        f_hammer_per_string = f_hammer / num_str

        # 3. Advance each unison string and aggregate bridge boundary forces
        total_bridge_T = 0.0
        total_bridge_P = 0.0
        coupling_per_string_T = f_coupling_T / num_str
        coupling_per_string_P = f_coupling_P / num_str

        for s in self.strings:
            s.step(f_hammer_per_string, coupling_per_string_T, coupling_per_string_P)
            fb_T, fb_P = s.get_bridge_forces()
            total_bridge_T += fb_T
            total_bridge_P += fb_P

        return total_bridge_T, total_bridge_P
