//! Integration tests for physics-guitar first-principles modeling.

use physics_guitar::params::{generate_guitar_string_set, GuitarStringSetType};
use physics_guitar::core::pluck::{PluckExciter, PluckStyle};
use physics_guitar::core::guitar_string::GuitarString;
use physics_guitar::core::fretboard::FretboardRouter;
use physics_guitar::core::pickup::{MagneticPickup, PickupType, PickupPosition};
use physics_guitar::engine::{GuitarEngine, GuitarInstrumentMode};

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
fn test_pluck_dynamics_and_window_filtering() {
    let exciter_plectrum = PluckExciter::new(PluckStyle::Plectrum);
    let exciter_finger = PluckExciter::new(PluckStyle::FingerFlesh);

    let (q_t_plec, _) = exciter_plectrum.compute_initial_modal_displacements(0.648, 0.15, 0.8, 30);
    let (q_t_finger, _) = exciter_finger.compute_initial_modal_displacements(0.648, 0.15, 0.8, 30);

    assert_eq!(q_t_plec.len(), 30);
    assert_eq!(q_t_finger.len(), 30);

    // High frequency modes (e.g. mode 8 ~ 2.6 kHz) should have much higher relative energy with plectrum than soft finger
    let plec_ratio = (q_t_plec[7] / q_t_plec[0]).abs();
    let finger_ratio = (q_t_finger[7] / q_t_finger[0]).abs();
    assert!(plec_ratio > finger_ratio, "Plectrum should generate brighter higher-order harmonics than finger");
}

#[test]
fn test_fretboard_routing_and_mpe() {
    let router = FretboardRouter::new();
    let strings_held = [false; 6];

    // Note 40 (E2) should map to String 6, fret 0
    let loc_e2 = router.allocate_note(40, &strings_held).expect("Should allocate E2");
    assert_eq!(loc_e2.string_index, 6);
    assert_eq!(loc_e2.fret, 0);

    // Note 60 (C4) can be played on String 2 (fret 1)
    let loc_c4 = router.allocate_note(60, &strings_held).expect("Should allocate C4");
    assert_eq!(loc_c4.string_index, 2);
    assert_eq!(loc_c4.fret, 1);

    // MPE Routing: Channel 2 is String 1, Channel 7 is String 6
    let mpe_loc = router.allocate_mpe_note(2, 64).expect("MPE Channel 2 should allocate String 1");
    assert_eq!(mpe_loc.string_index, 1);
    assert_eq!(mpe_loc.fret, 0);

    let mpe_loc_s6 = router.allocate_mpe_note(7, 45).expect("MPE Channel 7 should allocate String 6");
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

    let pu_single = MagneticPickup::new(PickupType::SingleCoil, PickupPosition::Bridge);
    let pu_humbucker = MagneticPickup::new(PickupType::Humbucker, PickupPosition::Bridge);

    let sig_single = pu_single.sample_string(&string);
    let sig_hum = pu_humbucker.sample_string(&string);

    assert!(sig_single.abs() > 0.0, "Single coil pickup signal must be non-zero");
    assert!(sig_hum.abs() > 0.0, "Humbucker pickup signal must be non-zero");
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

    assert!(decay_ratio_mute < decay_ratio_open, "Palm mute must accelerate string energy decay");
}

#[test]
fn test_guitar_engine_rendering_stability() {
    let mut engine_elec = GuitarEngine::new(44100.0, GuitarStringSetType::Electric010, GuitarInstrumentMode::Electric);
    let mut engine_acous = GuitarEngine::new(44100.0, GuitarStringSetType::Acoustic012, GuitarInstrumentMode::Acoustic);

    // Trigger open E2 note
    engine_elec.note_on(1, 40, 0.9);
    engine_acous.note_on(1, 40, 0.9);

    let mut left = [0.0f32; 256];
    let mut right = [0.0f32; 256];

    for _ in 0..10 {
        engine_elec.process_block(&mut left, &mut right);
        for &s in left.iter().chain(right.iter()) {
            assert!(!s.is_nan() && !s.is_infinite(), "Electric engine output must be finite");
        }

        engine_acous.process_block(&mut left, &mut right);
        for &s in left.iter().chain(right.iter()) {
            assert!(!s.is_nan() && !s.is_infinite(), "Acoustic engine output must be finite");
        }
    }
}
