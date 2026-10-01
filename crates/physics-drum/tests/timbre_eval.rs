use physics_drum::DrumEngine;
use std::f64::consts::PI;

// Deterministic relative regression checks for isolated General MIDI hits.
// These descriptor thresholds keep kit parts and envelopes distinct; they are
// not a substitute for fitting against close-miked reference recordings.
const SAMPLE_RATE: usize = 48_000;
const HIT_FRAMES: usize = SAMPLE_RATE;
const FFT_SIZE: usize = 4_096;

#[derive(Debug, Clone, Copy)]
struct TimbreFeatures {
    rms: f64,
    peak: f64,
    centroid_hz: f64,
    high_band_ratio: f64,
    time_to_peak_ms: f64,
    decay_ratio: f64,
}

fn render_note(note: u8, velocity: f64) -> Vec<f64> {
    let mut engine = DrumEngine::new(SAMPLE_RATE as f64);
    engine.set_master_gain(1.0);
    engine.trigger(note, velocity);
    (0..HIT_FRAMES)
        .map(|_| {
            let (left, right) = engine.process_sample();
            (left + right) * 0.5
        })
        .collect()
}

fn features(samples: &[f64]) -> TimbreFeatures {
    let fft = spectrum(&samples[..FFT_SIZE]);
    let bin_hz = SAMPLE_RATE as f64 / FFT_SIZE as f64;
    let mut spectral_energy = 0.0;
    let mut weighted_energy = 0.0;
    let mut high_energy = 0.0;
    for (index, magnitude_squared) in fft.iter().copied().enumerate().skip(1) {
        spectral_energy += magnitude_squared;
        weighted_energy += index as f64 * bin_hz * magnitude_squared;
        if index as f64 * bin_hz >= 5_000.0 {
            high_energy += magnitude_squared;
        }
    }

    let rms = window_rms(samples, 0, SAMPLE_RATE / 2);
    let peak = samples
        .iter()
        .fold(0.0_f64, |peak, value| peak.max(value.abs()));
    let attack_window = SAMPLE_RATE / 1_000;
    let attack_windows: Vec<f64> = samples
        .chunks_exact(attack_window)
        .take(200)
        .map(slice_rms)
        .collect();
    let time_to_peak_ms = attack_windows
        .iter()
        .enumerate()
        .max_by(|(_, left), (_, right)| {
            left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(index, _)| (index + 1) as f64)
        .unwrap_or(0.0);
    let early_rms = window_rms(samples, SAMPLE_RATE / 20, SAMPLE_RATE * 3 / 20);
    let late_rms = window_rms(samples, SAMPLE_RATE * 7 / 20, SAMPLE_RATE * 9 / 20);

    TimbreFeatures {
        rms,
        peak,
        centroid_hz: weighted_energy / spectral_energy.max(f64::MIN_POSITIVE),
        high_band_ratio: high_energy / spectral_energy.max(f64::MIN_POSITIVE),
        time_to_peak_ms,
        decay_ratio: late_rms / early_rms.max(f64::MIN_POSITIVE),
    }
}

fn window_rms(samples: &[f64], start: usize, end: usize) -> f64 {
    slice_rms(&samples[start..end])
}

fn slice_rms(samples: &[f64]) -> f64 {
    (samples.iter().map(|sample| sample * sample).sum::<f64>() / samples.len() as f64).sqrt()
}

fn spectrum(samples: &[f64]) -> Vec<f64> {
    let mut real = vec![0.0; FFT_SIZE];
    let mut imaginary = vec![0.0; FFT_SIZE];
    for (index, sample) in samples.iter().copied().enumerate() {
        let window = 0.5 - 0.5 * (2.0 * PI * index as f64 / (FFT_SIZE - 1) as f64).cos();
        real[index] = sample * window;
    }

    let mut reversed = 0;
    for index in 1..FFT_SIZE {
        let mut bit = FFT_SIZE >> 1;
        while reversed & bit != 0 {
            reversed ^= bit;
            bit >>= 1;
        }
        reversed ^= bit;
        if index < reversed {
            real.swap(index, reversed);
            imaginary.swap(index, reversed);
        }
    }

    let mut width = 2;
    while width <= FFT_SIZE {
        let half = width / 2;
        let angle = -2.0 * PI / width as f64;
        let step_real = angle.cos();
        let step_imaginary = angle.sin();
        for start in (0..FFT_SIZE).step_by(width) {
            let mut twiddle_real = 1.0;
            let mut twiddle_imaginary = 0.0;
            for offset in 0..half {
                let even = start + offset;
                let odd = even + half;
                let odd_real = real[odd] * twiddle_real - imaginary[odd] * twiddle_imaginary;
                let odd_imaginary = real[odd] * twiddle_imaginary + imaginary[odd] * twiddle_real;
                real[odd] = real[even] - odd_real;
                imaginary[odd] = imaginary[even] - odd_imaginary;
                real[even] += odd_real;
                imaginary[even] += odd_imaginary;
                let next_real = twiddle_real * step_real - twiddle_imaginary * step_imaginary;
                twiddle_imaginary = twiddle_real * step_imaginary + twiddle_imaginary * step_real;
                twiddle_real = next_real;
            }
        }
        width *= 2;
    }

    (0..=FFT_SIZE / 2)
        .map(|index| real[index] * real[index] + imaginary[index] * imaginary[index])
        .collect()
}

#[test]
fn isolated_hits_have_distinct_timbres_and_plausible_envelopes() {
    let mut rendered = Vec::new();
    for (name, note, velocity) in [
        ("kick", 36, 0.9),
        ("snare", 38, 0.9),
        ("tom", 45, 0.9),
        ("closed_hat", 42, 0.9),
        ("open_hat", 46, 0.9),
        ("crash", 49, 0.9),
        ("ride", 51, 0.9),
    ] {
        let samples = render_note(note, velocity);
        let feature = features(&samples);
        println!("{name}: {feature:?}");
        assert!(samples.iter().all(|sample| sample.is_finite()));
        assert!(feature.rms > 1e-8, "{name} rendered silence");
        assert!(
            feature.peak < 0.999,
            "{name} is reaching the output limiter"
        );
        assert!(feature.centroid_hz.is_finite());
        assert!((0.0..=1.0).contains(&feature.high_band_ratio));
        assert!(feature.time_to_peak_ms <= 20.0, "{name} onset is too slow");
        rendered.push((name, feature));
    }

    let feature = |name| {
        rendered
            .iter()
            .find(|(candidate, _)| *candidate == name)
            .unwrap()
            .1
    };
    let kick = feature("kick");
    let snare = feature("snare");
    let tom = feature("tom");
    let closed_hat = feature("closed_hat");
    let open_hat = feature("open_hat");
    let crash = feature("crash");
    let ride = feature("ride");

    assert!(snare.centroid_hz > kick.centroid_hz * 1.4);
    assert!(tom.centroid_hz > kick.centroid_hz * 1.1);
    assert!(snare.high_band_ratio > kick.high_band_ratio + 0.03);
    assert!(closed_hat.centroid_hz > 2_000.0);
    assert!(closed_hat.high_band_ratio > 0.15);
    assert!(crash.centroid_hz > 1_500.0 && crash.high_band_ratio > 0.15);
    assert!(ride.centroid_hz > 1_000.0 && ride.high_band_ratio > 0.10);
    assert!(crash.centroid_hz > ride.centroid_hz * 1.1);
    assert!(crash.decay_ratio > ride.decay_ratio * 1.05);
    assert!(open_hat.decay_ratio > closed_hat.decay_ratio * 2.0 + 0.01);
}

#[test]
fn harder_hits_add_energy_and_brightness() {
    for note in [36, 38, 45] {
        let soft = features(&render_note(note, 0.25));
        let hard = features(&render_note(note, 0.9));
        println!("note {note}: soft={soft:?} hard={hard:?}");
        assert!(
            hard.rms > soft.rms * 1.01,
            "note {note} lost velocity response: soft RMS {}, hard RMS {}",
            soft.rms,
            hard.rms
        );
        assert!(
            hard.peak > soft.peak * 1.05,
            "note {note} lost attack response"
        );
        assert!(
            hard.centroid_hz >= soft.centroid_hz * 0.9,
            "note {note} gets markedly darker at higher velocity"
        );
    }
}
