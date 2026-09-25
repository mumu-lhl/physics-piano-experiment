"""Automated objective acoustic comparison between physical synthesis engine and authentic Steinway Model B recordings."""

import os
import sys
import numpy as np
import scipy.io.wavfile as wavfile

# Add src to path
sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "src")))

from physics_piano.api import PianoSynth
from physics_piano.metrics.alignment import align_audio_onsets
from physics_piano.metrics.mrsl import compute_mrsl
from physics_piano.metrics.peaq import estimate_peaq_odg
from physics_piano.metrics.inharmonicity import estimate_inharmonicity_from_audio
from physics_piano.metrics.transient import analyze_transient_onset

def compare_note(sample_path: str, note_name: str, target_f0: float, velocity: float = 0.75):
    print(f"\n=======================================================")
    print(f"[*] Comparing Note: {note_name} against {os.path.basename(sample_path)}")
    print(f"=======================================================")

    if not os.path.exists(sample_path):
        print(f"[-] Reference file not found: {sample_path}")
        return

    # 1. Read Steinway B recording
    ref_sr, ref_audio = wavfile.read(sample_path)
    if ref_audio.ndim > 1:
        ref_audio = ref_audio.mean(axis=1)
    ref_audio = ref_audio.astype(np.float64) / 32768.0
    ref_max = np.max(np.abs(ref_audio))
    if ref_max > 0:
        ref_audio /= ref_max

    # 2. Synthesize note with physical engine (using same sample rate)
    synth = PianoSynth(sample_rate=ref_sr, num_modes=35)
    # Render 3 seconds of sound
    duration_sec = 2.5
    synth_audio = synth.render_note(note_name, velocity=velocity, duration=duration_sec, sustain=True)
    if synth_audio.ndim > 1:
        synth_audio = synth_audio.mean(axis=1)
    synth_max = np.max(np.abs(synth_audio))
    if synth_max > 0:
        synth_audio /= synth_max

    # 3. Align onsets via DTW / peak correlation
    n_samples = int(duration_sec * ref_sr)
    aligned_synth, aligned_ref = align_audio_onsets(synth_audio[:n_samples], ref_audio[:n_samples])

    # 4. Multi-Resolution STFT Loss
    mrsl_res = compute_mrsl(aligned_synth, aligned_ref, fft_sizes=[512, 1024, 2048])

    # 5. PEAQ Objective Difference Grade (ODG)
    peaq_res = estimate_peaq_odg(aligned_synth, aligned_ref, sample_rate=ref_sr)

    # 6. Transient Rise Time
    tr_synth = analyze_transient_onset(aligned_synth, sample_rate=float(ref_sr))
    tr_ref = analyze_transient_onset(aligned_ref, sample_rate=float(ref_sr))

    # 7. Inharmonicity B factor estimation
    b_synth = estimate_inharmonicity_from_audio(aligned_synth, target_f0, 0.0001, float(ref_sr))
    b_ref = estimate_inharmonicity_from_audio(aligned_ref, target_f0, 0.0001, float(ref_sr))

    print(f"[1] Multi-Resolution STFT Loss (MRSL):")
    print(f"    - Total Loss:               {mrsl_res['total_mrsl_loss']:.4f}")
    print(f"    - Spectral Convergence:     {mrsl_res['avg_spectral_convergence']:.4f}")
    print(f"    - Log-Magnitude Loss:       {mrsl_res['avg_log_magnitude']:.4f}")

    print(f"[2] ITU-R BS.1387 PEAQ Auditory Score:")
    print(f"    - Objective Diff Grade(ODG): {peaq_res['odg']:.2f} (Target >= -1.2 for high fidelity)")
    print(f"    - Relative Auditory Error:   {peaq_res['relative_auditory_error']:.4f}")
    print(f"    - Passing Status:            {'PASSED' if peaq_res['passed'] else 'NEEDS_TUNING'}")

    print(f"[3] Transient Rise Time (10%-90% Energy Wavefront):")
    print(f"    - Physical Synthesis:        {tr_synth['rise_time_ms']:.2f} ms")
    print(f"    - Steinway B Reference:      {tr_ref['rise_time_ms']:.2f} ms")

    print(f"[4] Inharmonicity B Coefficient:")
    print(f"    - Physical Synthesis:        {b_synth['fitted_b']:.6e} (Cent Std: {b_synth['cent_std']:.2f} cents)")
    print(f"    - Steinway B Reference:      {b_ref['fitted_b']:.6e} (Cent Std: {b_ref['cent_std']:.2f} cents)")


if __name__ == "__main__":
    compare_note("steinway_b_samples/Piano.mf.A4.wav", "A4", 440.0, velocity=0.75)
    compare_note("steinway_b_samples/Piano.mf.C4.wav", "C4", 261.63, velocity=0.75)
    compare_note("steinway_b_samples/Piano.mf.C2.wav", "C2", 65.41, velocity=0.75)
