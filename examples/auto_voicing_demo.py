"""Tier 9 Demo: Automated Physical Parameter Calibration (Auto-Voicing) against Steinway Model B recordings."""

import os
import sys
import numpy as np
import scipy.io.wavfile as wavfile

# Add src to path
sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "src")))

from physics_piano.autovoicing.auto_voicer import AutoVoicer
from physics_piano.autovoicing.pino_surrogate import SoundboardPINOSurrogate


def run_demo():
    print("================================================================================")
    print(" [Tier 9] Differentiable Physics & Auto-Voicing Calibration Pipeline")
    print("================================================================================\n")

    # 1. Test Soundboard PINO Surrogate
    print("[1] Evaluating Soundboard PINO Surrogate Plate Resonances...")
    surrogate = SoundboardPINOSurrogate()
    modes = surrogate.predict_modal_frequencies(max_m=6, max_n=6)
    print(f"    - Identified {len(modes)} 2D orthotropic plate modes below 8 kHz")
    for m, n, fn, rad in modes[:5]:
        print(f"      * Mode ({m},{n}): {fn:7.1f} Hz (Radiation Efficiency factor: {rad:.2f})")

    # 2. Run Auto-Voicing on Steinway Model B Sample
    ref_path = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "steinway_b_samples", "Piano.mf.A4.wav"))
    if not os.path.exists(ref_path):
        print(f"[-] Reference sample not found at {ref_path}")
        return

    print(f"\n[2] Auto-Voicing Calibration on Target Sample: {os.path.basename(ref_path)}")
    sr, audio = wavfile.read(ref_path)

    voicer = AutoVoicer(sample_rate=48000.0, num_modes=25)
    print("    - Running iterative Nelder-Mead optimization on Multi-Resolution Spectral Loss...")
    result = voicer.calibrate_note(
        target_audio=audio,
        target_sr=float(sr),
        midi_note=69,
        target_f0=440.0,
        velocity=0.75,
        max_iterations=15,
    )

    print(f"\n[3] Optimization Results:")
    print(f"    - Initial MRSL Loss: {result.initial_loss:.4f}")
    print(f"    - Final Calibrated Loss: {result.final_loss:.4f} (Improvement: {((result.initial_loss - result.final_loss)/result.initial_loss)*100:.1f}%)")
    print(f"    - Calibrated Multipliers:")
    for k, v in result.calibrated_params.items():
        if "multiplier" in k:
            print(f"      * {k:32s}: {v:.4f}")

    out_json = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "calibrated_voicing_A4.json"))
    result.save_json(out_json)
    print(f"\n[+] Saved calibrated physical profile to: {out_json}")


if __name__ == "__main__":
    run_demo()
