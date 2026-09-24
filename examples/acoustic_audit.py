"""Demonstration of objective acoustic verification metrics on synthesized notes."""

import sys
import os
sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "src")))

from physics_piano.api import PianoSynth
from physics_piano.metrics.inharmonicity import estimate_inharmonicity_from_audio
from physics_piano.metrics.decay_edc import analyze_two_stage_decay
from physics_piano.metrics.contact_time import measure_hammer_contact_time

def main():
    note = "A4"
    print(f"[*] Running physical acoustic audit for note {note}...")
    synth = PianoSynth(sample_rate=48000, num_modes=35)
    key_info = synth.engine.key_params[69]
    s0 = key_info.strings[0]

    # 1. Inharmonicity B factor check
    audio = synth.render_note(note, velocity=0.85, duration=2.5, sustain=False)
    b_res = estimate_inharmonicity_from_audio(
        audio, f0_nominal=key_info.target_f0, theoretical_b=s0.inharmonicity_b,
        sample_rate=48000.0, max_partials=24
    )
    print(f"[1] Inharmonicity B Error: {b_res['relative_error_b_percent']:.2f}% (Std: {b_res['cent_std']:.2f} cents) -> Passed: {b_res['passed']}")

    # 2. Two-stage decay check
    audio_free = synth.render_note(note, velocity=0.8, duration=3.5, sustain=True)
    edc_res = analyze_two_stage_decay(audio_free, sample_rate=48000.0)
    print(f"[2] Two-Stage Decay Ratio (tau_after / tau_prompt): {edc_res['decay_ratio']:.2f} -> Passed: {edc_res['passed']}")

    # 3. Hammer contact contraction
    c_res = measure_hammer_contact_time(s0, key_info.hammer, [0.2, 0.4, 0.6, 0.8, 1.0], sample_rate=48000.0)
    print(f"[3] Contact Duration Contraction Ratio: {c_res['contraction_ratio']:.2f}x -> Passed: {c_res['passed']}")

if __name__ == "__main__":
    main()
