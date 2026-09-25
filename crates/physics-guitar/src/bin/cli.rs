use std::env;
use hound::{WavSpec, WavWriter, SampleFormat};
use physics_guitar::params::GuitarStringSetType;
use physics_guitar::engine::{GuitarEngine, GuitarInstrumentMode};

fn pitch_to_midi(name: &str) -> Option<u8> {
    let name = name.trim();
    if name.len() < 2 {
        return None;
    }
    let note_letter = &name[0..1].to_uppercase();
    let (accidental, octave_str) = if name.len() == 3 {
        (&name[1..2], &name[2..3])
    } else {
        ("", &name[1..2])
    };

    let semitone = match note_letter.as_str() {
        "C" => 0,
        "D" => 2,
        "E" => 4,
        "F" => 5,
        "G" => 7,
        "A" => 9,
        "B" => 11,
        _ => return None,
    };

    let acc_offset = match accidental {
        "#" | "s" => 1,
        "b" => -1,
        "" => 0,
        _ => return None,
    };

    let octave: i32 = octave_str.parse().ok()?;
    let midi = (octave + 1) * 12 + semitone + acc_offset;
    if (0..=127).contains(&midi) {
        Some(midi as u8)
    } else {
        None
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage:");
        eprintln!("  physics-guitar-rs render <pitch> <duration> <out.wav> [electric|acoustic]");
        eprintln!("  physics-guitar-rs strum <duration> <out.wav> [electric|acoustic]");
        return;
    }

    let sample_rate = 44100.0;
    let cmd = &args[1];

    if cmd == "render" {
        let pitch = args.get(2).map(|s| s.as_str()).unwrap_or("E2");
        let dur: f64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(3.0);
        let out_path = args.get(4).map(|s| s.as_str()).unwrap_or("guitar_pluck.wav");
        let mode_str = args.get(5).map(|s| s.as_str()).unwrap_or("electric");

        let mode = if mode_str == "acoustic" {
            GuitarInstrumentMode::Acoustic
        } else {
            GuitarInstrumentMode::Electric
        };

        let set_type = if mode == GuitarInstrumentMode::Acoustic {
            GuitarStringSetType::Acoustic012
        } else {
            GuitarStringSetType::Electric010
        };

        let midi_note = pitch_to_midi(pitch).unwrap_or(40);
        println!("Synthesizing guitar note: {} (MIDI {}) in {:?} mode for {:.1}s...", pitch, midi_note, mode, dur);

        let mut engine = GuitarEngine::new(sample_rate, set_type, mode);
        engine.note_on(1, midi_note, 0.85);

        let total_samples = (dur * sample_rate) as usize;
        let spec = WavSpec {
            channels: 2,
            sample_rate: sample_rate as u32,
            bits_per_sample: 16,
            sample_format: SampleFormat::Int,
        };

        let mut writer = WavWriter::create(out_path, spec).expect("Failed to create WAV file");
        for _ in 0..total_samples {
            let (l, r) = engine.process_sample();
            let sl = (l.clamp(-1.0, 1.0) * 32767.0) as i16;
            let sr = (r.clamp(-1.0, 1.0) * 32767.0) as i16;
            writer.write_sample(sl).unwrap();
            writer.write_sample(sr).unwrap();
        }
        writer.finalize().unwrap();
        println!("Rendered output saved to: {}", out_path);
    } else if cmd == "strum" {
        let dur: f64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(3.5);
        let out_path = args.get(3).map(|s| s.as_str()).unwrap_or("guitar_strum.wav");
        let mode_str = args.get(4).map(|s| s.as_str()).unwrap_or("electric");

        let mode = if mode_str == "acoustic" {
            GuitarInstrumentMode::Acoustic
        } else {
            GuitarInstrumentMode::Electric
        };

        let set_type = if mode == GuitarInstrumentMode::Acoustic {
            GuitarStringSetType::Acoustic012
        } else {
            GuitarStringSetType::Electric010
        };

        println!("Strumming E-minor chord (E2, B2, E3, G3, B3, E4) in {:?} mode...", mode);
        let mut engine = GuitarEngine::new(sample_rate, set_type, mode);

        // Open Em chord:
        // String 6 (E2, midi 40, fret 0)
        // String 5 (B2, midi 47, fret 2)
        // String 4 (E3, midi 52, fret 2)
        // String 3 (G3, midi 55, fret 0)
        // String 2 (B3, midi 59, fret 0)
        // String 1 (E4, midi 64, fret 0)
        let chord_notes = [40, 47, 52, 55, 59, 64];

        let total_samples = (dur * sample_rate) as usize;
        let spec = WavSpec {
            channels: 2,
            sample_rate: sample_rate as u32,
            bits_per_sample: 16,
            sample_format: SampleFormat::Int,
        };

        let mut writer = WavWriter::create(out_path, spec).expect("Failed to create WAV file");
        let strum_delay_samples = (0.025 * sample_rate) as usize; // 25ms pick stroke delay per string

        for n in 0..total_samples {
            // Trigger each string with progressive downward strum delay
            for (idx, &note) in chord_notes.iter().enumerate() {
                if n == idx * strum_delay_samples {
                    engine.note_on(1, note, 0.82);
                }
            }

            let (l, r) = engine.process_sample();
            let sl = (l.clamp(-1.0, 1.0) * 32767.0) as i16;
            let sr = (r.clamp(-1.0, 1.0) * 32767.0) as i16;
            writer.write_sample(sl).unwrap();
            writer.write_sample(sr).unwrap();
        }
        writer.finalize().unwrap();
        println!("Rendered strummed chord saved to: {}", out_path);
    }
}
