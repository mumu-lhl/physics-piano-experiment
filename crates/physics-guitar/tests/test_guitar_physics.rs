//! Integration tests for physics-guitar first-principles modeling.

#[cfg(debug_assertions)]
use nice_assert_no_alloc::{assert_no_alloc, violation_count};
use physics_guitar::core::fretboard::FretboardRouter;
use physics_guitar::core::guitar_string::GuitarString;
use physics_guitar::core::pickup::{MagneticPickup, PickupPosition, PickupType};
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
