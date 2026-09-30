use hound::{SampleFormat, WavSpec, WavWriter};
use physics_drum::{DrumEngine, DrumEvent};
use std::env;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: physics-drum-rs <seconds> <output.wav>");
        return Ok(());
    }
    let seconds: f64 = args[1].parse()?;
    let output = &args[2];
    let sample_rate = 48_000.0;
    let mut engine = DrumEngine::new(sample_rate);
    let events = [
        DrumEvent::NoteOn {
            time: 0,
            note: 36,
            velocity: 0.95,
        },
        DrumEvent::NoteOn {
            time: (0.25 * sample_rate) as usize,
            note: 38,
            velocity: 0.82,
        },
        DrumEvent::NoteOn {
            time: (0.5 * sample_rate) as usize,
            note: 46,
            velocity: 0.76,
        },
        DrumEvent::NoteOn {
            time: (0.75 * sample_rate) as usize,
            note: 42,
            velocity: 0.72,
        },
    ];
    let spec = WavSpec {
        channels: 2,
        sample_rate: sample_rate as u32,
        bits_per_sample: 24,
        sample_format: SampleFormat::Int,
    };
    let mut writer = WavWriter::create(output, spec)?;
    let total = (seconds.max(0.1) * sample_rate) as usize;
    let mut event_index = 0;
    for sample in 0..total {
        while event_index < events.len() && event_time(events[event_index]) <= sample {
            let event = events[event_index];
            if let DrumEvent::NoteOn { note, velocity, .. } = event {
                engine.trigger(note, velocity);
            }
            event_index += 1;
        }
        let (left, right) = engine.process_sample();
        writer.write_sample((left.clamp(-1.0, 1.0) * 8_388_607.0) as i32)?;
        writer.write_sample((right.clamp(-1.0, 1.0) * 8_388_607.0) as i32)?;
    }
    writer.finalize()?;
    Ok(())
}

fn event_time(event: DrumEvent) -> usize {
    match event {
        DrumEvent::NoteOn { time, .. }
        | DrumEvent::NoteOff { time, .. }
        | DrumEvent::HiHatOpen { time, .. } => time,
    }
}
