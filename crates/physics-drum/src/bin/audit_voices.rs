use hound::{SampleFormat, WavSpec, WavWriter};
use physics_drum::DrumEngine;
use std::fs;
use std::path::Path;

const PADS: [(u8, &str); 10] = [
    (36, "kick"),
    (38, "snare"),
    (42, "hihat_closed"),
    (46, "hihat_open"),
    (44, "hihat_pedal"),
    (50, "tom_high"),
    (45, "tom_mid"),
    (41, "tom_floor"),
    (49, "cymbal_crash"),
    (51, "cymbal_ride"),
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output_dir = std::env::args().nth(1).map(std::path::PathBuf::from)
        .or_else(|| std::env::var("AUDIT_OUT_DIR").ok().map(std::path::PathBuf::from))
        .unwrap_or_else(|| {
            let brain_path = Path::new("/home/mumulhl/.gemini/antigravity-cli/brain/fce7533b-06fe-4465-a2c5-2abe23665bdb/audio/audit");
            if brain_path.parent().map_or(false, |p| p.exists()) {
                brain_path.to_path_buf()
            } else {
                std::path::PathBuf::from("target/audio_audit")
            }
        });
    fs::create_dir_all(&output_dir)?;

    let sample_rate = 48_000.0;
    let spec = WavSpec {
        channels: 2,
        sample_rate: sample_rate as u32,
        bits_per_sample: 24,
        sample_format: SampleFormat::Int,
    };

    println!("=== AUDITING ALL 10 DRUM KIT VOICES (sr = 48kHz, vel = 0.88) ===");

    for (note, name) in PADS {
        let mut engine = DrumEngine::new(sample_rate);
        engine.set_master_gain(1.0);
        // Note: GUI shortcut uses velocity 0.88
        engine.trigger(note, 0.88);

        let duration_secs = 2.0;
        let total_samples = (duration_secs * sample_rate) as usize;
        let file_path = output_dir.join(format!("{}_{}.wav", note, name));
        let mut writer = WavWriter::create(&file_path, spec)?;

        let mut samples_mono = Vec::with_capacity(total_samples);
        let mut peak_val = 0.0_f64;
        let mut sum_sq = 0.0_f64;

        let mut total_active_wires = 0usize;
        for sample_idx in 0..total_samples {
            if note == 38 && sample_idx < 4096 {
                for wire in &engine.snare.wires {
                    if wire.velocity.abs() > 0.001 {
                        total_active_wires += 1;
                    }
                }
            }

            let (left, right) = engine.process_sample();
            let mono = 0.5 * (left + right);


            samples_mono.push(mono);
            peak_val = peak_val.max(left.abs()).max(right.abs());
            sum_sq += mono * mono;

            writer.write_sample((left.clamp(-1.0, 1.0) * 8_388_607.0) as i32)?;
            writer.write_sample((right.clamp(-1.0, 1.0) * 8_388_607.0) as i32)?;
        }
        writer.finalize()?;

        let rms = (sum_sq / total_samples as f64).sqrt();
        let dc = samples_mono.iter().sum::<f64>() / total_samples as f64;

        // timbre_eval window decay: early = 50ms..150ms, late = 350ms..450ms
        let te_early_s = (0.050 * sample_rate) as usize;
        let te_early_e = (0.150 * sample_rate) as usize;
        let te_early_rms = (samples_mono[te_early_s..te_early_e].iter().map(|s| s * s).sum::<f64>() / (te_early_e - te_early_s) as f64).sqrt();
        let te_late_s = (0.350 * sample_rate) as usize;
        let te_late_e = (0.450 * sample_rate) as usize;
        let te_late_rms = (samples_mono[te_late_s..te_late_e].iter().map(|s| s * s).sum::<f64>() / (te_late_e - te_late_s) as f64).sqrt();
        let te_decay_ratio = te_late_rms / te_early_rms.max(1e-9);

        println!(
            "[{:2}] {:14}: peak = {:.4} ({:6.1} dBFS), rms = {:.4} ({:6.1} dBFS), te_decay = {:.4} (early={:.4}, late={:.4}), dc = {:.6}",
            note,
            name,
            peak_val,
            20.0 * peak_val.max(1e-9).log10(),
            rms,
            20.0 * rms.max(1e-9).log10(),
            te_decay_ratio,
            te_early_rms,
            te_late_rms,
            dc
        );
        if note == 38 {
            println!("   -> Snare active wire count in first 4096 samples: {}", total_active_wires);
        }
    }

    Ok(())
}
