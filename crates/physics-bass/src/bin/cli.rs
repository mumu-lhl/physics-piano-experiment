use hound::{SampleFormat, WavSpec, WavWriter};
use physics_bass::{BassEngine, BassMode, PluckStyle};
use std::env;

fn midi_from_name(name: &str) -> Option<u8> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    let split = name.char_indices().find(|(_, c)| c.is_ascii_digit())?;
    let (pitch, octave) = name.split_at(split.0);
    let octave: i32 = octave.parse().ok()?;
    let semitone = match pitch.to_ascii_uppercase().as_str() {
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
        _ => return None,
    };
    u8::try_from((octave + 1) * 12 + semitone).ok()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 4 {
        eprintln!("usage: physics-bass-rs <note> <seconds> <output.wav> [velocity] [acoustic]");
        return Ok(());
    }
    let note = midi_from_name(&args[1]).ok_or("invalid note")?;
    let seconds: f64 = args[2].parse()?;
    let output = &args[3];
    let velocity: f64 = args.get(4).map(|v| v.parse()).transpose()?.unwrap_or(0.8);
    let acoustic = args.get(5).is_some_and(|value| value == "acoustic");
    let sample_rate = 48_000.0;
    let mut engine = BassEngine::new(
        sample_rate,
        if acoustic {
            BassMode::Acoustic
        } else {
            BassMode::Electric
        },
        false,
    );
    engine.set_pluck_style(if acoustic {
        PluckStyle::Finger
    } else {
        PluckStyle::Pick
    });
    engine.note_on(note, velocity);

    let spec = WavSpec {
        channels: 2,
        sample_rate: sample_rate as u32,
        bits_per_sample: 24,
        sample_format: SampleFormat::Int,
    };
    let mut writer = WavWriter::create(output, spec)?;
    let total = (seconds.max(0.01) * sample_rate) as usize;
    let release_at = (total as f64 * 0.72) as usize;
    for i in 0..total {
        if i == release_at {
            engine.note_off(note);
        }
        let (left, right) = engine.process_sample();
        writer.write_sample((left.clamp(-1.0, 1.0) * 8_388_607.0) as i32)?;
        writer.write_sample((right.clamp(-1.0, 1.0) * 8_388_607.0) as i32)?;
    }
    writer.finalize()?;
    Ok(())
}
