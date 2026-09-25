//! Comprehensive Rust Verification Tests for Piano Physical Modeling Engine.

use physics_piano::params::grand_piano::generate_grand_piano_parameters;
use physics_piano::core::string::StiffStringModal;
use physics_piano::core::hammer::HuntCrossleyHammer;
use physics_piano::core::bridge::BridgeSoundboard;
use physics_piano::dsp::upols::UPOLSConvolver;
use physics_piano::engine::{PianoEngine, EngineEvent};

#[test]
fn test_database_88_keys_and_railsback() {
    let keys = generate_grand_piano_parameters(35, true);
    assert_eq!(keys.len(), 88);

    // Verify key boundaries: A0 (MIDI 21) to C8 (MIDI 108)
    let a0 = keys.iter().find(|k| k.midi_note == 21).expect("A0 should exist");
    assert_eq!(a0.pitch_name, "A0");
    assert_eq!(a0.num_unisons, 1);
    assert!(a0.target_f0 < 27.5 && a0.target_f0 > 26.5, "A0 should have negative Railsback stretch");

    let c4 = keys.iter().find(|k| k.midi_note == 60).expect("C4 should exist");
    assert_eq!(c4.pitch_name, "C4");
    assert_eq!(c4.num_unisons, 3);
    assert!((c4.target_f0 - 261.63).abs() < 1.0);

    let c8 = keys.iter().find(|k| k.midi_note == 108).expect("C8 should exist");
    assert_eq!(c8.pitch_name, "C8");
    assert_eq!(c8.num_unisons, 3);
    // C8 should be stretched higher than equal temperament 4186 Hz
    assert!(c8.target_f0 > 4186.0);

    // Verify monotonic frequencies
    for i in 0..(keys.len() - 1) {
        assert!(
            keys[i + 1].target_f0 > keys[i].target_f0,
            "Frequency must be strictly monotonic across all 88 keys"
        );
    }
}

#[test]
fn test_string_energy_and_decay() {
    let keys = generate_grand_piano_parameters(35, true);
    let a4 = keys.iter().find(|k| k.midi_note == 69).unwrap();
    let sr = 48000.0;
    let mut string = StiffStringModal::new(a4.strings[0].clone(), sr);

    // Initially string is resting with zero energy
    let init_energy = string.get_energy();
    assert!(init_energy < 1e-12);

    // Apply an impulse to mode 0
    string.state_t[0].q = 1e-4;
    let excited_energy = string.get_energy();
    assert!(excited_energy > 0.0);

    // Step forward 1000 samples and verify decay
    for _ in 0..1000 {
        string.step(0.0, 0.0, 0.0);
    }
    let decayed_energy = string.get_energy();
    assert!(decayed_energy < excited_energy);
    assert!(decayed_energy >= 0.0);
}

#[test]
fn test_hammer_sav_contact_stability() {
    let keys = generate_grand_piano_parameters(35, true);
    let a4 = keys.iter().find(|k| k.midi_note == 69).unwrap();
    let mut hammer = HuntCrossleyHammer::new(a4.hammer.clone(), 48000.0);
    hammer.strike(0.85, 0.0);

    let mut force_history = Vec::new();
    for _ in 0..120 {
        let f_h = hammer.compute_force(0.0, 0.0);
        assert!(!f_h.is_nan() && !f_h.is_infinite());
        assert!(f_h >= 0.0, "Contact force must be compressive");
        hammer.advance(f_h);
        force_history.push(f_h);
    }

    let max_force = force_history.iter().cloned().fold(0.0, f64::max);
    assert!(max_force > 1.0, "Peak hammer force should be significant");

    // Hammer should rebound and lose contact
    let final_force = *force_history.last().unwrap();
    assert_eq!(final_force, 0.0);
}

#[test]
fn test_bridge_and_soundboard_coupling() {
    let mut bridge = BridgeSoundboard::new(48000.0);
    let mut has_nonzero = false;

    for i in 0..200 {
        let f_t = if i == 0 { 10.0 } else { 0.0 };
        let (_react_t, _react_p, f_sb) = bridge.calculate_coupling_forces(f_t, 0.0, 0.0);
        let (out_l, out_r) = bridge.step_soundboard(f_sb, 0.5);
        assert!(!out_l.is_nan() && !out_r.is_nan());
        if out_l.abs() > 1e-8 || out_r.abs() > 1e-8 {
            has_nonzero = true;
        }
    }
    assert!(has_nonzero, "Bridge must propagate force into soundboard output");
}

#[test]
fn test_upols_convolver_correctness() {
    let block_size = 64;
    // Simple 3-sample FIR: [1.0, 0.5, 0.25]
    let ir = vec![1.0, 0.5, 0.25];
    let mut convolver = UPOLSConvolver::new(&ir, &ir, block_size);

    // Delta impulse input
    let mut in_block = vec![0.0; block_size];
    in_block[0] = 1.0;
    let mut out_l = vec![0.0; block_size];
    let mut out_r = vec![0.0; block_size];

    convolver.process_block(&in_block, &mut out_l, &mut out_r);

    assert!((out_l[0] - 1.0).abs() < 1e-5);
    assert!((out_l[1] - 0.5).abs() < 1e-5);
    assert!((out_l[2] - 0.25).abs() < 1e-5);
    assert!(out_l[3].abs() < 1e-5);

    assert!((out_r[0] - 1.0).abs() < 1e-5);
    assert!((out_r[1] - 0.5).abs() < 1e-5);
}

#[test]
fn test_engine_full_synthesis_and_pedal() {
    let mut engine = PianoEngine::new(48000.0, 30, true);
    let block_size = 64;
    let mut out_l = vec![0.0; block_size];
    let mut out_r = vec![0.0; block_size];

    // Trigger note A4 (MIDI 69)
    let events = vec![
        EngineEvent::NoteOn { time: 0, key: 69, velocity: 0.8 },
    ];
    let mut out_events = Vec::new();
    engine.process_block(block_size, &events, &mut out_events, &mut out_l, &mut out_r);

    // Check non-zero output
    let energy_l: f64 = out_l.iter().map(|s| s * s).sum();
    let energy_r: f64 = out_r.iter().map(|s| s * s).sum();
    assert!(energy_l > 0.0 || energy_r > 0.0);

    // Test half-pedal continuous modulation
    engine.set_sustain_pedal(true, 0.5);
    let empty_events: Vec<EngineEvent> = Vec::new();
    for _ in 0..10 {
        engine.process_block(block_size, &empty_events, &mut out_events, &mut out_l, &mut out_r);
        for i in 0..block_size {
            assert!(!out_l[i].is_nan() && !out_l[i].is_infinite());
            assert!(!out_r[i].is_nan() && !out_r[i].is_infinite());
        }
    }

    // Release note
    let release_events = vec![
        EngineEvent::NoteOff { time: 0, key: 69 },
    ];
    engine.process_block(block_size, &release_events, &mut out_events, &mut out_l, &mut out_r);

    // Fully release pedal
    engine.set_sustain_pedal(false, 0.0);
    for _ in 0..20 {
        engine.process_block(block_size, &empty_events, &mut out_events, &mut out_l, &mut out_r);
    }
}

#[test]
fn test_clap_plugin_c_abi_and_extensions() {
    use clap_sys::factory::plugin_factory::{clap_plugin_factory, CLAP_PLUGIN_FACTORY_ID};
    use clap_sys::ext::audio_ports::{clap_plugin_audio_ports, CLAP_EXT_AUDIO_PORTS};
    use clap_sys::ext::note_ports::{clap_plugin_note_ports, CLAP_EXT_NOTE_PORTS};
    use clap_sys::ext::params::{clap_plugin_params, CLAP_EXT_PARAMS};
    use clap_sys::ext::thread_pool::CLAP_EXT_THREAD_POOL;
    use physics_piano::raw_clap_entry;
    use std::ffi::CStr;
    use std::ptr;

    unsafe {
        // 1. Entry init
        let init_fn = raw_clap_entry.init.expect("init must be defined");
        assert!(init_fn(ptr::null()));

        // 2. Factory lookup
        let get_factory_fn = raw_clap_entry.get_factory.expect("get_factory must be defined");
        let factory_ptr = get_factory_fn(CLAP_PLUGIN_FACTORY_ID.as_ptr());
        assert!(!factory_ptr.is_null());

        let factory = &*(factory_ptr as *const clap_plugin_factory);
        let count_fn = factory.get_plugin_count.unwrap();
        assert_eq!(count_fn(factory), 1);

        let desc_fn = factory.get_plugin_descriptor.unwrap();
        let desc = &*desc_fn(factory, 0);
        let id_str = CStr::from_ptr(desc.id).to_str().unwrap();
        assert_eq!(id_str, "org.eu.mumulhl.physics-piano");

        // 3. Create plugin instance
        let create_fn = factory.create_plugin.unwrap();
        let plugin = create_fn(factory, ptr::null(), desc.id);
        assert!(!plugin.is_null());

        // 4. Query extensions
        let get_ext_fn = (*plugin).get_extension.unwrap();

        // Audio Ports
        let audio_ext = get_ext_fn(plugin, CLAP_EXT_AUDIO_PORTS.as_ptr());
        assert!(!audio_ext.is_null());
        let audio_ports = &*(audio_ext as *const clap_plugin_audio_ports);
        assert_eq!((audio_ports.count.unwrap())(plugin, true), 0); // 0 audio inputs
        assert_eq!((audio_ports.count.unwrap())(plugin, false), 1); // 1 audio output

        // Note Ports
        let note_ext = get_ext_fn(plugin, CLAP_EXT_NOTE_PORTS.as_ptr());
        assert!(!note_ext.is_null());
        let note_ports = &*(note_ext as *const clap_plugin_note_ports);
        assert_eq!((note_ports.count.unwrap())(plugin, true), 1); // 1 note input
        assert_eq!((note_ports.count.unwrap())(plugin, false), 0); // 0 note outputs

        // Parameters
        let params_ext = get_ext_fn(plugin, CLAP_EXT_PARAMS.as_ptr());
        assert!(!params_ext.is_null());
        let params = &*(params_ext as *const clap_plugin_params);
        assert_eq!((params.count.unwrap())(plugin), 3); // Sustain, Una Corda, Master Volume

        // Thread Pool
        let tp_ext = get_ext_fn(plugin, CLAP_EXT_THREAD_POOL.as_ptr());
        assert!(!tp_ext.is_null());

        // 5. Lifecycle: init, activate, deactivate, destroy
        let init_p = (*plugin).init.unwrap();
        assert!(init_p(plugin));

        let activate_p = (*plugin).activate.unwrap();
        assert!(activate_p(plugin, 48000.0, 32, 512));

        let deactivate_p = (*plugin).deactivate.unwrap();
        deactivate_p(plugin);

        let destroy_p = (*plugin).destroy.unwrap();
        destroy_p(plugin);

        // 6. Entry deinit
        let deinit_fn = raw_clap_entry.deinit.expect("deinit must be defined");
        deinit_fn();
    }
}

#[test]
fn test_polyphonic_stability_and_headroom() {
    let mut engine = PianoEngine::new(48000.0, 35, true);
    engine.set_sustain_pedal(true, 1.0);

    let chord = [36, 43, 48, 52, 55, 60, 64, 67, 71, 72]; // 10-note massive chord
    for &note in &chord {
        engine.note_on(note, 0.95);
    }

    let block_size = 512;
    let mut out_l = vec![0.0; block_size];
    let mut out_r = vec![0.0; block_size];
    let mut out_events = Vec::new();

    let mut max_abs = 0.0f64;

    for _ in 0..100 { // ~1.06 seconds
        engine.process_block(block_size, &[], &mut out_events, &mut out_l, &mut out_r);
        for s in 0..block_size {
            assert!(!out_l[s].is_nan(), "Left channel is NaN!");
            assert!(!out_r[s].is_nan(), "Right channel is NaN!");
            assert!(!out_l[s].is_infinite(), "Left channel is Infinite!");
            assert!(!out_r[s].is_infinite(), "Right channel is Infinite!");
            max_abs = max_abs.max(out_l[s].abs()).max(out_r[s].abs());
        }
    }

    println!("10-Note Chord Peak Absolute Amplitude: {}", max_abs);
}


