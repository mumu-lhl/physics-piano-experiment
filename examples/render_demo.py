"""Demo: Render physical piano single notes and chords to audio files."""

import sys
import os
sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "src")))

from physics_piano.api import PianoSynth

def main():
    print("[*] Initializing Physics Piano Synthesizer...")
    synth = PianoSynth(sample_rate=48000, num_modes=35)

    print("[*] Rendering single note A4 (440 Hz concert pitch)...")
    a4_audio = synth.render_note("A4", velocity=0.8, duration=3.0, sustain=False)
    synth.export_wav("piano_single_a4.wav", a4_audio)
    print("    -> Saved piano_single_a4.wav")

    print("[*] Rendering C Major chord (C4, E4, G4) with sympathetic resonance...")
    chord_audio = synth.render_chord(["C4", "E4", "G4"], velocity=0.85, duration=4.0, sustain=True)
    synth.export_wav("piano_chord_c_major.wav", chord_audio)
    print("    -> Saved piano_chord_c_major.wav")

    print("[+] Render demo complete!")

if __name__ == "__main__":
    main()
