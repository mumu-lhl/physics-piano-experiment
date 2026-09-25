"""Comprehensive test suite for physical modeling piano engine."""

import unittest
import math
import numpy as np

from physics_piano.params.schema import StringPhysicalParameters, HammerPhysicalParameters
from physics_piano.params.grand_piano import generate_grand_piano_parameters, pitch_name_to_midi, midi_to_pitch_name
from physics_piano.core.string import StiffStringModal
from physics_piano.core.hammer import HuntCrossleyHammer
from physics_piano.core.bridge import BridgeSoundboard
from physics_piano.core.voice import PianoVoice
from physics_piano.engine import PianoEngine
from physics_piano.api import PianoSynth
from physics_piano.dsp.audio import normalize_audio, write_wav
from physics_piano.metrics.contact_time import measure_hammer_contact_time
from physics_piano.metrics.decay_edc import compute_schroeder_edc


class TestStiffString(unittest.TestCase):
    def setUp(self):
        self.param = StringPhysicalParameters(
            length=0.62, radius=0.00045, density=7850.0, youngs_modulus=2.0e11,
            tension=750.0, sigma0=0.6, sigma1=1.5e-5, num_modes=20
        )
        self.string = StiffStringModal(self.param, sample_rate=48000.0)

    def test_fundamental_and_inharmonicity(self):
        self.assertGreater(self.param.fundamental_hz, 200.0)
        self.assertLess(self.param.fundamental_hz, 400.0)
        self.assertGreater(self.param.inharmonicity_b, 1e-5)
        self.assertLess(self.param.inharmonicity_b, 1e-2)

    def test_numerical_stability(self):
        # Step string with impulse then let it decay for 1000 steps
        self.string.step(50.0)
        max_v = 0.0
        for _ in range(1000):
            self.string.step(0.0)
            u, v = self.string.get_strike_displacement_and_velocity()
            max_v = max(max_v, abs(v))
            self.assertFalse(np.isnan(u))
            self.assertFalse(np.isinf(u))
        self.assertGreater(max_v, 0.0)

    def test_high_register_nyquist_truncation(self):
        # In treble notes, frequencies above 0.95 * fs/2 must be truncated
        fs = 48000.0
        c8_param = generate_grand_piano_parameters(num_modes=35)[108].strings[0]
        c8_string = StiffStringModal(c8_param, sample_rate=fs)
        
        # Verify that all retained mode frequencies are strictly below Nyquist
        max_allowed_freq = 0.95 * (fs / 2.0)
        retained_freqs = c8_string.omega_T / (2.0 * np.pi)
        self.assertTrue(all(f <= max_allowed_freq for f in retained_freqs))
        # C8 should have around 5 modes below Nyquist, not 35
        self.assertLess(c8_string.M, 10)
        self.assertGreaterEqual(c8_string.M, 1)


class TestHammerContact(unittest.TestCase):
    def setUp(self):
        self.s_param = StringPhysicalParameters(
            length=0.62, radius=0.00045, density=7850.0, youngs_modulus=2.0e11,
            tension=750.0, sigma0=0.6, sigma1=1.5e-5, num_modes=20
        )
        self.h_param = HammerPhysicalParameters(
            mass=0.009, stiffness=3.0e9, exponent=2.4, dissipation=2.0e4
        )

    def test_contact_force_non_negative(self):
        hammer = HuntCrossleyHammer(self.h_param, 48000.0)
        hammer.strike(0.8)
        hammer.u_h = 0.0002  # felt compressed by 0.2mm
        f = hammer.compute_force(u_string=0.0, v_string=0.0)
        self.assertGreater(f, 0.0)
        # Pulling back string beyond hammer
        hammer.u_h = -0.001
        f_neg = hammer.compute_force(u_string=0.0, v_string=0.0)
        self.assertEqual(f_neg, 0.0)

    def test_contact_time_contraction(self):
        res = measure_hammer_contact_time(self.s_param, self.h_param, [0.2, 0.5, 0.9], sample_rate=48000.0)
        self.assertTrue(res["is_monotonic_contracting"])
        self.assertGreater(res["contraction_ratio"], 1.1)

    def test_rapid_retrigger_continuity(self):
        # Test striking hammer twice in rapid succession while string is already vibrating
        hammer = HuntCrossleyHammer(self.h_param, 48000.0)
        hammer.strike(0.8, initial_u_string=0.0)
        self.assertTrue(hammer.is_active)
        # Simulate re-strike with string already displaced
        hammer.strike(0.9, initial_u_string=0.0005)
        self.assertTrue(hammer.is_active)
        self.assertLessEqual(hammer.u_h, 0.0005)


class TestGrandPianoParameters(unittest.TestCase):
    def test_pitch_naming(self):
        self.assertEqual(pitch_name_to_midi("A4"), 69)
        self.assertEqual(pitch_name_to_midi("C4"), 60)
        self.assertEqual(midi_to_pitch_name(60), "C4")
        self.assertEqual(midi_to_pitch_name(69), "A4")

    def test_88_keys_generation(self):
        params = generate_grand_piano_parameters(num_modes=15)
        self.assertEqual(len(params), 88)
        self.assertIn(21, params)
        self.assertIn(108, params)
        # Bass has 1 string, treble has 3
        self.assertEqual(params[21].num_unisons, 1)
        self.assertEqual(params[60].num_unisons, 3)
        self.assertLess(params[21].target_f0, params[108].target_f0)

    def test_all_88_keys_stability(self):
        # Scan through representation of 88 keys to guarantee no NaN or overflow
        synth = PianoSynth(sample_rate=48000, num_modes=20)
        # Test sample spread across all 8 octaves
        sample_keys = [21, 32, 44, 60, 69, 84, 96, 108]
        for k in sample_keys:
            audio = synth.render_note(k, velocity=0.85, duration=0.04, sustain=False, normalize=False)
            self.assertFalse(np.isnan(audio).any(), f"Key {k} produced NaN")
            self.assertFalse(np.isinf(audio).any(), f"Key {k} produced Inf")
            self.assertLess(np.max(np.abs(audio)), 500.0, f"Key {k} experienced overflow")


class TestPianoSynthAPI(unittest.TestCase):
    def setUp(self):
        self.synth = PianoSynth(sample_rate=48000, num_modes=20)

    def test_render_single_note(self):
        audio = self.synth.render_note("A4", velocity=0.7, duration=0.2, sustain=False)
        self.assertEqual(audio.shape[1], 2)
        self.assertGreater(len(audio), 0)
        self.assertFalse(np.all(audio == 0))

    def test_render_chord(self):
        audio = self.synth.render_chord(["C4", "E4", "G4"], velocity=0.8, duration=0.25, sustain=True)
        self.assertEqual(audio.shape[1], 2)
        self.assertGreater(len(audio), 0)

    def test_dense_polyphony_stability(self):
        # Test 8-note dense polyphonic cluster with sustain pedal
        dense_notes = ["C3", "G3", "C4", "E4", "G4", "B4", "D5", "G5"]
        audio = self.synth.render_chord(dense_notes, velocity=0.85, duration=0.3, sustain=True, normalize=False)
        self.assertFalse(np.isnan(audio).any(), "Dense chord produced NaN")
        self.assertFalse(np.isinf(audio).any(), "Dense chord produced Inf")
        # Ensure peak does not explode
        self.assertLess(np.max(np.abs(audio)), 100.0)

    def test_pedal_transition_clicks(self):
        # Test dynamic pedal down and pedal up transition during note decay
        engine = PianoEngine(sample_rate=48000, num_modes=20)
        engine.note_on(60, velocity=0.8)
        chunk1 = engine.render(0.1)
        engine.pedal_down()
        chunk2 = engine.render(0.1)
        engine.note_off(60)
        chunk3 = engine.render(0.1)
        engine.pedal_up()
        chunk4 = engine.render(0.15)
        combined = np.concatenate([chunk1, chunk2, chunk3, chunk4], axis=0)
        # Check no discontinuities or extreme click spikes
        diffs = np.diff(combined[:, 0])
        self.assertLess(np.max(np.abs(diffs)), 20.0, "Pedal transition produced click artifact")

    def test_normalization_and_edc(self):
        arr = np.array([0.1, 0.5, -0.8, 0.3])
        norm = normalize_audio(arr, target_peak=0.9)
        self.assertAlmostEqual(np.max(np.abs(norm)), 0.9)
        edc = compute_schroeder_edc(arr)
        self.assertEqual(len(edc), len(arr))
        self.assertLessEqual(edc[-1], edc[0])

    def test_edge_cases_and_clamping(self):
        # Test empty audio normalization
        empty = np.zeros((0, 2))
        res_empty = normalize_audio(empty)
        self.assertEqual(len(res_empty), 0)

        # Test NaN sanitization
        nan_arr = np.array([[np.nan, 1.0], [0.5, np.inf]])
        clean_arr = normalize_audio(nan_arr, target_peak=0.95)
        self.assertFalse(np.isnan(clean_arr).any())
        self.assertFalse(np.isinf(clean_arr).any())

        # Test extreme velocity clamping (0.0001 and 1.5)
        audio_low = self.synth.render_note("C4", velocity=0.0001, duration=0.05)
        audio_high = self.synth.render_note("C4", velocity=1.5, duration=0.05)
        self.assertGreater(len(audio_low), 0)
        self.assertGreater(len(audio_high), 0)


class TestContinuumMechanicsAndSAV(unittest.TestCase):
    def test_geometric_nonlinearity_tension_modulation(self):
        # High velocity strike should produce significant longitudinal tension modulation FL
        param = generate_grand_piano_parameters(num_modes=20)[24].strings[0]
        string = StiffStringModal(param, sample_rate=48000.0)
        string.step(f_hammer=200.0)
        f_t, f_p, f_l = string.get_bridge_forces()
        self.assertGreater(f_l, 0.0)
        # Energy should be positive
        self.assertGreater(string.get_energy(), 0.0)

    def test_sav_hammer_contact_stability(self):
        h_param = HammerPhysicalParameters(
            mass=0.01, stiffness=4.0e9, exponent=2.5, dissipation=2.0e4
        )
        hammer = HuntCrossleyHammer(h_param, 48000.0)
        hammer.use_sav = True
        hammer.strike(0.95)
        # Verify force is non-negative and stable under multiple compressions
        for _ in range(50):
            f = hammer.compute_force(u_string=0.0001, v_string=0.0)
            self.assertGreaterEqual(f, 0.0)
            hammer.advance(f)
            self.assertFalse(math.isnan(hammer.u_h))
            self.assertFalse(math.isnan(hammer.xi))

    def test_railsback_stretch_tuning(self):
        from physics_piano.params.grand_piano import compute_railsback_cents
        # Bass A0 (21) should be negative cents
        self.assertLess(compute_railsback_cents(21), -20.0)
        # Mid C4 (60) should be 0.0 cents
        self.assertEqual(compute_railsback_cents(60), 0.0)
        # Treble C8 (108) should be positive cents
        self.assertGreater(compute_railsback_cents(108), 25.0)


class TestUPOLSConvolver(unittest.TestCase):
    def test_upols_exact_convolution(self):
        from physics_piano.dsp.upols import UPOLSConvolver
        # Test signal and impulse response
        ir = np.array([1.0, 0.5, 0.25, 0.125], dtype=np.float64)
        sig = np.array([0.8, -0.4, 0.2, 0.1, -0.05, 0.02, 0.0, 0.0], dtype=np.float64)
        direct_conv = np.convolve(sig, ir)[:len(sig)]

        convolver = UPOLSConvolver(ir, block_size=4)
        b1 = convolver.process_block(sig[:4])
        b2 = convolver.process_block(sig[4:])
        upols_conv = np.concatenate([b1[:, 0], b2[:, 0]])

        # Compare outputs
        np.testing.assert_allclose(upols_conv, direct_conv, atol=1e-10)

    def test_soundboard_ir_generation(self):
        from physics_piano.dsp.upols import generate_orthotropic_soundboard_ir
        ir_l, ir_r = generate_orthotropic_soundboard_ir(sample_rate=48000.0, duration=0.05, num_modes=20)
        self.assertEqual(len(ir_l), len(ir_r))
        self.assertGreater(np.max(np.abs(ir_l)), 0.0)


class TestClapPluginArchitecture(unittest.TestCase):
    def test_clap_event_stream_and_note_lifecycle(self):
        from physics_piano.dsp.clap_engine import ClapPianoEngine
        from physics_piano.core.events import (
            ClapNoteOnEvent, ClapNoteOffEvent, ClapNoteExpressionEvent,
            ClapNoteExpressionType, ClapParamValueEvent, ClapParamId
        )

        engine = ClapPianoEngine(sample_rate=48000.0, num_modes=15, block_size=64)
        out_events = []

        # Block 1: Note On at sample 10
        in_events_1 = [ClapNoteOnEvent(time=10, key=69, velocity=0.8)]
        block1 = engine.process(64, in_events=in_events_1, out_events=out_events)
        self.assertEqual(block1.shape, (64, 2))
        self.assertIn(69, engine.active_keys)

        # Block 2: Dynamic tuning expression (+15 cents)
        in_events_2 = [ClapNoteExpressionEvent(time=0, key=69, expression_type=ClapNoteExpressionType.TUNING, value=15.0)]
        block2 = engine.process(64, in_events=in_events_2, out_events=out_events)
        self.assertEqual(block2.shape, (64, 2))

        # Block 3: Note Off
        in_events_3 = [ClapNoteOffEvent(time=0, key=69)]
        block3 = engine.process(64, in_events=in_events_3, out_events=out_events)
        self.assertEqual(block3.shape, (64, 2))


class TestObjectiveMetricsSuite(unittest.TestCase):
    def setUp(self):
        self.synth = PianoSynth(sample_rate=48000, num_modes=20)

    def test_transient_rise_time(self):
        from physics_piano.metrics.transient import analyze_transient_onset
        audio = self.synth.render_note("A4", velocity=0.85, duration=0.2, sustain=False)
        res = analyze_transient_onset(audio, sample_rate=48000.0)
        self.assertIn("rise_time_ms", res)
        self.assertLess(res["rise_time_ms"], 15.0)

    def test_mrsl_identical_signal_zero_loss(self):
        from physics_piano.metrics.mrsl import compute_mrsl
        audio = self.synth.render_note("C4", velocity=0.8, duration=0.1, sustain=False)
        res = compute_mrsl(audio, audio, fft_sizes=[512, 1024])
        self.assertAlmostEqual(res["total_mrsl_loss"], 0.0, places=4)

    def test_octave_decay_metric(self):
        from physics_piano.metrics.octave_decay import analyze_octave_t60
        audio = self.synth.render_note("A4", velocity=0.8, duration=0.3, sustain=True)
        res = analyze_octave_t60(audio, sample_rate=48000.0)
        self.assertIn("rmse_t60", res)
        self.assertGreater(len(res["band_details"]), 0)


if __name__ == "__main__":
    unittest.main()
