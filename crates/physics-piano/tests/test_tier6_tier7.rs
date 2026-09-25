//! Verification tests for Tier 6 (Micro-Mechanical Noise) & Tier 7 (Spatial Multi-Mic & Lid Baffle).

use physics_piano::core::action::{Biquad, KeyActionNoise};
use physics_piano::core::pedal::{DamperWhoosh, PlateShock, RestrikeBuzz};
use physics_piano::dsp::lid::{LidBaffle, LidPosition};
use physics_piano::dsp::upols::{MultiPerspectiveUPOLS, StereoIR};
use physics_piano::engine::{PianoEngine, EngineEvent};

#[test]
fn test_tier6_biquad_stability_and_modes() {
    let mut bq = Biquad::bandpass(48000.0, 1000.0, 2.0);
    let mut energy = 0.0f64;
    // Impulse excitation
    let out0 = bq.process(1.0);
    energy += out0.powi(2);
    for _ in 1..4800 {
        let out = bq.process(0.0);
        energy += out.powi(2);
        assert!(!out.is_nan());
        assert!(!out.is_infinite());
    }
    // Energy must be finite and decaying
    assert!(energy > 0.0 && energy < 100.0);
}

#[test]
fn test_tier6_key_action_noise() {
    let mut action = KeyActionNoise::new(48000.0);
    // Baseline silence
    let (sil_l, sil_r) = action.step();
    assert_eq!(sil_l, 0.0);
    assert_eq!(sil_r, 0.0);

    // Strike C4 (key 60) with velocity 0.8
    action.trigger_note_on(60, 0.8);
    let mut peak = 0.0f64;
    for _ in 0..1000 {
        let (l, r) = action.step();
        assert!(!l.is_nan() && !r.is_nan());
        peak = peak.max(l.abs()).max(r.abs());
    }
    assert!(peak > 0.001, "Key strike thump and escapement must produce mechanical output");

    // Release key: triggers key-up clack
    action.trigger_note_off(60, 0.6);
    let mut clack_peak = 0.0f64;
    for _ in 0..1000 {
        let (l, r) = action.step();
        clack_peak = clack_peak.max(l.abs()).max(r.abs());
    }
    assert!(clack_peak > 0.0001, "Key release must produce back-rail felt clack");
}

#[test]
fn test_tier6_pedal_and_damper_mechanics() {
    let mut whoosh = DamperWhoosh::new(48000.0);
    let mut shock = PlateShock::new(48000.0);
    let mut buzz = RestrikeBuzz::new(48000.0);

    // 1. Damper Whoosh
    whoosh.trigger(1.2);
    let (w_l, w_r) = whoosh.step();
    assert!(w_l.abs() > 0.0 || w_r.abs() > 0.0);

    // 2. Fast pedal stomp shock
    shock.trigger(0.9);
    let (s_l, s_r) = shock.step();
    assert!(s_l.abs() > 0.0 || s_r.abs() > 0.0);

    // 3. Restrike buzz on vibrating string
    buzz.trigger(40, 0.05); // E2 with 0.05 energy
    let (b_l, b_r) = buzz.step();
    assert!(b_l.abs() > 0.0 || b_r.abs() > 0.0);
}

#[test]
fn test_tier7_lid_baffle_spectral_diffraction() {
    let mut lid = LidBaffle::new(48000.0);

    // 1. Full Stick (Flat 0dB)
    lid.set_position(LidPosition::FullStick);
    let (out_full_l, _) = lid.process(1.0, 1.0);
    assert!(!out_full_l.is_nan());

    // 2. Closed (Muffled, high frequencies attenuated)
    lid.set_position(LidPosition::Closed);
    // Measure response to high frequency pulse
    let mut closed_energy = 0.0f64;
    lid.reset();
    for i in 0..200 {
        let inp = if i % 4 < 2 { 1.0 } else { -1.0 }; // ~12 kHz square wave
        let (l, _) = lid.process(inp, inp);
        closed_energy += l.powi(2);
    }

    // Measure response with FullStick
    lid.set_position(LidPosition::FullStick);
    let mut open_energy = 0.0f64;
    lid.reset();
    for i in 0..200 {
        let inp = if i % 4 < 2 { 1.0 } else { -1.0 };
        let (l, _) = lid.process(inp, inp);
        open_energy += l.powi(2);
    }

    // Closed lid must attenuate high-frequency energy compared to open lid
    assert!(
        closed_energy < open_energy,
        "Closed lid must attenuate highs: closed={closed_energy} vs open={open_energy}"
    );
}

#[test]
fn test_tier7_multi_perspective_upols_correctness() {
    let block_size = 128;
    // FIR kernels: Close = [1.0], Player = [0.5], Ambient = [0.25]
    let close = StereoIR { left: vec![1.0; 1], right: vec![1.0; 1] };
    let player = StereoIR { left: vec![0.5; 1], right: vec![0.5; 1] };
    let ambient = StereoIR { left: vec![0.25; 1], right: vec![0.25; 1] };

    let mut upols = MultiPerspectiveUPOLS::new(&close, &player, &ambient, block_size);
    upols.close_gain = 1.0;
    upols.player_gain = 1.0;
    upols.ambient_gain = 1.0;

    let mut in_block = vec![0.0f64; block_size];
    in_block[0] = 1.0;
    let mut out_l = vec![0.0f64; block_size];
    let mut out_r = vec![0.0f64; block_size];

    upols.process_block(&in_block, &mut out_l, &mut out_r);

    // Expected mix: 1.0*1.0 + 0.5*1.0 + 0.25*1.0 = 1.75
    assert!((out_l[0] - 1.75).abs() < 1e-4, "Expected mixed output ~1.75, got {}", out_l[0]);
    assert!((out_r[0] - 1.75).abs() < 1e-4, "Expected mixed output ~1.75, got {}", out_r[0]);

    // Test fader isolation: mute player & ambient
    upols.reset();
    upols.player_gain = 0.0;
    upols.ambient_gain = 0.0;
    upols.process_block(&in_block, &mut out_l, &mut out_r);
    assert!((out_l[0] - 1.0).abs() < 1e-4);
}

#[test]
fn test_tier6_and_tier7_engine_integration() {
    let mut engine = PianoEngine::new(48000.0, 30, true);
    engine.set_radiation_mode("multi_upols");
    assert!(engine.multi_upols.is_some());

    let block_size = 128;
    let mut out_l = vec![0.0f64; block_size];
    let mut out_r = vec![0.0f64; block_size];
    let mut out_events = Vec::new();

    // NoteOn with velocity 0.85
    let events = vec![
        EngineEvent::NoteOn { time: 0, key: 60, velocity: 0.85 },
        EngineEvent::SustainPedal { time: 10, depth: 0.9 },
    ];

    let mut peak = 0.0f64;
    // Process 4 blocks (512 samples = ~10.6ms) to let string and soundboard modes evolve
    for b in 0..4 {
        let evs = if b == 0 { &events[..] } else { &[] };
        engine.process_block(block_size, evs, &mut out_events, &mut out_l, &mut out_r);
        for s in 0..block_size {
            assert!(!out_l[s].is_nan() && !out_r[s].is_nan());
            peak = peak.max(out_l[s].abs()).max(out_r[s].abs());
        }
    }
    assert!(peak > 0.005, "Integrated engine must radiate audio with multi_upols & action noise, got peak {peak}");

    // Fast pedal release to test frame shock
    let events2 = vec![
        EngineEvent::SustainPedal { time: 0, depth: 0.0 },
        EngineEvent::NoteOff { time: 20, key: 60 },
    ];
    engine.process_block(block_size, &events2, &mut out_events, &mut out_l, &mut out_r);
    for s in 0..block_size {
        assert!(!out_l[s].is_nan() && !out_r[s].is_nan());
    }
}
