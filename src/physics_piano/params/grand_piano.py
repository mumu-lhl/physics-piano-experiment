"""Grand piano 88-key physical parameter generator based on concert grand acoustics."""

import math
from typing import Dict, List
from physics_piano.params.schema import StringPhysicalParameters, HammerPhysicalParameters, KeyParameters

PITCH_NAMES = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"]


def midi_to_pitch_name(midi_note: int) -> str:
    """Convert MIDI number (21..108) to standard pitch name, e.g. 60 -> 'C4'."""
    octave = (midi_note // 12) - 1
    name = PITCH_NAMES[midi_note % 12]
    return f"{name}{octave}"


def pitch_name_to_midi(pitch: str) -> int:
    """Convert pitch string (e.g. 'A4', 'C#3', 'Eb5') to MIDI number."""
    p = pitch.strip()
    if not p:
        raise ValueError("Empty pitch string")
    
    # Handle accidental
    if len(p) >= 3 and p[1] in ('#', 'b'):
        accidental = p[1]
        note_letter = p[0].upper()
        octave_str = p[2:]
        if accidental == '#':
            base_name = note_letter + '#'
        else: # flat
            # map flat to sharp
            flats = {'Db': 'C#', 'Eb': 'D#', 'Gb': 'F#', 'Ab': 'G#', 'Bb': 'A#'}
            base_name = flats.get(note_letter + 'b', note_letter)
    else:
        base_name = p[0].upper()
        octave_str = p[1:]

    octave = int(octave_str)
    note_idx = PITCH_NAMES.index(base_name)
    return (octave + 1) * 12 + note_idx


def compute_railsback_cents(midi_note: int) -> float:
    """Compute Railsback stretch tuning offset in cents for acoustic grand piano.
    
    Inharmonicity B causes partials to stretch upward: f_n = n * f0 * sqrt(1 + B * n^2).
    To align upper partials of bass notes with fundamental frequencies of treble notes,
    concert pianos employ a characteristic Railsback stretch tuning curve:
      - Bass (A0): -30 to -35 cents
      - Mid (C4): 0 cents
      - Treble (C8): +30 to +38 cents
    """
    if midi_note < 60:
        norm = (60.0 - float(midi_note)) / 39.0
        return -32.0 * (norm ** 1.85)
    elif midi_note > 60:
        norm = (float(midi_note) - 60.0) / 48.0
        return 35.0 * (norm ** 2.1)
    return 0.0


def generate_grand_piano_parameters(
    num_modes: int = 35,
    stretch_tuning: bool = False
) -> Dict[int, KeyParameters]:
    """Generate physically-grounded parameters for all 88 keys of a concert grand piano.
    
    Discontinuous Mechanical Break Points (calibrated to Steinway D-274 / Yamaha CFX):
      - Bass (21-28, A0..E1): 1 heavy single-wound copper string per note
      - Tenor (29-34, F1..Bb2): 2 double-wound copper strings (bichords)
      - Treble (35-108, B2..C8): 3 plain high-carbon steel wire strings (trichords)
    """
    key_params_dict = {}

    for midi in range(21, 109):
        norm_key = (midi - 21) / (108 - 21)  # 0.0 at A0, 1.0 at C8
        cents_stretch = compute_railsback_cents(midi) if stretch_tuning else 0.0
        f0 = 440.0 * (2.0 ** ((midi - 69 + cents_stretch / 100.0) / 12.0))
        pitch_name = midi_to_pitch_name(midi)

        # 1. Discontinuous break points across keyboard
        if midi <= 28:
            num_unisons = 1
            detuning = [0.0]
        elif midi <= 34:
            num_unisons = 2
            detuning = [-0.25, 0.25]
        else:
            num_unisons = 3
            detuning = [-0.38, 0.0, 0.38]

        # 2. String active length L (meters)
        # Scaled smoothly: ~1.85m at A0, ~0.62m at C4, down to ~0.065m at C8
        if midi <= 40:
            length = 1.85 - (midi - 21) * 0.045
        else:
            length = 0.065 + (0.95 - 0.065) * ((1.0 - norm_key) ** 1.35)

        # 3. String wire radius r (meters) and density rho
        # Discontinuous transitions between wound and plain wire sections
        if midi <= 28:
            # Single-wound copper bass strings
            radius = 0.00072 - (norm_key * 0.00012)
            density = 7850.0 * 3.2  # Heavy copper wrapping
        elif midi <= 34:
            # Double-wound copper bichords
            radius = 0.00058 - (norm_key * 0.00010)
            density = 7850.0 * 2.0  # Medium copper winding
        else:
            # Plain high-tensile music wire steel
            radius = 0.00048 - (norm_key * 0.00015)
            density = 7850.0  # High-carbon music wire steel

        youngs_modulus = 2.0e11  # Pa
        area = math.pi * (radius ** 2)
        mu = density * area

        # Calculate static tension T0 to match target fundamental f0:
        # f0 = (1 / (2*L)) * sqrt(T0 / mu)  =>  T0 = mu * (2 * L * f0)^2
        tension = mu * ((2.0 * length * f0) ** 2)

        # Clamp tension within realistic piano wire limits (450 N ~ 1200 N)
        if tension < 450.0:
            tension = 550.0
            # Adjust effective linear density to preserve f0
            mu = tension / ((2.0 * length * f0) ** 2)
            density = mu / area
        elif tension > 1200.0:
            tension = 1050.0
            mu = tension / ((2.0 * length * f0) ** 2)
            density = mu / area

        # Frequency-dependent damping parameters:
        # sigma0: air friction (higher in bass)
        # sigma1: internal viscoelastic losses (higher in treble)
        sigma0 = 0.8 - (0.4 * norm_key)
        sigma1 = 5.0e-6 + (3.0e-5 * norm_key)

        # Strike location x_h / L (~1/8 in bass, slightly closer to 1/10 ~ 1/12 in treble)
        strike_ratio = 0.125 - (0.035 * norm_key)

        # Create StringPhysicalParameters
        string_params = [
            StringPhysicalParameters(
                length=length,
                radius=radius,
                density=density,
                youngs_modulus=youngs_modulus,
                tension=tension,
                sigma0=sigma0,
                sigma1=sigma1,
                strike_ratio=strike_ratio,
                num_modes=num_modes,
                polarization_mistuning=0.0012 + (0.0008 * norm_key)
            )
            for _ in range(num_unisons)
        ]

        # 4. Hammer parameters
        # Hammer mass decreases from 11.5g (A0) down to 5.2g (C8)
        hammer_mass = 0.0115 - (0.0063 * norm_key)

        # Non-linear felt stiffness Kh increases from bass (1.5e9) to treble (8.0e11)
        hammer_stiffness = 1.5e9 * (10.0 ** (norm_key * 2.8))

        # Felt compression exponent p: ~2.0 in soft bass, up to ~3.1 in hard treble
        hammer_p = 2.0 + (1.1 * norm_key)

        # Felt dissipation lambda
        hammer_dissipation = 1.5e4 + (4.0e4 * norm_key)

        hammer_param = HammerPhysicalParameters(
            mass=hammer_mass,
            stiffness=hammer_stiffness,
            exponent=hammer_p,
            dissipation=hammer_dissipation,
            max_velocity=5.2,
            velocity_gamma=1.55
        )

        key_params_dict[midi] = KeyParameters(
            midi_note=midi,
            pitch_name=pitch_name,
            target_f0=f0,
            strings=string_params,
            hammer=hammer_param,
            num_unisons=num_unisons,
            detuning_cents=detuning
        )

    return key_params_dict
