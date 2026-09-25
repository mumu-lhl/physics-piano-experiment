//! Standalone High-Performance CLI for Physics Piano in Rust.

use std::env;
use std::time::Instant;
use hound::{WavSpec, WavWriter, SampleFormat};
use physics_piano::engine::PianoEngine;

fn print_usage() {
    println!("Physics Piano (Rust High-Performance Physical Modeling Engine)");
    println!("Usage:");
    println!("  physics-piano-rs render <note> <duration_sec> <output.wav> [velocity]");
    println!("  physics-piano-rs benchmark [num_modes]");
    println!("Examples:");
    println!("  physics-piano-rs render A4 3.0 output_a4.wav 0.85");
    println!("  physics-piano-rs benchmark 35");
}

fn note_to_midi(note: &str) -> u8 {
    let note = note.trim();
    let (name, oct_str) = if note.len() >= 3 && (note.chars().nth(1) == Some('#') || note.chars().nth(1) == Some('b')) {
        (&note[0..2], &note[2..])
    } else {
        (&note[0..1], &note[1..])
    };

    let base_idx = match name.to_uppercase().as_str() {
        "C" => 0, "C#" | "DB" => 1, "D" => 2, "D#" | "EB" => 3,
        "E" => 4, "F" => 5, "F#" | "GB" => 6, "G" => 7,
        "G#" | "AB" => 8, "A" => 9, "A#" | "BB" => 10, "B" => 11,
        _ => 9,
    };
    let oct: i32 = oct_str.parse().unwrap_or(4);
    ((oct + 1) * 12 + base_idx) as u8
}

fn cmd_render(note_name: &str, duration: f64, out_path: &str, velocity: f64) {
    let sample_rate = 48000.0;
    let mut engine = PianoEngine::new(sample_rate, 35, false);
    let key = note_to_midi(note_name);

    println!("[*] Rendering physical piano note: {} (MIDI {}) in Rust...", note_name, key);
    println!("    Sample Rate: {} Hz, Duration: {:.2}s, Velocity: {:.2}", sample_rate, duration, velocity);

    let t0 = Instant::now();
    engine.note_on(key, velocity);
    let (left, right) = engine.render(duration);
    let elapsed = t0.elapsed().as_secs_f64();
    let rtf = elapsed / duration;

    println!("[+] Synthesis complete in {:.4}s (RTF: {:.2}x, {:.1}% of realtime)", elapsed, rtf, rtf * 100.0);

    // Normalize audio
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
    println!("[+] Saved 16-bit PCM WAV: {}", out_path);
}

fn cmd_benchmark(num_modes: usize) {
    let sample_rate = 48000.0;
    let test_keys = ["C2", "C3", "C4", "C5", "C6"];
    let dur = 1.0;

    println!("[*] Running Rust physical engine throughput benchmark (modes={})...", num_modes);

    let mut total_time = 0.0;
    for &note in &test_keys {
        let mut engine = PianoEngine::new(sample_rate, num_modes, false);
        let key = note_to_midi(note);
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

    match args[1].as_str() {
        "render" => {
            let note = if args.len() > 2 { &args[2] } else { "A4" };
            let duration: f64 = if args.len() > 3 { args[3].parse().unwrap_or(3.0) } else { 3.0 };
            let out_path = if args.len() > 4 { &args[4] } else { "rust_output.wav" };
            let velocity: f64 = if args.len() > 5 { args[5].parse().unwrap_or(0.85) } else { 0.85 };
            cmd_render(note, duration, out_path, velocity);
        }
        "benchmark" => {
            let modes: usize = if args.len() > 2 { args[2].parse().unwrap_or(35) } else { 35 };
            cmd_benchmark(modes);
        }
        _ => {
            print_usage();
        }
    }
}
