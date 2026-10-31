//! Standalone High-Performance CLI for Physics Piano in Rust.

use hound::{SampleFormat, WavSpec, WavWriter};
use physics_piano::engine::PianoEngine;
use std::env;
use std::time::Instant;

fn usage_lines() -> [&'static str; 10] {
    [
        "Physics Piano (Rust High-Performance Physical Modeling Engine)",
        "Usage:",
        "  physics-piano-rs render <note> <duration_sec> <output.wav> [velocity] [--sustain] [--upols]",
        "  physics-piano-rs chord <notes_comma_separated> <duration_sec> <output.wav> [velocity] [--sustain] [--upols]",
        "  physics-piano-rs benchmark [num_modes]",
        "Examples:",
        "  physics-piano-rs render A4 3.0 output_a4.wav 0.85",
        "  physics-piano-rs render A0 4.0 bass_a0.wav 0.90",
        "  physics-piano-rs chord \"C4,E4,G4,C5\" 4.0 chord.wav 0.85 --sustain",
        "  physics-piano-rs benchmark 35",
    ]
}

fn print_usage() {
    for line in usage_lines() {
        println!("{line}");
    }
}

/// Report a bad argument on stderr with the usage text and exit nonzero,
/// instead of silently substituting a default value.
fn fail_with_usage(message: String) -> ! {
    eprintln!("error: {message}");
    for line in usage_lines() {
        eprintln!("{line}");
    }
    std::process::exit(2);
}

fn parse_f64_arg(value: &str, name: &str) -> f64 {
    match value.parse::<f64>() {
        Ok(parsed) if parsed.is_finite() => parsed,
        _ => fail_with_usage(format!(
            "invalid {name} {value:?}: expected a finite number"
        )),
    }
}

fn parse_usize_arg(value: &str, name: &str) -> usize {
    match value.parse::<usize>() {
        Ok(parsed) => parsed,
        Err(_) => fail_with_usage(format!(
            "invalid {name} {value:?}: expected a non-negative integer"
        )),
    }
}

fn note_to_midi(note: &str) -> Option<u8> {
    let note = note.trim();
    if note.is_empty() || !note.is_ascii() {
        return None;
    }
    let (name, oct_str) = if note.len() >= 3
        && (note.chars().nth(1) == Some('#') || note.chars().nth(1) == Some('b'))
    {
        (&note[0..2], &note[2..])
    } else {
        (&note[0..1], &note[1..])
    };

    let base_idx = match name.to_uppercase().as_str() {
        "C" => 0,
        "C#" | "DB" => 1,
        "D" => 2,
        "D#" | "EB" => 3,
        "E" => 4,
        "F" => 5,
        "F#" | "GB" => 6,
        "G" => 7,
        "G#" | "AB" => 8,
        "A" => 9,
        "A#" | "BB" => 10,
        "B" => 11,
        _ => 9,
    };
    let oct: i32 = oct_str.parse().ok()?;
    Some(((oct + 1) * 12 + base_idx).clamp(21, 108) as u8)
}

fn save_wav(out_path: &str, sample_rate: f64, left: &[f64], right: &[f64]) {
    let mut peak = 1e-6f64;
    for (&l, &r) in left.iter().zip(right.iter()) {
        peak = peak.max(l.abs()).max(r.abs());
    }

    let spec = WavSpec {
        channels: 2,
        sample_rate: sample_rate as u32,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };

    let mut writer = WavWriter::create(out_path, spec).expect("Failed to create WAV file");
    let scale = 0.92 / peak * 32767.0;

    for (&l, &r) in left.iter().zip(right.iter()) {
        let sample_l = (l * scale).clamp(-32768.0, 32767.0) as i16;
        let sample_r = (r * scale).clamp(-32768.0, 32767.0) as i16;
        writer.write_sample(sample_l).unwrap();
        writer.write_sample(sample_r).unwrap();
    }
    writer.finalize().unwrap();
    println!("[+] Saved 16-bit PCM Stereo WAV: {}", out_path);
}

fn cmd_render(
    note_name: &str,
    duration: f64,
    out_path: &str,
    velocity: f64,
    sustain: bool,
    upols: bool,
) {
    let sample_rate = 48000.0;
    let mut engine = PianoEngine::new(sample_rate, 35, true);
    if upols {
        engine.set_radiation_mode("upols");
    }
    if sustain {
        engine.set_sustain_pedal(true, 1.0);
    }

    let key = match note_to_midi(note_name) {
        Some(key) => key,
        None => fail_with_usage(format!(
            "invalid note {note_name:?}: expected a note name like A4 or C#3"
        )),
    };
    println!(
        "[*] Rendering physical piano note: {} (MIDI {}) in Rust...",
        note_name, key
    );
    println!(
        "    Sample Rate: {} Hz, Duration: {:.2}s, Velocity: {:.2}, Sustain: {}",
        sample_rate, duration, velocity, sustain
    );

    let t0 = Instant::now();
    engine.note_on(key, velocity);
    let (left, right) = engine.render(duration);
    let elapsed = t0.elapsed().as_secs_f64();
    let rtf = elapsed / duration;

    println!(
        "[+] Synthesis complete in {:.4}s (RTF: {:.2}x, {:.1}% of realtime)",
        elapsed,
        rtf,
        rtf * 100.0
    );
    save_wav(out_path, sample_rate, &left, &right);
}

fn cmd_chord(
    chord_notes: &str,
    duration: f64,
    out_path: &str,
    velocity: f64,
    sustain: bool,
    upols: bool,
) {
    let sample_rate = 48000.0;
    let mut engine = PianoEngine::new(sample_rate, 35, true);
    if upols {
        engine.set_radiation_mode("upols");
    }
    if sustain {
        engine.set_sustain_pedal(true, 1.0);
    }

    let notes: Vec<&str> = chord_notes
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    let keys: Vec<u8> = notes
        .iter()
        .map(|n| match note_to_midi(n) {
            Some(key) => key,
            None => fail_with_usage(format!(
                "invalid note {n:?} in chord: expected note names like C4,E4,G4"
            )),
        })
        .collect();

    println!("[*] Rendering physical piano chord: {:?} in Rust...", notes);
    println!(
        "    Sample Rate: {} Hz, Duration: {:.2}s, Velocity: {:.2}, Sustain: {}",
        sample_rate, duration, velocity, sustain
    );

    let t0 = Instant::now();
    for &k in &keys {
        engine.note_on(k, velocity);
    }
    let (left, right) = engine.render(duration);
    let elapsed = t0.elapsed().as_secs_f64();
    let rtf = elapsed / duration;

    println!(
        "[+] Synthesis complete in {:.4}s (RTF: {:.2}x, {:.1}% of realtime)",
        elapsed,
        rtf,
        rtf * 100.0
    );
    save_wav(out_path, sample_rate, &left, &right);
}

fn cmd_benchmark(num_modes: usize) {
    let sample_rate = 48000.0;
    let test_keys = ["C2", "C3", "C4", "C5", "C6"];
    let dur = 1.0;

    println!(
        "[*] Running Rust physical engine throughput benchmark (modes={})...",
        num_modes
    );

    let mut total_time = 0.0;
    for &note in &test_keys {
        let mut engine = PianoEngine::new(sample_rate, num_modes, true);
        let key = match note_to_midi(note) {
            Some(key) => key,
            None => fail_with_usage(format!(
                "invalid note {note:?}: expected a note name like A4 or C#3"
            )),
        };
        let t0 = Instant::now();
        engine.note_on(key, 0.85);
        let _ = engine.render(dur);
        let dt = t0.elapsed().as_secs_f64();
        total_time += dt;
        println!("    - {:<3}: {:.4}s (RTF: {:.2}x)", note, dt, dt / dur);
    }

    let avg_rtf = total_time / (test_keys.len() as f64 * dur);
    let total_samples = test_keys.len() as f64 * dur * sample_rate;
    let throughput = total_samples / total_time;

    println!("[+] Total Elapsed: {:.4}s", total_time);
    println!("[+] Average Real-time Factor (RTF): {:.2}x", avg_rtf);
    println!("[+] Synthesis Throughput: {:.0} samples/sec", throughput);
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        print_usage();
        return;
    }

    let has_flag = |flag: &str| -> bool { args.iter().any(|a| a == flag) };

    match args[1].as_str() {
        "render" => {
            let note = if args.len() > 2 { &args[2] } else { "A4" };
            let duration: f64 = if args.len() > 3 {
                parse_f64_arg(&args[3], "duration_sec")
            } else {
                3.0
            };
            let out_path = if args.len() > 4 {
                &args[4]
            } else {
                "rust_output.wav"
            };
            let velocity: f64 = if args.len() > 5 {
                parse_f64_arg(&args[5], "velocity")
            } else {
                0.85
            };
            let sustain = has_flag("--sustain");
            let upols = has_flag("--upols");
            cmd_render(note, duration, out_path, velocity, sustain, upols);
        }
        "chord" => {
            let chord_str = if args.len() > 2 { &args[2] } else { "C4,E4,G4" };
            let duration: f64 = if args.len() > 3 {
                parse_f64_arg(&args[3], "duration_sec")
            } else {
                4.0
            };
            let out_path = if args.len() > 4 {
                &args[4]
            } else {
                "rust_chord.wav"
            };
            let velocity: f64 = if args.len() > 5 {
                parse_f64_arg(&args[5], "velocity")
            } else {
                0.85
            };
            let sustain = has_flag("--sustain");
            let upols = has_flag("--upols");
            cmd_chord(chord_str, duration, out_path, velocity, sustain, upols);
        }
        "benchmark" => {
            let modes: usize = if args.len() > 2 {
                parse_usize_arg(&args[2], "num_modes")
            } else {
                35
            };
            cmd_benchmark(modes);
        }
        _ => {
            print_usage();
        }
    }
}
