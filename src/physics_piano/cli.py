"""Command-line interface (CLI) for physics-based piano synthesis and objective verification."""

import sys
import argparse
import time
import numpy as np

from physics_piano.params.grand_piano import pitch_name_to_midi, midi_to_pitch_name, generate_grand_piano_parameters
from physics_piano.api import PianoSynth
from physics_piano.dsp.audio import write_wav
from physics_piano.metrics.inharmonicity import estimate_inharmonicity_from_audio
from physics_piano.metrics.decay_edc import analyze_two_stage_decay, compute_schroeder_edc
from physics_piano.metrics.contact_time import measure_hammer_contact_time


def cmd_render(args):
    """Render a single note or chord to WAV."""
    synth = PianoSynth(sample_rate=args.sample_rate, num_modes=args.modes)

    if args.chord:
        pitches = [p.strip() for p in args.chord.split(",")]
        print(f"[*] Rendering physical chord: {pitches} (duration={args.duration}s, vel={args.velocity}, sustain={args.sustain}, una_corda={args.una_corda}, rad={args.radiation})")
        t0 = time.time()
        audio = synth.render_chord(
            pitches, velocity=args.velocity, duration=args.duration, sustain=args.sustain,
            una_corda=args.una_corda, radiation_mode=args.radiation
        )
        elapsed = time.time() - t0
    else:
        note_name = args.note or "C4"
        midi = pitch_name_to_midi(note_name)
        key_info = synth.engine.key_params[midi]
        s0 = key_info.strings[0]
        print(f"[*] Rendering Note: {note_name} (MIDI {midi})")
        print(f"    - Target f0: {key_info.target_f0:.2f} Hz, Unisons: {key_info.num_unisons}")
        print(f"    - Length L: {s0.length:.3f} m, Radius r: {s0.radius*1e3:.3f} mm, Tension T0: {s0.tension:.1f} N")
        print(f"    - Theoretical Inharmonicity B: {s0.inharmonicity_b:.6e}")
        print(f"    - Felt Exponent p: {key_info.hammer.exponent:.2f}, Mass: {key_info.hammer.mass*1e3:.2f} g")
        print(f"    - Una Corda: {args.una_corda}, Radiation: {args.radiation}")

        t0 = time.time()
        audio = synth.render_note(
            note_name, velocity=args.velocity, duration=args.duration, sustain=args.sustain,
            una_corda=args.una_corda, radiation_mode=args.radiation
        )
        elapsed = time.time() - t0

    write_wav(args.output, audio, sample_rate=args.sample_rate)
    rtf = elapsed / args.duration
    print(f"[+] Output saved: {args.output}")
    print(f"[+] Render time: {elapsed:.3f}s (Real-time factor: {rtf:.2f}x)")


def cmd_verify(args):
    """Run objective acoustic metrics according to physical verification rules."""
    synth = PianoSynth(sample_rate=args.sample_rate, num_modes=args.modes)
    target_note = args.note or "A4"
    midi = pitch_name_to_midi(target_note)
    key_info = synth.engine.key_params[midi]
    s0 = key_info.strings[0]

    print("=" * 65)
    print(f"OBJECTIVE PHYSICAL ACOUSTIC VERIFICATION: {target_note} (MIDI {midi})")
    print("=" * 65)

    if args.metric in ("inharmonicity", "all"):
        print("\n--- 1. Dispersion & Inharmonicity B Factor Analysis ---")
        audio = synth.render_note(target_note, velocity=0.85, duration=2.5, sustain=False)
        res = estimate_inharmonicity_from_audio(
            audio, f0_nominal=key_info.target_f0, theoretical_b=s0.inharmonicity_b,
            sample_rate=args.sample_rate, max_partials=24
        )
        print(f"Nominal f0:              {res['f0_nominal']:.2f} Hz")
        print(f"Fitted f0:               {res['f0_fit']:.2f} Hz")
        print(f"Theoretical B (formula): {res['theoretical_b']:.6e}")
        print(f"Fitted B (regression):   {res['fitted_b']:.6e}")
        print(f"Relative Error epsilon_B: {res['relative_error_b_percent']:.2f}% (Threshold: <= 2.5%)")
        print(f"Cent Std Dev sigma(DC):  {res['cent_std']:.2f} cents (Threshold: < 2.5 cents)")
        print(f"Partials Analyzed:       {res['num_partials_analyzed']}")
        print(f"Status:                  {'[PASS]' if res['passed'] else '[FAIL]'}")

    if args.metric in ("edc", "all"):
        print("\n--- 2. Schroeder EDC & Two-Stage Decay (Prompt / Aftersound) ---")
        audio = synth.render_note(target_note, velocity=0.8, duration=3.5, sustain=True)
        res = analyze_two_stage_decay(audio, sample_rate=args.sample_rate)
        print(f"Prompt Time Constant tau_prompt: {res['tau_prompt']:.3f} s")
        print(f"Aftersound Constant tau_after:   {res['tau_after']:.3f} s")
        print(f"Decay Rate Ratio tau_after/tau_p: {res['decay_ratio']:.2f} (Physical Window: {res['target_range']})")
        print(f"Status:                          {'[PASS]' if res['passed'] else '[FAIL]'}")

    if args.metric in ("contact", "all"):
        print("\n--- 3. Nonlinear Felt Contact Dynamics & Time Contraction ---")
        velocities = [0.2, 0.4, 0.6, 0.8, 1.0]
        res = measure_hammer_contact_time(s0, key_info.hammer, velocities, sample_rate=args.sample_rate)
        print("Velocity ladder -> Contact duration (ms):")
        for v, dur in zip(res["velocities"], res["contact_times_ms"]):
            print(f"  v = {v:4.1f}  ->  {dur:6.3f} ms")
        print(f"Monotonic contraction verified:  {res['is_monotonic_contracting']}")
        print(f"Contraction Ratio:               {res['contraction_ratio']:.2f}x")
        print(f"Status:                          {'[PASS]' if res['passed'] else '[FAIL]'}")

    if args.metric in ("dynamics", "all"):
        print("\n--- 4. Dynamic Spectral Centroid Scaling Slope (kappa_dyn) ---")
        from physics_piano.metrics.dynamic_centroid import measure_dynamic_centroid_slope
        res_dyn = measure_dynamic_centroid_slope(key_info, velocities=[0.2, 0.4, 0.6, 0.8, 1.0], sample_rate=args.sample_rate)
        print(f"Dynamic log-slope kappa_dyn:     {res_dyn['kappa_dyn']:.3f} (Target: {res_dyn['target_range']})")
        print(f"Status:                          {'[PASS]' if res_dyn['passed'] else '[FAIL]'}")

    if args.metric in ("transient", "all"):
        print("\n--- 5. Initial Attack Transient & Envelope Rise Time (Delta t_10-90) ---")
        from physics_piano.metrics.transient import analyze_transient_onset
        audio = synth.render_note(target_note, velocity=0.9, duration=0.5, sustain=False)
        res_trans = analyze_transient_onset(audio, sample_rate=args.sample_rate)
        print(f"Rise Time Delta t_10-90:         {res_trans['rise_time_ms']:.2f} ms (Target: {res_trans['target_rise_time']})")
        print(f"Peak Arrival Time:               {res_trans['peak_arrival_time_ms']:.2f} ms (Target: {res_trans['target_peak_arrival']})")
        print(f"Status:                          {'[PASS]' if res_trans['passed'] else '[FAIL]'}")

    if args.metric in ("octave", "all"):
        print("\n--- 6. 1/1 Octave Band Filter Bank Decay (RMSE_T60) ---")
        from physics_piano.metrics.octave_decay import analyze_octave_t60
        audio = synth.render_note(target_note, velocity=0.8, duration=3.0, sustain=True)
        res_oct = analyze_octave_t60(audio, sample_rate=args.sample_rate, f0=key_info.target_f0)
        print(f"RMSE_T60 across octave bands:    {res_oct['rmse_t60']:.4f} (Target: <= 0.08)")
        print(f"Status:                          {'[PASS]' if res_oct['passed'] else '[FAIL]'}")
    print("=" * 65)


def cmd_benchmark(args):
    """Run performance throughput benchmark across keys."""
    synth = PianoSynth(sample_rate=args.sample_rate, num_modes=args.modes)
    test_notes = ["C2", "C3", "C4", "C5", "C6"]
    dur = 1.0

    print(f"[*] Benchmarking physical discrete solver across {len(test_notes)} registers ({dur}s each, modes={args.modes})...")
    total_time = 0.0
    for note in test_notes:
        t0 = time.time()
        _ = synth.render_note(note, velocity=0.8, duration=dur, sustain=False)
        dt = time.time() - t0
        total_time += dt
        print(f"    - {note}: {dt:.3f}s (RTF = {dt/dur:.2f}x)")

    avg_rtf = (total_time) / (len(test_notes) * dur)
    total_samples = len(test_notes) * dur * args.sample_rate
    throughput = total_samples / total_time
    print(f"[+] Total elapsed: {total_time:.3f}s")
    print(f"[+] Average Real-time Factor (RTF): {avg_rtf:.2f}x")
    print(f"[+] Throughput: {throughput:,.0f} samples/sec")


def main():
    parser = argparse.ArgumentParser(
        prog="physics-piano",
        description="First-principles physical modeling piano sound synthesis and verification CLI"
    )
    subparsers = parser.add_subparsers(dest="command", required=True)

    # 1. render
    p_render = subparsers.add_parser("render", help="Synthesize note or chord to WAV")
    p_render.add_argument("-n", "--note", type=str, default="C4", help="Note name (e.g. C4, A4, F#3)")
    p_render.add_argument("-c", "--chord", type=str, default=None, help="Comma-separated chord notes (e.g. C4,E4,G4)")
    p_render.add_argument("-v", "--velocity", type=float, default=0.8, help="Strike velocity in (0, 1]")
    p_render.add_argument("-d", "--duration", type=float, default=3.0, help="Duration in seconds")
    p_render.add_argument("-s", "--sustain", action="store_true", help="Hold sustain pedal down")
    p_render.add_argument("--una-corda", action="store_true", help="Engage Una Corda soft pedal")
    p_render.add_argument("--radiation", choices=["modal", "upols"], default="modal", help="Soundboard radiation mode")
    p_render.add_argument("-m", "--modes", type=int, default=35, help="Number of modal oscillators per string")
    p_render.add_argument("-r", "--sample-rate", type=int, default=48000, help="Audio sample rate (Hz)")
    p_render.add_argument("-o", "--output", type=str, default="piano_output.wav", help="Output WAV path")
    p_render.set_defaults(func=cmd_render)

    # 2. verify
    p_verify = subparsers.add_parser("verify", help="Run objective physical verification tests")
    p_verify.add_argument("metric", choices=["inharmonicity", "edc", "contact", "dynamics", "transient", "octave", "all"], help="Metric to verify")
    p_verify.add_argument("-n", "--note", type=str, default="A4", help="Note to test")
    p_verify.add_argument("-m", "--modes", type=int, default=35, help="Number of modes")
    p_verify.add_argument("-r", "--sample-rate", type=int, default=48000, help="Audio sample rate")
    p_verify.set_defaults(func=cmd_verify)

    # 3. benchmark
    p_bench = subparsers.add_parser("benchmark", help="Measure numerical synthesis performance")
    p_bench.add_argument("-m", "--modes", type=int, default=30, help="Number of modes")
    p_bench.add_argument("-r", "--sample-rate", type=int, default=48000, help="Audio sample rate")
    p_bench.set_defaults(func=cmd_benchmark)

    args = parser.parse_args()
    args.func(args)


if __name__ == "__main__":
    main()
