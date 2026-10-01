use physics_piano::core::string::StiffStringModal;
use physics_piano::engine::PianoEngine;
use physics_piano::params::generate_grand_piano_parameters;
use realfft::RealFftPlanner;
use std::f64::consts::PI;

const SAMPLE_RATE: f64 = 48_000.0;

fn mono(left: &[f64], right: &[f64]) -> Vec<f64> {
    left.iter()
        .zip(right)
        .map(|(&l, &r)| 0.5 * (l + r))
        .collect()
}

fn spectrum(signal: &[f64], start: usize, length: usize) -> (Vec<f64>, usize) {
    let fft_size = (length * 4).next_power_of_two();
    let mut input = vec![0.0; fft_size];
    for (i, sample) in signal[start..start + length].iter().enumerate() {
        let window = 0.5 - 0.5 * (2.0 * PI * i as f64 / (length - 1) as f64).cos();
        input[i] = sample * window;
    }

    let mut planner = RealFftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(fft_size);
    let mut output = fft.make_output_vec();
    fft.process(&mut input, &mut output)
        .expect("FFT input and output sizes match plan");
    (output.iter().map(|bin| bin.norm()).collect(), fft_size)
}

fn interpolated_peak_hz(
    magnitudes: &[f64],
    fft_size: usize,
    target_hz: f64,
    radius_hz: f64,
) -> f64 {
    let resolution = SAMPLE_RATE / fft_size as f64;
    let first = ((target_hz - radius_hz).max(0.0) / resolution).floor() as usize;
    let last = ((target_hz + radius_hz) / resolution).ceil() as usize;
    let peak = (first..=last)
        .max_by(|&a, &b| magnitudes[a].total_cmp(&magnitudes[b]))
        .expect("peak search range contains FFT bins");
    if peak == 0 || peak + 1 >= magnitudes.len() {
        return peak as f64 * resolution;
    }

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

fn spectral_centroid(signal: &[f64], start: usize, length: usize) -> f64 {
    let (magnitudes, fft_size) = spectrum(signal, start, length);
    let bin_hz = SAMPLE_RATE / fft_size as f64;
    let weighted_sum = magnitudes
        .iter()
        .enumerate()
        .map(|(i, magnitude)| i as f64 * bin_hz * magnitude)
        .sum::<f64>();
    let magnitude_sum = magnitudes.iter().sum::<f64>().max(1e-30);
    weighted_sum / magnitude_sum
}

fn render_note(note: u8, velocity: f64, duration: f64, detuning_scale: f64) -> Vec<f64> {
    let mut engine = PianoEngine::new(SAMPLE_RATE, 35, true);
    engine.set_unison_detuning(detuning_scale);
    engine.note_on(note, velocity);
    let (left, right) = engine.render(duration);
    mono(&left, &right)
}

#[test]
fn rendered_a4_partials_match_stiff_string_inharmonicity() {
    let mut engine = PianoEngine::new(SAMPLE_RATE, 35, true);
    engine.note_on(69, 0.85);
    let voice = engine.get_voice(69).expect("note-on creates A4 voice");
    let string = &voice.strings[1]; // Center string has zero unison detuning.
    let params = &string.params;
    let theoretical_b = (PI.powi(3) * params.youngs_modulus * params.radius.powi(4))
        / (4.0 * params.tension * params.length.powi(2));
    let expected_frequencies = string
        .omega_t
        .iter()
        .take(24)
        .map(|omega| omega / (2.0 * PI))
        .collect::<Vec<_>>();
    let (left, right) = engine.render(1.35);
    let signal = mono(&left, &right);
    let (magnitudes, fft_size) = spectrum(&signal, 7_200, 48_000);

    let mut points = Vec::with_capacity(expected_frequencies.len());
    for (mode, expected_hz) in expected_frequencies.iter().copied().enumerate() {
        let observed_hz = interpolated_peak_hz(
            &magnitudes,
            fft_size,
            expected_hz,
            (expected_hz * 0.008).max(3.0),
        );
        points.push(((mode + 1) as f64, observed_hz));
    }

    // Fit (f_n / n)^2 = f_0^2 + f_0^2 * B * n^2.
    let mean_x = points.iter().map(|(n, _)| n * n).sum::<f64>() / points.len() as f64;
    let mean_y = points
        .iter()
        .map(|(n, frequency)| (frequency / n).powi(2))
        .sum::<f64>()
        / points.len() as f64;
    let covariance = points
        .iter()
        .map(|(n, frequency)| (n * n - mean_x) * ((frequency / n).powi(2) - mean_y))
        .sum::<f64>();
    let variance = points
        .iter()
        .map(|(n, _)| (n * n - mean_x).powi(2))
        .sum::<f64>();
    let slope = covariance / variance;
    let intercept = mean_y - slope * mean_x;
    let fitted_b = slope / intercept;
    let relative_error = ((fitted_b - theoretical_b) / theoretical_b).abs();
    let fitted_f0 = intercept.sqrt();
    let cent_std = (points
        .iter()
        .map(|(n, frequency)| {
            let predicted = fitted_f0 * n * (1.0 + theoretical_b * n * n).sqrt();
            (1200.0 * (frequency / predicted).log2()).powi(2)
        })
        .sum::<f64>()
        / points.len() as f64)
        .sqrt();

    assert!(
        relative_error <= 0.012,
        "rendered A4 B factor differs from string model: fitted={fitted_b:.6e}, theory={theoretical_b:.6e}, relative error={:.2}% (limit 1.2%)",
        relative_error * 100.0
    );
    assert!(
        cent_std < 2.0,
        "rendered A4 partial cent-error standard deviation is {cent_std:.2} cents (limit 2.0 cents)"
    );
}

#[test]
fn harder_a4_strike_has_brighter_attack_spectrum() {
    let sample_count = (0.12 * SAMPLE_RATE) as usize;
    let soft = render_note(69, 0.25, 0.12, 1.0);
    let hard = render_note(69, 0.95, 0.12, 1.0);
    let soft_centroid = spectral_centroid(&soft, 0, sample_count);
    let hard_centroid = spectral_centroid(&hard, 0, sample_count);

    assert!(
        hard_centroid > soft_centroid,
        "hard strike should excite a brighter spectrum: soft={soft_centroid:.1} Hz, hard={hard_centroid:.1} Hz"
    );
}

#[test]
fn unison_detuning_spreads_partial_energy() {
    let partial = 12;
    let duration = 1.6;
    let window_start = (0.05 * SAMPLE_RATE) as usize;
    let window_len = (1.5 * SAMPLE_RATE) as usize;

    let mut detuned_engine = PianoEngine::new(SAMPLE_RATE, 35, true);
    detuned_engine.note_on(69, 0.8);
    let detuned_voice = detuned_engine.get_voice(69).unwrap();
    let detuned_freqs = detuned_voice
        .strings
        .iter()
        .map(|string| string.omega_t[partial - 1] / (2.0 * PI))
        .collect::<Vec<_>>();
    let (detuned_l, detuned_r) = detuned_engine.render(duration);
    let (detuned_spectrum, fft_size) =
        spectrum(&mono(&detuned_l, &detuned_r), window_start, window_len);

    let mut matched_engine = PianoEngine::new(SAMPLE_RATE, 35, true);
    matched_engine.set_unison_detuning(0.0);
    matched_engine.note_on(69, 0.8);
    let (matched_l, matched_r) = matched_engine.render(duration);
    let matched_signal = mono(&matched_l, &matched_r);
    let (matched_spectrum, _) = spectrum(&matched_signal, window_start, window_len);

    let center_hz = detuned_freqs.iter().sum::<f64>() / detuned_freqs.len() as f64;
    let radius_hz = 16.0;
    let bin_hz = SAMPLE_RATE / fft_size as f64;
    let first = ((center_hz - radius_hz) / bin_hz).floor() as usize;
    let last = ((center_hz + radius_hz) / bin_hz).ceil() as usize;
    let spectral_variance = |magnitudes: &[f64]| {
        let mut total_power = 0.0;
        let mut weighted_offset_sq = 0.0;
        for (bin, magnitude) in magnitudes.iter().enumerate().take(last + 1).skip(first) {
            let power = magnitude * magnitude;
            let offset = bin as f64 * bin_hz - center_hz;
            total_power += power;
            weighted_offset_sq += power * offset * offset;
        }
        weighted_offset_sq / total_power.max(1e-30)
    };

    let detuned_variance = spectral_variance(&detuned_spectrum);
    let matched_variance = spectral_variance(&matched_spectrum);
    assert!(
        detuned_variance > matched_variance * 1.1,
        "detuned unison should spread partial energy: detuned={detuned_variance:.3}, matched={matched_variance:.3} Hz^2"
    );
}

#[test]
fn held_key_has_longer_acoustic_tail_than_damped_release() {
    fn tail_rms(release_key: bool) -> f64 {
        let mut engine = PianoEngine::new(SAMPLE_RATE, 24, true);
        engine.note_on(60, 0.85);
        let _ = engine.render(0.12);
        if release_key {
            engine.note_off(60);
        }
        let (left, right) = engine.render(0.48);
        let start = left.len() - 7_200;
        let count = left.len() - start;
        let energy = left[start..]
            .iter()
            .chain(&right[start..])
            .map(|sample| sample * sample)
            .sum::<f64>();
        (energy / (2 * count) as f64).sqrt()
    }

    let released_tail = tail_rms(true);
    let held_tail = tail_rms(false);
    assert!(
        held_tail > released_tail * 1.5,
        "damper lift should preserve more late acoustic energy: held={held_tail:.3e}, released={released_tail:.3e}"
    );
}

#[test]
fn nonlinear_hammer_contact_shortens_as_velocity_increases() {
    let params = generate_grand_piano_parameters(35, true)
        .into_iter()
        .find(|key| key.midi_note == 69)
        .expect("A4 parameters exist");
    let string_params = params.strings[0].clone();
    let hammer_params = params.hammer.clone();

    let contact_samples = |velocity: f64| {
        let mut string = StiffStringModal::new(string_params.clone(), SAMPLE_RATE);
        let mut hammer = physics_piano::core::hammer::HuntCrossleyHammer::new(
            hammer_params.clone(),
            SAMPLE_RATE,
        );
        hammer.strike(velocity, 0.0);
        let mut samples = 0;
        for _ in 0..(SAMPLE_RATE as usize / 20) {
            let (u, v) = string.get_strike_displacement_and_velocity();
            let force = hammer.compute_force(u, v);
            if force > 0.0 {
                samples += 1;
            }
            string.step(force, 0.0, 0.0);
            hammer.advance(force);
            if hammer.has_struck && !hammer.is_active {
                break;
            }
        }
        samples
    };

    let velocities = [0.2, 0.4, 0.6, 0.8, 1.0];
    let durations = velocities.map(contact_samples);
    assert!(
        durations.windows(2).all(|pair| pair[0] >= pair[1]),
        "contact duration should decrease across velocity ladder: {durations:?}"
    );
    assert!(durations[0] > durations[4]);
}
