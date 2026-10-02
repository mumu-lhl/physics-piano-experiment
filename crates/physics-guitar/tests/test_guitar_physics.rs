//! Integration tests for physics-guitar first-principles modeling.

#[cfg(debug_assertions)]
use nice_assert_no_alloc::{assert_no_alloc, violation_count};
use physics_guitar::core::amp_cab::CabinetModel;
use physics_guitar::core::fretboard::FretboardRouter;
use physics_guitar::core::guitar_string::GuitarString;
use physics_guitar::core::pickup::{MagneticPickup, PickupPosition, PickupSelector, PickupType};
use physics_guitar::core::pluck::{PluckExciter, PluckStyle};
use physics_guitar::engine::{GuitarEngine, GuitarInstrumentMode};
use physics_guitar::params::{GuitarStringSetType, generate_guitar_string_set};

#[test]
fn test_guitar_string_set_parameters() {
    let electric = generate_guitar_string_set(GuitarStringSetType::Electric010, 30);
    assert_eq!(electric.len(), 6);

    // String 1 is high E (329.63 Hz), String 6 is low E (82.41 Hz)
    assert_eq!(electric[0].string_index, 1);
    assert_eq!(electric[0].open_midi_note, 64);
    assert!((electric[0].open_f0 - 329.63).abs() < 0.1);

    assert_eq!(electric[5].string_index, 6);
    assert_eq!(electric[5].open_midi_note, 40);
    assert!((electric[5].open_f0 - 82.41).abs() < 0.1);

    // 12th fret halves the vibrating length and doubles the frequency
    let l_fret12 = electric[0].effective_length_at_fret(12);
    assert!((l_fret12 - electric[0].scale_length / 2.0).abs() < 1e-4);
    let f0_fret12 = electric[0].frequency_at_fret(12);
    assert!((f0_fret12 - electric[0].open_f0 * 2.0).abs() < 0.1);
}

#[test]
fn test_modal_fundamentals_follow_standard_tuning_across_modes_and_frets() {
    for set_type in [
        GuitarStringSetType::Electric010,
        GuitarStringSetType::Acoustic012,
    ] {
        for params in generate_guitar_string_set(set_type, 30) {
            let mut string = GuitarString::new(params.clone(), 44100.0);
            for fret in [0, 1, 5, 12, 24] {
                string.set_fret(fret);
                let expected_hz = params.frequency_at_fret(fret);
                let actual_hz = string.omega_t[0] / (2.0 * std::f64::consts::PI);
                let cents = 1200.0 * (actual_hz / expected_hz).log2();
                assert!(
                    cents.abs() < 0.5,
                    "{set_type:?} string {} fret {fret} is {cents:.2} cents from tuning",
                    params.string_index
                );
            }

            string.set_fret(5);
            string.set_pitch_bend(2.0);
            let expected_hz = params.frequency_at_fret(5) * 2.0_f64.powf(2.0 / 12.0);
            let actual_hz = string.omega_t[0] / (2.0 * std::f64::consts::PI);
            let cents = 1200.0 * (actual_hz / expected_hz).log2();
            assert!(
                cents.abs() < 0.5,
                "{set_type:?} string {} pitch bend is {cents:.2} cents from target",
                params.string_index
            );
        }
    }
}

#[test]
fn test_pluck_dynamics_and_window_filtering() {
    let exciter_plectrum = PluckExciter::new(PluckStyle::Plectrum);
    let exciter_finger = PluckExciter::new(PluckStyle::FingerFlesh);

    let (q_t_plec, _, _) =
        exciter_plectrum.compute_initial_modal_displacements(0.648, 329.63, 0.15, 0.8, 30);
    let (q_t_finger, _, _) =
        exciter_finger.compute_initial_modal_displacements(0.648, 329.63, 0.15, 0.8, 30);

    assert_eq!(q_t_plec.len(), 30);
    assert_eq!(q_t_finger.len(), 30);

    // High frequency modes (e.g. mode 8 ~ 2.6 kHz) should have much higher relative energy with plectrum than soft finger
    let plec_ratio = (q_t_plec[7] / q_t_plec[0]).abs();
    let finger_ratio = (q_t_finger[7] / q_t_finger[0]).abs();
    assert!(
        plec_ratio > finger_ratio,
        "Plectrum should generate brighter higher-order harmonics than finger"
    );
}

#[test]
fn test_fretboard_routing_and_mpe() {
    let mut router = FretboardRouter::new();
    let strings_held = [false; 6];

    // Note 40 (E2) should map to String 6, fret 0
    let dummy_frets = [None; 6];
    let loc_e2 = router
        .allocate_note(40, &strings_held, &dummy_frets)
        .expect("Should allocate E2");
    assert_eq!(loc_e2.string_index, 6);
    assert_eq!(loc_e2.fret, 0);

    // Note 60 (C4) can be played on String 2 (fret 1)
    let loc_c4 = router
        .allocate_note(60, &strings_held, &dummy_frets)
        .expect("Should allocate C4");
    assert_eq!(loc_c4.string_index, 2);
    assert_eq!(loc_c4.fret, 1);

    // MPE Routing: Channel 2 is String 1, Channel 7 is String 6
    let mpe_loc = router
        .allocate_mpe_note(2, 64)
        .expect("MPE Channel 2 should allocate String 1");
    assert_eq!(mpe_loc.string_index, 1);
    assert_eq!(mpe_loc.fret, 0);

    let mpe_loc_s6 = router
        .allocate_mpe_note(7, 45)
        .expect("MPE Channel 7 should allocate String 6");
    assert_eq!(mpe_loc_s6.string_index, 6);
    assert_eq!(mpe_loc_s6.fret, 5); // E2 + 5 semitones = A2
}

#[test]
fn test_magnetic_pickup_single_vs_humbucker() {
    let strings = generate_guitar_string_set(GuitarStringSetType::Electric010, 30);
    let mut string = GuitarString::new(strings[0].clone(), 44100.0);
    let exciter = PluckExciter::new(PluckStyle::Plectrum);
    string.pluck(&exciter, 0.15, 0.8);
    // Step string by 2 samples to develop velocity
    string.step();
    string.step();

    let mut pu_single =
        MagneticPickup::new(PickupType::SingleCoil, PickupPosition::Bridge, 44100.0);
    let mut pu_humbucker =
        MagneticPickup::new(PickupType::Humbucker, PickupPosition::Bridge, 44100.0);

    let sig_single = pu_single.sample_string(&string);
    let sig_hum = pu_humbucker.sample_string(&string);

    assert!(
        sig_single.abs() > 0.0,
        "Single coil pickup signal must be non-zero"
    );
    assert!(
        sig_hum.abs() > 0.0,
        "Humbucker pickup signal must be non-zero"
    );
}

#[test]
fn test_palm_mute_decay_acceleration() {
    let strings = generate_guitar_string_set(GuitarStringSetType::Electric010, 30);
    let exciter = PluckExciter::new(PluckStyle::Plectrum);

    // String without palm mute
    let mut s_open = GuitarString::new(strings[5].clone(), 44100.0);
    s_open.pluck(&exciter, 0.15, 0.8);

    // String with heavy palm mute
    let mut s_mute = GuitarString::new(strings[5].clone(), 44100.0);
    s_mute.palm_mute_depth = 1.0;
    s_mute.recalculate_modal_operators();
    s_mute.pluck(&exciter, 0.15, 0.8);

    let initial_energy_open = s_open.total_energy();
    let initial_energy_mute = s_mute.total_energy();
    assert!(initial_energy_open > 0.0);
    assert!(initial_energy_mute > 0.0);

    // Step 1000 samples (~22ms)
    for _ in 0..1000 {
        s_open.step();
        s_mute.step();
    }

    let decay_ratio_open = s_open.total_energy() / initial_energy_open;
    let decay_ratio_mute = s_mute.total_energy() / initial_energy_mute;

    assert!(
        decay_ratio_mute < decay_ratio_open,
        "Palm mute must accelerate string energy decay"
    );
}

#[test]
fn test_guitar_engine_rendering_stability() {
    let mut engine_elec = GuitarEngine::new(
        44100.0,
        GuitarStringSetType::Electric010,
        GuitarInstrumentMode::Electric,
    );
    let mut engine_acous = GuitarEngine::new(
        44100.0,
        GuitarStringSetType::Acoustic012,
        GuitarInstrumentMode::Acoustic,
    );

    // Trigger open E2 note
    engine_elec.note_on(1, 40, 0.9);
    engine_acous.note_on(1, 40, 0.9);

    let mut left = [0.0f32; 256];
    let mut right = [0.0f32; 256];

    for _ in 0..10 {
        engine_elec.process_block(&mut left, &mut right);
        for &s in left.iter().chain(right.iter()) {
            assert!(
                !s.is_nan() && !s.is_infinite(),
                "Electric engine output must be finite"
            );
        }

        engine_acous.process_block(&mut left, &mut right);
        for &s in left.iter().chain(right.iter()) {
            assert!(
                !s.is_nan() && !s.is_infinite(),
                "Acoustic engine output must be finite"
            );
        }
    }
}

#[test]
fn test_default_guitar_output_stays_below_digital_full_scale() {
    for (mode, set_type) in [
        (
            GuitarInstrumentMode::Electric,
            GuitarStringSetType::Electric010,
        ),
        (
            GuitarInstrumentMode::Acoustic,
            GuitarStringSetType::Acoustic012,
        ),
    ] {
        let mut engine = GuitarEngine::new(44100.0, set_type, mode);
        engine.strummer.set_strum_speed_ms(0.0);
        engine.note_on(1, 40, 1.0);

        let mut peak = 0.0_f64;
        for _ in 0..44100 {
            let (left, right) = engine.process_sample();
            peak = peak.max(left.abs()).max(right.abs());
        }

        assert!(
            peak <= 1.0,
            "{mode:?} default output exceeded 0 dBFS: peak={peak}"
        );
    }
}

#[test]
fn test_smart_strummer_chord_stagger() {
    use physics_guitar::core::strummer::{SmartStrummer, StrumDirection};

    let mut strummer = SmartStrummer::new(44100.0);
    strummer.strum_speed_ms = 20.0; // 20ms total stroke
    strummer.direction = StrumDirection::Down;

    // Chord: strings 5 (low E), 4 (A), 3 (D)
    let chord = [(5, 0, 0.8), (4, 2, 0.8), (3, 2, 0.8)];
    strummer.trigger_chord(&chord);

    assert_eq!(strummer.pending_plucks.len(), 3);
    // Down stroke: string 5 has 0 delay, string 4 intermediate, string 3 largest delay
    assert_eq!(strummer.pending_plucks[0].string_index, 5);
    assert_eq!(strummer.pending_plucks[0].delay_samples, 0);

    assert_eq!(strummer.pending_plucks[1].string_index, 4);
    assert!(strummer.pending_plucks[1].delay_samples > 0);

    assert_eq!(strummer.pending_plucks[2].string_index, 3);
    assert!(strummer.pending_plucks[2].delay_samples > strummer.pending_plucks[1].delay_samples);
}

#[test]
fn test_tube_amp_and_cabinet_saturation() {
    use physics_guitar::core::amp_cab::GuitarAmpCab;

    let mut amp = GuitarAmpCab::new(44100.0);
    amp.drive = 0.8; // High drive
    amp.is_enabled = true;
    amp.cab_enabled = true;

    // Pass high amplitude sine wave
    let mut out_max = 0.0f64;
    for n in 0..1000 {
        let input = 2.5 * (2.0 * std::f64::consts::PI * 440.0 * n as f64 / 44100.0).sin();
        let out = amp.process(input);
        assert!(!out.is_nan() && !out.is_infinite());
        out_max = out_max.max(out.abs());
    }

    // Output must be saturated and bounded
    assert!(out_max > 0.0 && out_max < 3.0);
}

#[test]
fn test_christensen_3dof_acoustic_body() {
    use physics_guitar::core::body::AcousticGuitarBody;

    let mut body = AcousticGuitarBody::new(44100.0);
    // Pulse input
    let out_impulse = body.process(10.0);
    assert!(!out_impulse.is_nan() && out_impulse.abs() > 0.0);

    // Ringing decay
    let mut ring_samples = 0;
    for _ in 0..2000 {
        let out = body.process(0.0);
        assert!(!out.is_nan());
        if out.abs() > 1e-4 {
            ring_samples += 1;
        }
    }
    assert!(
        ring_samples > 100,
        "Acoustic body must sustain resonant ring"
    );
}

#[test]
fn test_acoustic_two_stage_decay() {
    let strings = generate_guitar_string_set(GuitarStringSetType::Acoustic012, 30);
    let mut string = GuitarString::new(strings[0].clone(), 44100.0);
    let exciter = PluckExciter::new(PluckStyle::Plectrum);
    string.pluck(&exciter, 0.15, 0.8);

    // Initial energy in vertical vs horizontal planes
    let mut energy_t_init = 0.0;
    let mut energy_p_init = 0.0;
    for m in 0..string.num_modes {
        energy_t_init += 0.5
            * (string.state_t[m].v.powi(2)
                + string.omega_t[m].powi(2) * string.state_t[m].q.powi(2));
        energy_p_init += 0.5
            * (string.state_p[m].v.powi(2)
                + string.omega_p[m].powi(2) * string.state_p[m].q.powi(2));
    }

    // Step 15000 samples (~340ms)
    for _ in 0..15000 {
        string.step();
    }

    let mut energy_t_later = 0.0;
    let mut energy_p_later = 0.0;
    for m in 0..string.num_modes {
        energy_t_later += 0.5
            * (string.state_t[m].v.powi(2)
                + string.omega_t[m].powi(2) * string.state_t[m].q.powi(2));
        energy_p_later += 0.5
            * (string.state_p[m].v.powi(2)
                + string.omega_p[m].powi(2) * string.state_p[m].q.powi(2));
    }

    let ratio_t = energy_t_later / energy_t_init;
    let ratio_p = energy_p_later / energy_p_init;

    // Horizontal polarization P must decay significantly slower than vertical polarization T
    assert!(
        ratio_p > ratio_t * 1.5,
        "Horizontal polarization must sustain longer than vertical (two-stage decay)"
    );
}

#[test]
fn test_sympathetic_resonance_coupling() {
    let strings = generate_guitar_string_set(GuitarStringSetType::Acoustic012, 30);
    let mut s_open = GuitarString::new(strings[0].clone(), 44100.0); // High E
    assert_eq!(s_open.total_energy(), 0.0);

    // Inject bridge vibration at 329.63 Hz matching String 1 fundamental
    let f0 = 329.63;
    for n in 0..2000 {
        let v_bridge = 0.05 * (2.0 * std::f64::consts::PI * f0 * (n as f64) / 44100.0).sin();
        s_open.inject_bridge_motion(v_bridge, 0.0003);
        s_open.step();
    }

    // Open string must build up energy via sympathetic resonance
    assert!(
        s_open.total_energy() > 1e-12,
        "Sympathetic resonance must excite tuned open string"
    );
}

#[test]
fn test_dynamic_tension_modulation() {
    let strings = generate_guitar_string_set(GuitarStringSetType::Acoustic012, 30);
    let mut string = GuitarString::new(strings[5].clone(), 44100.0); // Low E
    let exciter = PluckExciter::new(PluckStyle::Plectrum);
    string.pluck(&exciter, 0.15, 1.0); // Hard pluck

    string.step();
    // Dynamic tension delta_T must be positive on hard attack
    assert!(
        string.current_delta_t > 0.0,
        "Hard pluck must induce dynamic geometric tension increase"
    );
}

#[test]
fn test_acoustic_stereo_spatial_radiation() {
    let mut engine = GuitarEngine::new(
        44100.0,
        GuitarStringSetType::Acoustic012,
        GuitarInstrumentMode::Acoustic,
    );
    engine.note_on(1, 40, 0.9); // Low E2

    let mut left = [0.0f32; 1024];
    let mut right = [0.0f32; 1024];
    engine.process_block(&mut left, &mut right);

    // Both channels must have energy, but not be bit-for-bit identical (must have stereo width)
    let sum_l: f32 = left.iter().map(|s| s.abs()).sum();
    let sum_r: f32 = right.iter().map(|s| s.abs()).sum();
    assert!(sum_l > 0.0 && sum_r > 0.0);

    let diff_sq: f32 = left
        .iter()
        .zip(right.iter())
        .map(|(l, r)| (l - r).powi(2))
        .sum();
    assert!(
        diff_sq > 1e-6,
        "Acoustic body must produce natural spatial stereo image (not dead mono)"
    );
}

#[test]
fn test_single_channel_pitch_bend_and_pre_bend() {
    let mut engine = GuitarEngine::new(
        44100.0,
        GuitarStringSetType::Acoustic012,
        GuitarInstrumentMode::Acoustic,
    );

    // 1. Send pitch bend on standard MIDI channel 1 (Channel 1, +2.0 semitones) BEFORE playing
    engine.pitch_bend(1, 2.0);
    assert_eq!(engine.global_pitch_bend, 2.0);

    // 2. Play note on Channel 1 (Low E2 -> String 6)
    engine.note_on(1, 40, 0.8);

    // String 6 (index 5) must have pitch bend applied (+2 semitones)
    assert!((engine.strings[5].pitch_bend_semitones - 2.0).abs() < 1e-4);

    // 3. Modulate bend while sounding to +4.0 semitones
    engine.pitch_bend(1, 4.0);
    assert!((engine.strings[5].pitch_bend_semitones - 4.0).abs() < 1e-4);
}

#[cfg(debug_assertions)]
#[test]
fn note_on_does_not_allocate_after_engine_initialization() {
    let mut engine = GuitarEngine::new(
        48_000.0,
        GuitarStringSetType::Electric010,
        GuitarInstrumentMode::Electric,
    );
    let violations_before = violation_count();

    assert_no_alloc(|| {
        engine.note_on(1, 64, 0.85);
        for _ in 0..400 {
            engine.process_sample();
        }
    });

    assert_eq!(
        violation_count(),
        violations_before,
        "guitar note-on must not allocate on the audio thread"
    );
}

#[test]
fn test_gui_dynamic_velocity_and_keyboard_event_routing() {
    let mut engine = GuitarEngine::new(
        44100.0,
        GuitarStringSetType::Electric010,
        GuitarInstrumentMode::Electric,
    );
    let exciter = PluckExciter::new(PluckStyle::Plectrum);

    // 1. Dynamic velocity alters pluck amplitude and energy
    let mut string_soft = engine.strings[0].clone();
    let mut string_hard = engine.strings[0].clone();
    string_soft.pluck(&exciter, 0.7, 0.35);
    string_hard.pluck(&exciter, 0.7, 0.95);
    assert!(
        string_hard.total_energy() > string_soft.total_energy() * 2.0,
        "Hard pluck (vel=0.95) must produce significantly more energy than soft pluck (vel=0.35)"
    );

    // 2. MidiNoteOn / MidiNoteOff routing
    engine.note_on(0, 60, 0.85);
    assert!(
        engine.active_notes_on_string.contains(&Some(60)),
        "MidiNoteOn for 60 must be routed to active note on fretboard"
    );

    engine.note_off(0, 60);
    assert!(
        !engine.active_notes_on_string.contains(&Some(60)),
        "MidiNoteOff for 60 must release note"
    );
}

#[test]
fn test_guitar_repeat_note_on_string_reuse_and_energy_damping() {
    let mut engine = GuitarEngine::new(
        44100.0,
        GuitarStringSetType::Electric010,
        GuitarInstrumentMode::Electric,
    );

    // Initial note on C4 (MIDI 60)
    engine.note_on(0, 60, 0.8);
    let first_string_idx = engine
        .active_notes_on_string
        .iter()
        .position(|&n| n == Some(60))
        .expect("Note 60 should be active on a string");

    // Repeated note_on for same note (e.g. key repeat) should reuse the SAME string rather than allocating a new one
    engine.note_on(0, 60, 0.8);
    let second_string_idx = engine
        .active_notes_on_string
        .iter()
        .position(|&n| n == Some(60))
        .expect("Note 60 should still be active");
    assert_eq!(
        first_string_idx, second_string_idx,
        "Repeated NoteOn must reuse active string"
    );

    // Count how many strings are playing note 60: exactly 1
    let active_count = engine
        .active_notes_on_string
        .iter()
        .filter(|&&n| n == Some(60))
        .count();
    assert_eq!(active_count, 1, "Only one string should hold note 60");

    // Verify re-pluck damping prevents energy explosion
    let exciter = PluckExciter::new(PluckStyle::Plectrum);
    let mut string = engine.strings[first_string_idx].clone();
    string.pluck(&exciter, 0.7, 0.9);
    let energy1 = string.total_energy();

    // 10 repeated rapid plucks without delay
    for _ in 0..10 {
        string.pluck(&exciter, 0.7, 0.9);
    }
    let energy_accum = string.total_energy();
    assert!(
        energy_accum < energy1 * 5.0,
        "Repeated rapid plucks must be physically damped by plectrum contact, avoiding energy runaway (single={energy1}, 10x={energy_accum})"
    );
}

#[test]
fn test_multi_stage_tube_amp_and_tone_stack() {
    use physics_guitar::core::amp_cab::GuitarAmpCab;

    let mut amp = GuitarAmpCab::new(44100.0);
    amp.is_enabled = true;
    amp.cab_enabled = true;
    amp.set_drive(0.7);
    amp.set_tone_stack(0.8, 0.4, 0.7, 0.6);

    assert_eq!(amp.bass, 0.8);
    assert_eq!(amp.middle, 0.4);
    assert_eq!(amp.treble, 0.7);

    // Pass test signal
    let mut max_val = 0.0_f64;
    for n in 0..1000 {
        let input = 1.5 * (2.0 * std::f64::consts::PI * 220.0 * n as f64 / 44100.0).sin();
        let out = amp.process(input);
        assert!(!out.is_nan() && !out.is_infinite());
        max_val = max_val.max(out.abs());
    }
    assert!(max_val > 0.0 && max_val < 3.0);
}

#[test]
fn test_acoustic_high_frequency_air_and_cavity_dispersion() {
    use physics_guitar::core::body::AcousticGuitarBody;

    let mut body = AcousticGuitarBody::new(44100.0);
    // Test that high frequency content (e.g. 7 kHz sheen) transmits through air damping
    let (l, r) = body.process_stereo(1.0);
    assert!(!l.is_nan() && !r.is_nan());
    assert!(l.abs() > 0.0 && r.abs() > 0.0);
}

#[test]
fn test_guitar_physical_legato_modal_projection_and_energy_conservation() {
    let engine = GuitarEngine::new(
        44100.0,
        GuitarStringSetType::Electric010,
        GuitarInstrumentMode::Electric,
    );
    let exciter = PluckExciter::new(PluckStyle::Plectrum);

    // 1. Initial pluck at fret 2 (e.g. string 2, A string, fret 2 = B1)
    let mut string = engine.strings[1].clone();
    string.set_fret(2);
    string.pluck(&exciter, 0.7, 0.85);

    // Let it vibrate for 500 samples
    for _ in 0..500 {
        string.step();
    }
    let energy_before = string.total_energy();
    assert!(
        energy_before > 1e-6,
        "String must possess vibrational energy before legato"
    );

    // 2. Hammer-on to fret 5 (L decreases)
    string.legato_fret(5, 0.80);
    assert_eq!(string.current_fret, 5);
    let energy_after_hammer = string.total_energy();
    // Modal projection must preserve stored energy and inject hammer impulse (not collapse to zero)
    assert!(
        energy_after_hammer > energy_before * 0.5,
        "Hammer-on modal projection must preserve energy (before={energy_before}, after={energy_after_hammer})"
    );
    assert!(string.is_held);

    // Let it vibrate at fret 5
    for _ in 0..500 {
        string.step();
    }
    let energy_fret5 = string.total_energy();

    // 3. Pull-off back to fret 2 (L increases)
    string.legato_fret(2, 0.75);
    assert_eq!(string.current_fret, 2);
    let energy_after_pulloff = string.total_energy();
    assert!(
        energy_after_pulloff > energy_fret5 * 0.4,
        "Pull-off modal projection must preserve energy (before={energy_fret5}, after={energy_after_pulloff})"
    );
}

#[test]
fn test_guitar_distributed_viscoelastic_palm_muting() {
    let params = physics_guitar::params::guitar_tuning::generate_guitar_string_set(
        physics_guitar::params::guitar_tuning::GuitarStringSetType::Electric010,
        24,
    )[0]
    .clone();
    let mut string_open = GuitarString::new(params, 44100.0);
    let mut string_muted = string_open.clone();
    string_muted.palm_mute_depth = 0.9;
    string_muted.recalculate_modal_operators();

    let exciter = PluckExciter::new(PluckStyle::Plectrum);
    string_open.pluck(&exciter, 0.2, 0.9);
    string_muted.pluck(&exciter, 0.2, 0.9);

    // Evolve 4000 samples (~90ms)
    for _ in 0..4000 {
        string_open.step();
        string_muted.step();
    }

    // High modes (m >= 8) under viscoelastic palm mute must decay much faster than fundamental
    let muted_fund_amp = string_muted.state_t[0].q.abs();
    let muted_high_amp = string_muted.state_t[7].q.abs();
    let open_high_amp = string_open.state_t[7].q.abs();

    assert!(
        muted_fund_amp > 1e-7,
        "Fundamental thump should be preserved under bridge palm mute"
    );
    assert!(
        muted_high_amp < open_high_amp * 0.15,
        "High modes under viscoelastic palm muting must decay dramatically faster than open string (muted={muted_high_amp}, open={open_high_amp})"
    );
}

#[test]
fn test_guitar_comprehensive_harmonics() {
    let params = physics_guitar::params::guitar_tuning::generate_guitar_string_set(
        physics_guitar::params::guitar_tuning::GuitarStringSetType::Electric010,
        24,
    )[0]
    .clone();
    let mut string = GuitarString::new(params, 44100.0);
    let exciter = PluckExciter::new(PluckStyle::Plectrum);

    // 1. Natural harmonic at 12th fret (Node 2 = octave): fundamental mode 1 should be damped, mode 2 active
    string.trigger_natural_harmonic(&exciter, 2, 0.85);
    let mode1 = string.state_t[0].q.abs();
    let mode2 = string.state_t[1].q.abs();
    assert!(
        mode2 > mode1 * 4.0,
        "12th fret natural harmonic must suppress fundamental in favor of 2nd harmonic (m1={mode1}, m2={mode2})"
    );

    // 2. Pinch harmonic: fundamental (modes 1 & 2) suppressed, upper screaming harmonics active
    let mut string_pinch = GuitarString::new(
        physics_guitar::params::guitar_tuning::generate_guitar_string_set(
            physics_guitar::params::guitar_tuning::GuitarStringSetType::Electric010,
            24,
        )[0]
        .clone(),
        44100.0,
    );
    string_pinch.trigger_pinch_harmonic(&exciter, 0.20, 0.90);
    let pinch_low = string_pinch.state_t[0].q.abs() + string_pinch.state_t[1].q.abs();
    let pinch_high = string_pinch.state_t[2].q.abs() + string_pinch.state_t[3].q.abs();
    assert!(
        pinch_high > pinch_low * 3.0,
        "Pinch harmonic must suppress low fundamental in favor of high harmonics (low={pinch_low}, high={pinch_high})"
    );
}

#[test]
fn test_guitar_continuous_legato_slide() {
    let params = physics_guitar::params::guitar_tuning::generate_guitar_string_set(
        physics_guitar::params::guitar_tuning::GuitarStringSetType::Electric010,
        24,
    )[0]
    .clone();
    let mut string = GuitarString::new(params, 44100.0);
    let exciter = PluckExciter::new(PluckStyle::Plectrum);
    string.set_fret(2);
    string.pluck(&exciter, 0.5, 0.85);

    // Slide from fret 2 to fret 7 over 40ms (~1764 samples)
    string.start_slide(7, 40.0);
    assert!(string.slide_active);

    for _ in 0..1800 {
        string.step();
    }

    assert!(!string.slide_active, "Slide must complete after duration");
    assert_eq!(
        string.current_fret, 7,
        "Fret should be target fret after slide"
    );
    assert!(
        string.total_energy() > 1e-7,
        "String should remain vibrating during and after slide"
    );
}

#[test]
fn test_guitar_acoustic_feedback_loop_sustain() {
    let mut engine_dry = GuitarEngine::new(
        44100.0,
        GuitarStringSetType::Electric010,
        GuitarInstrumentMode::Electric,
    );
    let mut engine_fb = engine_dry.clone();

    // Enable high-gain acoustic feedback singing loop
    engine_fb.set_acoustic_feedback(0.92, 4.2);
    engine_fb.amp_cab.set_drive(0.85);
    engine_dry.amp_cab.set_drive(0.85);

    // Play note on G string
    engine_dry.note_on(1, 67, 0.9);
    engine_fb.note_on(1, 67, 0.9);

    // Evolve 18,000 samples (~0.4s)
    for _ in 0..18_000 {
        engine_dry.process_sample();
        engine_fb.process_sample();
    }

    let energy_dry: f64 = engine_dry.strings.iter().map(|s| s.total_energy()).sum();
    let energy_fb: f64 = engine_fb.strings.iter().map(|s| s.total_energy()).sum();

    assert!(
        energy_fb > energy_dry * 1.5,
        "Acoustic feedback closed loop must sustain string vibration energy through air coupling (fb={energy_fb}, dry={energy_dry})"
    );
}

#[test]
fn test_guitar_stratocaster_5way_and_rwrp_quack() {
    let mut engine = GuitarEngine::new(
        44100.0,
        GuitarStringSetType::Electric010,
        GuitarInstrumentMode::Electric,
    );

    // Test position 4 (Neck + Middle)
    engine.pickup.selector = PickupSelector::NeckAndMiddle;
    engine.note_on(1, 64, 0.85);

    let mut signal_max = 0.0_f64;
    for _ in 0..600 {
        let (l, r) = engine.process_sample();
        signal_max = signal_max.max(l.abs()).max(r.abs());
    }
    assert!(
        signal_max > 1e-4,
        "Neck + Middle position must produce clear audible audio"
    );

    // Test RWRP quack toggle
    engine.set_rwrp_quack(false);
    assert!(!engine.pickup.rwrp_quack);
    engine.set_rwrp_quack(true);
    assert!(engine.pickup.rwrp_quack);
}

#[test]
fn test_guitar_cabinet_models_and_mic_proximity() {
    let mut engine = GuitarEngine::new(
        44100.0,
        GuitarStringSetType::Electric010,
        GuitarInstrumentMode::Electric,
    );

    // Test model switching
    engine.set_cabinet_model(CabinetModel::TwinReverb);
    assert_eq!(engine.amp_cab.cabinet.model, CabinetModel::TwinReverb);

    engine.set_cabinet_model(CabinetModel::Greenback);
    assert_eq!(engine.amp_cab.cabinet.model, CabinetModel::Greenback);

    engine.set_cabinet_model(CabinetModel::Vintage30);
    assert_eq!(engine.amp_cab.cabinet.model, CabinetModel::Vintage30);

    // Test mic proximity effect
    engine.set_mic_distance(0.0); // Close mic
    assert_eq!(engine.amp_cab.cabinet.mic_distance_cm, 0.0);

    engine.set_mic_distance(25.0); // Distant mic
    assert_eq!(engine.amp_cab.cabinet.mic_distance_cm, 25.0);

    // Render audio through cabinet
    engine.note_on(1, 60, 0.8);
    let mut max_abs = 0.0_f64;
    for _ in 0..600 {
        let (l, r) = engine.process_sample();
        max_abs = max_abs.max(l.abs()).max(r.abs());
    }
    assert!(
        max_abs > 1e-4,
        "Cabinet processing must output valid non-zero audio"
    );
}


