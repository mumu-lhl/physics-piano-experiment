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
    // Process 40 blocks (~0.1s) to let string and soundboard modes evolve
    for b in 0..40 {
        let evs = if b == 0 { &events[..] } else { &[] };
        engine.process_block(block_size, evs, &mut out_events, &mut out_l, &mut out_r);
        for s in 0..block_size {
            assert!(!out_l[s].is_nan() && !out_r[s].is_nan());
            peak = peak.max(out_l[s].abs()).max(out_r[s].abs());
        }
    }
    println!("PEAK AMPLITUDE WITH MULTI_UPOLS: {}", peak);
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

#[test]
fn test_stereo_spatial_soundstage_bass_vs_treble() {
    // 1. Bass note A0 (key 21) - should be panned towards Left
    let mut engine_bass = PianoEngine::new(48000.0, 30, false);
    engine_bass.set_pedal_noise_gain(0.0);
    engine_bass.set_key_noise_gain(0.0);
    engine_bass.set_damper_noise_gain(0.0);
    engine_bass.note_on(21, 0.8);

    let (bass_l, bass_r) = engine_bass.render(0.3);
    let bass_energy_l: f64 = bass_l.iter().map(|x| x * x).sum();
    let bass_energy_r: f64 = bass_r.iter().map(|x| x * x).sum();

    println!("Bass A0 energy L: {bass_energy_l:.6}, R: {bass_energy_r:.6}");
    assert!(bass_energy_l > bass_energy_r * 1.5, "Bass notes must have significantly more Left channel energy than Right");

    // 2. Treble note C8 (key 108) - should be panned towards Right
    let mut engine_treble = PianoEngine::new(48000.0, 30, false);
    engine_treble.set_pedal_noise_gain(0.0);
    engine_treble.set_key_noise_gain(0.0);
    engine_treble.set_damper_noise_gain(0.0);
    engine_treble.note_on(108, 0.8);

    let (treble_l, treble_r) = engine_treble.render(0.3);
    let treble_energy_l: f64 = treble_l.iter().map(|x| x * x).sum();
    let treble_energy_r: f64 = treble_r.iter().map(|x| x * x).sum();

    println!("Treble C8 energy L: {treble_energy_l:.6}, R: {treble_energy_r:.6}");
    assert!(treble_energy_r > treble_energy_l * 1.5, "Treble notes must have significantly more Right channel energy than Left");
}

#[test]
fn test_voicing_parameters_effect() {
    // Test Hammer Hardness: soft vs hard hammer must produce noticeably different timbre/energy
    let mut engine_soft = PianoEngine::new(48000.0, 30, false);
    engine_soft.set_hammer_hardness(0.5);
    engine_soft.note_on(60, 0.8);
    let (soft_l, _) = engine_soft.render(0.2);

    let mut engine_hard = PianoEngine::new(48000.0, 30, false);
    engine_hard.set_hammer_hardness(2.5);
    engine_hard.note_on(60, 0.8);
    let (hard_l, _) = engine_hard.render(0.2);

    let mut diff_hammer = 0.0f64;
    for (s, h) in soft_l.iter().zip(hard_l.iter()) {
        diff_hammer += (s - h).abs();
    }
    assert!(diff_hammer > 0.05, "Hammer hardness change must noticeably alter acoustic response, got diff {diff_hammer}");

    // Test Inharmonicity Scale: 0.2 vs 2.5 must produce noticeably shifted partials
    let mut engine_inharm1 = PianoEngine::new(48000.0, 30, false);
    engine_inharm1.set_inharmonicity_scale(0.2);
    engine_inharm1.note_on(40, 0.8);
    let (inh1_l, _) = engine_inharm1.render(0.2);

    let mut engine_inharm2 = PianoEngine::new(48000.0, 30, false);
    engine_inharm2.set_inharmonicity_scale(2.5);
    engine_inharm2.note_on(40, 0.8);
    let (inh2_l, _) = engine_inharm2.render(0.2);

    let mut diff_inharm = 0.0f64;
    for (a, b) in inh1_l.iter().zip(inh2_l.iter()) {
        diff_inharm += (a - b).abs();
    }
    assert!(diff_inharm > 0.05, "Inharmonicity scale change must noticeably alter modal dispersion, got diff {diff_inharm}");

    // Test Unison Detuning: 0.0 (pure) vs 3.0 (wide detune)
    let mut engine_detune0 = PianoEngine::new(48000.0, 30, false);
    engine_detune0.set_unison_detuning(0.0);
    engine_detune0.note_on(60, 0.8);
    let (det0_l, _) = engine_detune0.render(0.3);

    let mut engine_detune3 = PianoEngine::new(48000.0, 30, false);
    engine_detune3.set_unison_detuning(3.0);
    engine_detune3.note_on(60, 0.8);
    let (det3_l, _) = engine_detune3.render(0.3);

    let mut diff_detune = 0.0f64;
    for (a, b) in det0_l.iter().zip(det3_l.iter()) {
        diff_detune += (a - b).abs();
    }
    assert!(diff_detune > 0.05, "Unison detuning change must noticeably alter beating pattern, got diff {diff_detune}");
}

#[test]
fn test_tier10_adaptive_modal_culling_and_high_polyphony() {
    let mut engine = PianoEngine::new(48000.0, 35, true);

    // 1. Check adaptive modal culling across keyboard
    let bass_v = engine.get_or_create_voice(21); // A0 (27.5 Hz)
    let bass_modes = bass_v.strings[0].num_modes;
    println!("Bass A0 modal count: {}", bass_modes);
    assert_eq!(bass_modes, 35, "Bass notes must retain full modal resolution");

    let treble_c7 = engine.get_or_create_voice(96); // C7 (2093 Hz)
    let c7_modes = treble_c7.strings[0].num_modes;
    println!("Treble C7 modal count: {}", c7_modes);
    assert!(c7_modes <= 12, "C7 modes must be culled by Nyquist hearing limit");

    let treble_c8 = engine.get_or_create_voice(108); // C8 (4186 Hz)
    let c8_modes = treble_c8.strings[0].num_modes;
    println!("High treble C8 modal count: {}", c8_modes);
    assert!(c8_modes <= 6, "C8 modes must be culled to 6 modes");

    // 2. High-Polyphony Test: Trigger 32 simultaneous voices
    for note in 40..72 {
        engine.note_on(note, 0.7);
    }
    assert_eq!(engine.active_keys.len(), 32, "Engine must support 32 concurrent voices under Tier 10 architecture");

    let block_size = 256;
    let mut out_l = vec![0.0; block_size];
    let mut out_r = vec![0.0; block_size];
    let mut out_events = Vec::new();

    // Step 20 blocks (~100ms) with 32 full polyphonic voices
    for _ in 0..20 {
        engine.process_block(block_size, &[], &mut out_events, &mut out_l, &mut out_r);
        for s in 0..block_size {
            assert!(!out_l[s].is_nan() && !out_r[s].is_nan());
            assert!(!out_l[s].is_infinite() && !out_r[s].is_infinite());
        }
    }
}


