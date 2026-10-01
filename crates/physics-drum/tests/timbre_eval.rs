use physics_drum::DrumEngine;
use std::f64::consts::PI;
use std::fs;
use std::io::Cursor;
use std::path::PathBuf;

// Deterministic checks for isolated General MIDI hits, with one recorded kit as
// a broad sanity anchor rather than a universal acoustic ground truth.
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

fn read_reference_recording(name: &str) -> Vec<f64> {
    let reference_dir = std::env::var_os("DRUM_REFERENCE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/drum-reference/virtuosity")
        });
    let path = reference_dir.join(format!("{name}.wav"));
    read_reference_wav(&path)
}

fn read_stemgmd_recording(kit: &str, instrument: &str) -> Vec<f64> {
    let reference_dir = std::env::var_os("DRUM_STEMGMD_REFERENCE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/drum-reference/stemgmd")
        });
    let path = reference_dir.join(kit).join(format!("{instrument}.wav"));
    read_reference_wav(&path)
}

fn read_reference_wav(path: &PathBuf) -> Vec<f64> {
    let wav = fs::read(&path).unwrap_or_else(|error| {
        panic!(
            "missing drum reference {} ({error}); run `python3 tools/download_drum_references.py`",
            path.display()
        )
    });
    let mut reader = hound::WavReader::new(Cursor::new(wav)).expect("reference WAV should decode");
    let spec = reader.spec();
    assert!(matches!(spec.channels, 1 | 2));
    assert_eq!(spec.sample_rate, 44_100);
    assert_eq!(spec.bits_per_sample, 16);

    let interleaved: Vec<f64> = reader
        .samples::<i16>()
        .map(|sample| f64::from(sample.expect("valid reference PCM")) / f64::from(i16::MAX))
        .collect();
    let channel_count = usize::from(spec.channels);
    assert_eq!(interleaved.len() % channel_count, 0);
    let source: Vec<f64> = interleaved
        .chunks_exact(channel_count)
        .map(|frame| frame.iter().sum::<f64>() / channel_count as f64)
        .collect();
    let source_peak = source
        .iter()
        .fold(0.0_f64, |peak, sample| peak.max(sample.abs()));
    let onset = source
        .iter()
        .position(|sample| sample.abs() >= source_peak * 0.02)
        .expect("reference recording should contain an onset");

    let resampled_len =
        (source.len() as f64 * SAMPLE_RATE as f64 / f64::from(spec.sample_rate)).round() as usize;
    let mut resampled = Vec::with_capacity(resampled_len);
    for index in 0..resampled_len {
        let source_position = index as f64 * f64::from(spec.sample_rate) / SAMPLE_RATE as f64;
        let lower = source_position.floor() as usize;
        let upper = (lower + 1).min(source.len() - 1);
        let fraction = source_position - lower as f64;
        resampled.push(source[lower] * (1.0 - fraction) + source[upper] * fraction);
    }

    let onset = (onset as f64 * SAMPLE_RATE as f64 / f64::from(spec.sample_rate)).round() as usize;
    let aligned = &resampled[onset.min(resampled.len())..];
    let peak = aligned
        .iter()
        .fold(0.0_f64, |peak, sample| peak.max(sample.abs()));
    let mut output = vec![0.0; HIT_FRAMES];
    for (destination, source) in output.iter_mut().zip(aligned.iter().copied()) {
        *destination = source / peak.max(f64::MIN_POSITIVE);
    }
    output
}

fn normalized_features(samples: &[f64]) -> TimbreFeatures {
    let peak = samples
        .iter()
        .fold(0.0_f64, |peak, sample| peak.max(sample.abs()));
    let normalized: Vec<f64> = samples
        .iter()
        .map(|sample| sample / peak.max(f64::MIN_POSITIVE))
        .collect();
    features(&normalized)
}

fn median(values: &mut [f64]) -> f64 {
    values.sort_by(f64::total_cmp);
    let middle = values.len() / 2;
    if values.len() % 2 == 0 {
        (values[middle - 1] + values[middle]) * 0.5
    } else {
        values[middle]
    }
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
    assert!(crash.decay_ratio > ride.decay_ratio * 1.1);
    assert!((crash.centroid_hz - ride.centroid_hz).abs() > 500.0);
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

#[test]
fn acoustic_recordings_are_a_broad_timbre_reference() {
    for (name, note, reference_file) in [
        ("kick", 36, "kick"),
        ("snare", 38, "snare"),
        ("tom_low", 41, "tom_low"),
        ("tom_high", 50, "tom_high"),
        ("closed_hat", 42, "hihat_closed"),
        ("open_hat", 46, "hihat_open"),
        ("crash", 49, "crash"),
        ("ride", 51, "ride"),
    ] {
        let reference = normalized_features(&read_reference_recording(reference_file));
        let synthesized = normalized_features(&render_note(note, 0.65));
        let centroid_ratio = (synthesized.centroid_hz / reference.centroid_hz)
            .max(reference.centroid_hz / synthesized.centroid_hz);
        let high_band_delta = (synthesized.high_band_ratio - reference.high_band_ratio).abs();
        let decay_delta = (synthesized.decay_ratio - reference.decay_ratio).abs();
        println!(
            "{name}: centroid {} Hz vs {} Hz ({centroid_ratio:.2}x), high band Δ={high_band_delta:.3}, decay Δ={decay_delta:.3}",
            synthesized.centroid_hz, reference.centroid_hz
        );
        assert!(reference.rms.is_finite() && reference.rms > 0.0);
        assert!(reference.centroid_hz.is_finite());
        assert!((0.0..=1.0).contains(&reference.high_band_ratio));
        assert!(
            // The single jazz-kit anchor has substantially darker open hats
            // than the ten-kit median; the multi-kit test below owns cymbal
            // spectrum calibration.
            centroid_ratio <= 4.0,
            "{name} spectral centroid differs by {centroid_ratio:.2}x from the reference"
        );
        assert!(
            // This one jazz kit has noticeably darker hats than the ten-kit
            // sample-library median; keep it as an anchor without making it
            // override the multi-kit calibration.
            high_band_delta <= 0.55,
            "{name} high-band energy differs by {high_band_delta:.3} from the reference"
        );
        assert!(
            decay_delta <= 0.35,
            "{name} decay profile differs by {decay_delta:.3} from the reference"
        );
    }
}

#[test]
fn ten_sample_kits_report_cross_kit_timbre_deviation() {
    const KITS: [&str; 10] = [
        "bluebird",
        "brooklyn",
        "detroit_garage",
        "east_bay",
        "heavy",
        "motown_revisited",
        "portland",
        "retro_rock",
        "roots",
        "socal",
    ];

    for (instrument, note) in [
        ("kick", 36),
        ("snare", 38),
        ("low_tom", 41),
        ("mid_tom", 45),
        ("hi_tom", 50),
        ("hihat_closed", 42),
        ("hihat_open", 46),
        ("crash", 49),
        ("ride", 51),
    ] {
        let synthesized = normalized_features(&render_note(note, 0.65));
        let mut centroid_hz = Vec::with_capacity(KITS.len());
        let mut high_band_ratio = Vec::with_capacity(KITS.len());
        let mut decay_ratio = Vec::with_capacity(KITS.len());
        for kit in KITS {
            let reference = normalized_features(&read_stemgmd_recording(kit, instrument));
            centroid_hz.push(reference.centroid_hz);
            high_band_ratio.push(reference.high_band_ratio);
            decay_ratio.push(reference.decay_ratio);
        }
        let centroid_min = centroid_hz.iter().copied().fold(f64::INFINITY, f64::min);
        let centroid_max = centroid_hz
            .iter()
            .copied()
            .fold(f64::NEG_INFINITY, f64::max);
        let centroid_median = median(&mut centroid_hz);
        let high_band_median = median(&mut high_band_ratio);
        let decay_median = median(&mut decay_ratio);
        let centroid_ratio = (synthesized.centroid_hz / centroid_median)
            .max(centroid_median / synthesized.centroid_hz);
        let high_band_delta = (synthesized.high_band_ratio - high_band_median).abs();
        let decay_delta = (synthesized.decay_ratio - decay_median).abs();
        println!(
            "StemGMD {instrument}: synth centroid {:.0} Hz, kit median {:.0} Hz (range {:.0}–{:.0}); high band {:.3} vs {:.3}; decay {:.3} vs {:.3}",
            synthesized.centroid_hz,
            centroid_median,
            centroid_min,
            centroid_max,
            synthesized.high_band_ratio,
            high_band_median,
            synthesized.decay_ratio,
            decay_median,
        );
        assert!(centroid_ratio.is_finite());
        assert!((0.0..=1.0).contains(&synthesized.high_band_ratio));
        assert!(synthesized.decay_ratio.is_finite());
        assert!(
            centroid_ratio <= 2.2,
            "{instrument} centroid differs by {centroid_ratio:.2}x from the ten-kit median"
        );
        assert!(
            high_band_delta <= 0.18,
            "{instrument} high-band ratio differs by {high_band_delta:.3} from the ten-kit median"
        );
        assert!(
            decay_delta <= 0.21,
            "{instrument} decay ratio differs by {decay_delta:.3} from the ten-kit median"
        );
    }
}
