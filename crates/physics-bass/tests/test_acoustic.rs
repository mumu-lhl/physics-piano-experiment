use physics_bass::{
    AcousticBassBody, BassEngine, BassMode, BassStringParams, FdtdString, PluckStyle,
};
use realfft::RealFftPlanner;
use std::f64::consts::PI;

const SAMPLE_RATE: f64 = 48_000.0;

fn render_note(
    note: u8,
    velocity: f64,
    style: PluckStyle,
    tone: f64,
    pickup_position: f64,
    seconds: f64,
) -> Vec<f64> {
    let mut engine = BassEngine::new(SAMPLE_RATE, BassMode::Electric, true);
    engine.set_body_mix(0.0);
    engine.set_pluck_style(style);
    engine.set_tone(tone);
    engine.set_pickup_position(pickup_position);
    engine.note_on(note, velocity);
    (0..(seconds * SAMPLE_RATE) as usize)
        .map(|_| {
            let (left, right) = engine.process_sample();
            0.5 * (left + right)
        })
        .collect()
}

fn spectrum(signal: &[f64], start: usize, length: usize) -> (Vec<f64>, usize) {
    let fft_size = (length * 4).next_power_of_two();
    let mut input = vec![0.0; fft_size];
    for (index, sample) in signal[start..start + length].iter().enumerate() {
        let window = 0.5 - 0.5 * (2.0 * PI * index as f64 / (length - 1) as f64).cos();
        input[index] = sample * window;
    }

    let mut planner = RealFftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(fft_size);
    let mut output = fft.make_output_vec();
    fft.process(&mut input, &mut output)
        .expect("FFT input and output sizes match plan");
    (output.iter().map(|bin| bin.norm()).collect(), fft_size)
}

fn peak_frequency(signal: &[f64], start: usize, length: usize, target_hz: f64) -> f64 {
    let (magnitudes, fft_size) = spectrum(signal, start, length);
    let radius_hz = (target_hz * 0.08).max(2.0);
    peak_in_band(&magnitudes, fft_size, target_hz, radius_hz)
}

fn peak_in_band(magnitudes: &[f64], fft_size: usize, target_hz: f64, radius_hz: f64) -> f64 {
    let resolution = SAMPLE_RATE / fft_size as f64;
    let first = ((target_hz - radius_hz).max(0.0) / resolution).floor() as usize;
    let last = ((target_hz + radius_hz) / resolution).ceil() as usize;
    let peak = (first..=last)
        .max_by(|&left, &right| magnitudes[left].total_cmp(&magnitudes[right]))
        .expect("peak search range contains FFT bins");
    let a = magnitudes[peak - 1].max(1e-30).ln();
    let b = magnitudes[peak].max(1e-30).ln();
    let c = magnitudes[peak + 1].max(1e-30).ln();
    let denominator = a - 2.0 * b + c;
    let offset = if denominator.abs() > 1e-12 {
        (0.5 * (a - c) / denominator).clamp(-1.0, 1.0)
    } else {
        0.0
    };
    (peak as f64 + offset) * resolution
}

fn rms(signal: &[f64]) -> f64 {
    (signal.iter().map(|sample| sample * sample).sum::<f64>() / signal.len() as f64).sqrt()
}

#[test]
fn rendered_open_string_pitch_tracks_b0_e1_a1_and_g2() {
    for (note, label) in [(23, "B0"), (28, "E1"), (33, "A1"), (43, "G2")] {
        let signal = render_note(note, 0.55, PluckStyle::Finger, 0.8, 0.18, 1.2);
        let observed = peak_frequency(
            &signal,
            (0.15 * SAMPLE_RATE) as usize,
            (0.85 * SAMPLE_RATE) as usize,
            440.0 * 2.0_f64.powf((note as f64 - 69.0) / 12.0),
        );
        let expected = 440.0 * 2.0_f64.powf((note as f64 - 69.0) / 12.0);
        let cents = 1200.0 * (observed / expected).log2();
        assert!(
            cents.abs() < 45.0,
            "{label} rendered pitch is {observed:.2} Hz, expected {expected:.2} Hz ({cents:+.1} cents)"
        );
    }
}

#[test]
fn rendered_e1_partials_follow_the_string_inharmonicity_model() {
    let params = BassStringParams::electric_four()[0];
    let theoretical_b = PI.powi(2)
        * params.youngs_modulus
        * params.second_moment()
        * params.bending_stiffness.powi(2)
        / (params.tension * params.scale_length.powi(2));
    let mut string = FdtdString::new(params, SAMPLE_RATE);
    string.trigger(0.2, PluckStyle::Finger, 0.18);
    let signal = (0..(1.4 * SAMPLE_RATE) as usize)
        .map(|_| string.step())
        .collect::<Vec<_>>();
    let start = (0.2 * SAMPLE_RATE) as usize;
    let length = (1.0 * SAMPLE_RATE) as usize;
    let (magnitudes, fft_size) = spectrum(&signal, start, length);

    let points = (1..=8)
        .map(|mode| {
            let expected_hz =
                params.open_f0 * mode as f64 * (1.0 + theoretical_b * (mode * mode) as f64).sqrt();
            let observed_hz = peak_in_band(&magnitudes, fft_size, expected_hz, expected_hz * 0.04);
            (mode as f64, observed_hz)
        })
        .collect::<Vec<_>>();
    let mean_x = points.iter().map(|(mode, _)| mode * mode).sum::<f64>() / points.len() as f64;
    let mean_y = points
        .iter()
        .map(|(mode, frequency)| (frequency / mode).powi(2))
        .sum::<f64>()
        / points.len() as f64;
    let covariance = points
        .iter()
        .map(|(mode, frequency)| (mode * mode - mean_x) * ((frequency / mode).powi(2) - mean_y))
        .sum::<f64>();
    let variance = points
        .iter()
        .map(|(mode, _)| (mode * mode - mean_x).powi(2))
        .sum::<f64>();
    let fitted_b = (covariance / variance) / (mean_y - (covariance / variance) * mean_x);
    let relative_error = ((fitted_b - theoretical_b) / theoretical_b).abs();

    assert!(
        relative_error < 0.2,
        "rendered E1 B factor differs from the stiff-string model: fitted={fitted_b:.5e}, theory={theoretical_b:.5e}, relative error={:.1}% (limit 20%)",
        relative_error * 100.0
    );
}

#[test]
fn harder_finger_pluck_has_greater_attack_level() {
    let soft = render_note(33, 0.2, PluckStyle::Finger, 0.8, 0.18, 0.08);
    let hard = render_note(33, 0.9, PluckStyle::Finger, 0.8, 0.18, 0.08);
    let soft_rms = rms(&soft);
    let hard_rms = rms(&hard);
    assert!(
        hard_rms > soft_rms * 1.5,
        "harder pluck should produce a stronger attack: soft={soft_rms:.3e}, hard={hard_rms:.3e}"
    );
}

#[test]
fn pickup_position_changes_the_harmonic_balance() {
    let neck_position = render_note(33, 0.65, PluckStyle::Finger, 0.8, 0.12, 0.7);
    let bridge_position = render_note(33, 0.65, PluckStyle::Finger, 0.8, 0.32, 0.7);
    let start = (0.08 * SAMPLE_RATE) as usize;
    let length = (0.55 * SAMPLE_RATE) as usize;
    let (neck, _) = spectrum(&neck_position, start, length);
    let (bridge, _) = spectrum(&bridge_position, start, length);
    let total_neck = neck.iter().sum::<f64>().max(1e-30);
    let total_bridge = bridge.iter().sum::<f64>().max(1e-30);
    let shape_distance = neck
        .iter()
        .zip(&bridge)
        .map(|(a, b)| (a / total_neck - b / total_bridge).abs())
        .sum::<f64>();
    assert!(
        shape_distance > 0.08,
        "moving the pickup should measurably change its normalized spectrum: L1={shape_distance:.3}"
    );
}

#[test]
fn wooden_body_radiation_peaks_near_the_a0_mode() {
    fn steady_rms(frequency: f64) -> f64 {
        let mut body = AcousticBassBody::new(SAMPLE_RATE);
        let total_samples = SAMPLE_RATE as usize;
        let measure_from = total_samples - (0.2 * SAMPLE_RATE) as usize;
        let mut energy = 0.0;
        for sample in 0..total_samples {
            let force = 1.0e-4 * (2.0 * PI * frequency * sample as f64 / SAMPLE_RATE).sin();
            let output = body.process(force);
            if sample >= measure_from {
                energy += output * output;
            }
        }
        (energy / (total_samples - measure_from) as f64).sqrt()
    }

    let resonant = steady_rms(59.0);
    let below_mode = steady_rms(43.0);
    let above_mode = steady_rms(76.0);
    let off_resonance = below_mode.max(above_mode);
    assert!(
        resonant > off_resonance * 3.0,
        "A0 body mode should dominate nearby response: 59 Hz={resonant:.3e}, off-mode={off_resonance:.3e}"
    );
}

#[test]
fn note_release_reduces_late_electric_string_energy() {
    fn late_rms(release: bool) -> f64 {
        let mut engine = BassEngine::new(SAMPLE_RATE, BassMode::Electric, true);
        engine.set_body_mix(0.0);
        engine.note_on(33, 0.7);
        let mut output = Vec::with_capacity((0.7 * SAMPLE_RATE) as usize);
        for sample in 0..(0.7 * SAMPLE_RATE) as usize {
            if release && sample == (0.15 * SAMPLE_RATE) as usize {
                engine.note_off(33);
            }
            let (left, right) = engine.process_sample();
            output.push(0.5 * (left + right));
        }
        rms(&output[output.len() - (0.12 * SAMPLE_RATE) as usize..])
    }

    let held = late_rms(false);
    let released = late_rms(true);
    assert!(
        released < held * 0.5,
        "note-off should damp the late tail: held={held:.3e}, released={released:.3e}"
    );
}

#[test]
fn test_gui_dynamic_velocity_and_midi_routing() {
    let mut engine = BassEngine::new(SAMPLE_RATE, BassMode::Electric, true);

    // 1. Dynamic velocity alters pluck attack level
    let mut engine_soft = BassEngine::new(SAMPLE_RATE, BassMode::Electric, true);
    let mut engine_hard = BassEngine::new(SAMPLE_RATE, BassMode::Electric, true);
    engine_soft.note_on_string(1, 0, 0.35);
    engine_hard.note_on_string(1, 0, 0.98);

    let energy_soft = engine_soft.strings[1].string.energy();
    let energy_hard = engine_hard.strings[1].string.energy();
    assert!(
        energy_hard > energy_soft * 2.0,
        "Higher velocity pluck must yield higher string energy (hard={energy_hard}, soft={energy_soft})"
    );

    // 2. Midi note routing
    engine.note_on(28, 0.88); // E1 (String index 1, fret 0)
    assert!(engine.strings[1].current_note.is_some());
    engine.note_off(28);
    assert!(engine.strings[1].string.is_releasing);

    // 3. Repeated note_on on same note retriggers without state corruption
    engine.note_on(28, 0.88);
    assert_eq!(engine.strings[1].current_note, Some(28));
    engine.note_on(28, 0.95);
    assert_eq!(engine.strings[1].current_note, Some(28));
    assert!(engine.strings[1].string.is_active);
}

#[test]
fn test_acoustic_bass_mode_switching_and_tuning() {
    let mut engine = BassEngine::new(SAMPLE_RATE, BassMode::Electric, true);
    assert_eq!(engine.mode, BassMode::Electric);

    // Switch to Acoustic Upright Bass
    engine.set_mode(BassMode::Acoustic);
    assert_eq!(engine.mode, BassMode::Acoustic);

    // Strings should be reconfigured to 1.05m Double Bass scale
    assert!((engine.strings[1].string.params.scale_length - 1.05).abs() < 1e-6);

    // Trigger note and verify finite stable sound rendering
    engine.note_on(28, 0.8);
    for _ in 0..1000 {
        let (l, r) = engine.process_sample();
        assert!(!l.is_nan() && !r.is_nan());
    }
}

#[test]
fn test_bass_fdtd_physical_legato_preserves_energy_and_pitch() {
    let mut engine = BassEngine::new(SAMPLE_RATE, BassMode::Electric, true);
    // Pluck E1 string at fret 0 (open E1)
    engine.note_on_string(1, 0, 0.85);

    for _ in 0..500 {
        let _ = engine.process_sample();
    }
    let energy_before = engine.strings[1].string.energy();
    assert!(
        energy_before > 1e-7,
        "String must be vibrating before legato"
    );

    // Hammer-on to fret 5 (A1)
    engine.legato_to_string(1, 5, 0.80);
    assert_eq!(engine.strings[1].current_fret, 5);
    assert!(engine.strings[1].string.is_active);
    let energy_after_hammer = engine.strings[1].string.energy();
    assert!(
        energy_after_hammer > energy_before * 0.35,
        "FDTD legato must preserve vibrating wave energy (before={energy_before}, after={energy_after_hammer})"
    );

    for _ in 0..500 {
        let _ = engine.process_sample();
    }
    let energy_fret5 = engine.strings[1].string.energy();

    // Pull-off back to fret 0
    engine.legato_to_string(1, 0, 0.75);
    assert_eq!(engine.strings[1].current_fret, 0);
    assert!(engine.strings[1].string.is_active);
    let energy_after_pulloff = engine.strings[1].string.energy();
    assert!(
        energy_after_pulloff > energy_fret5 * 0.30,
        "Pull-off must preserve energy without grid wipe (before={energy_fret5}, after={energy_after_pulloff})"
    );
}

#[test]
fn test_bass_pop_and_ghost_articulations() {
    let mut string_finger = FdtdString::new(BassStringParams::electric_four()[0], SAMPLE_RATE);
    let mut string_pop = string_finger.clone();
    let mut string_ghost = string_finger.clone();

    // 1. Pop has high attack energy & slap active
    string_pop.trigger(0.9, PluckStyle::Pop, 0.2);
    assert!(string_pop.is_active);
    assert!(
        string_pop.energy() > 1e-5,
        "Pop excitation must produce high initial energy"
    );

    // 2. Ghost note has fast viscoelastic damping
    string_finger.trigger(0.9, PluckStyle::Finger, 0.2);
    string_ghost.trigger(0.9, PluckStyle::Ghost, 0.2);

    for _ in 0..2500 {
        // ~55ms
        string_finger.step();
        string_ghost.step();
    }

    let finger_energy = string_finger.energy();
    let ghost_energy = string_ghost.energy();
    assert!(
        ghost_energy < finger_energy * 0.15,
        "Ghost note must decay rapidly compared to finger pluck (ghost={ghost_energy}, finger={finger_energy})"
    );
}

#[test]
fn test_bass_dual_jbass_pickup_blend_comb_filtering() {
    let mut engine = BassEngine::new(SAMPLE_RATE, BassMode::Electric, true);
    // Test blend settings
    engine.set_pickup_blend(0.0); // 100% Neck
    assert_eq!(engine.pickup.pickup_blend, 0.0);

    engine.set_pickup_blend(0.5); // 50/50 Scooped J-Bass
    assert_eq!(engine.pickup.pickup_blend, 0.5);

    engine.set_pickup_blend(1.0); // 100% Bridge
    assert_eq!(engine.pickup.pickup_blend, 1.0);

    // Render with 50/50 blend
    engine.note_on(28, 0.85);
    let mut max_abs = 0.0_f64;
    for _ in 0..1000 {
        let (l, r) = engine.process_sample();
        assert!(!l.is_nan() && !r.is_nan());
        max_abs = max_abs.max(l.abs()).max(r.abs());
    }
    assert!(
        max_abs > 1e-4,
        "Blended dual pickup must produce clear audible audio"
    );
}

#[test]
fn test_bass_arco_bowed_stick_slip_sustain() {
    let params = BassStringParams::electric_four()[1]; // E1 string
    let mut string_bowed = FdtdString::new(params, SAMPLE_RATE);
    let mut string_plucked = FdtdString::new(params, SAMPLE_RATE);

    string_bowed.trigger(0.8, PluckStyle::Arco, 0.15);
    string_plucked.trigger(0.8, PluckStyle::Finger, 0.15);

    // Evolve 10,000 samples (~0.23 seconds)
    for _ in 0..10_000 {
        string_bowed.step();
        string_plucked.step();
    }

    let bowed_energy = string_bowed.energy();
    let _plucked_energy = string_plucked.energy();

    assert!(
        bowed_energy > 1e-6,
        "Continuous stick-slip bowing must maintain sustained vibration energy (bowed={bowed_energy})"
    );
    assert!(
        string_bowed.bow_active,
        "Bow should remain active until note off / release"
    );

    // Now release bow
    string_bowed.release();
    assert!(!string_bowed.bow_active);
    for _ in 0..5000 {
        string_bowed.step();
    }
    assert!(
        string_bowed.energy() < bowed_energy,
        "After bow release, bowed string must decay naturally"
    );
}

#[test]
fn test_bass_neck_dead_spot_attenuation() {
    let params = BassStringParams::electric_four()[3]; // G2 string (open 98 Hz, 6th fret = C#3 ~ 138.6 Hz near neck resonance)
    let mut string_normal = FdtdString::new(params, SAMPLE_RATE);
    string_normal.set_fret(6); // C#3
    let mut string_dead_spot = string_normal.clone();

    string_dead_spot.set_neck_dead_spot(0.9); // enable neck dead spot absorption
    assert_eq!(string_dead_spot.neck_dead_spot_depth, 0.9);

    string_normal.trigger(0.8, PluckStyle::Finger, 0.2);
    string_dead_spot.trigger(0.8, PluckStyle::Finger, 0.2);

    // Evolve 8000 samples (~180 ms)
    for _ in 0..8000 {
        string_normal.step();
        string_dead_spot.step();
    }

    let normal_energy = string_normal.energy();
    let dead_spot_energy = string_dead_spot.energy();

    assert!(
        dead_spot_energy < normal_energy * 0.75,
        "Neck bending resonator must accelerate vibrational energy absorption at dead spot frequency (dead_spot={dead_spot_energy}, normal={normal_energy})"
    );
}

#[test]
fn test_bass_split_coil_pbass_pickup() {
    let mut engine = BassEngine::new(SAMPLE_RATE, BassMode::Electric, true);
    engine.set_pickup_type(physics_bass::acoustic::BassPickupType::PrecisionSplitCoil);
    assert_eq!(
        engine.pickup.pickup_type,
        physics_bass::acoustic::BassPickupType::PrecisionSplitCoil
    );

    // Play low note (E1)
    engine.note_on(28, 0.85);
    let mut max_abs = 0.0_f64;
    for _ in 0..800 {
        let (l, r) = engine.process_sample();
        max_abs = max_abs.max(l.abs()).max(r.abs());
    }
    assert!(
        max_abs > 1e-4,
        "Split-coil P-Bass pickup must produce clear audible signal on low strings"
    );

    // Switch back to Jazz Bass
    engine.set_pickup_type(physics_bass::acoustic::BassPickupType::JazzDualSingle);
    assert_eq!(
        engine.pickup.pickup_type,
        physics_bass::acoustic::BassPickupType::JazzDualSingle
    );
}

