//! Standalone Desktop Application with Hardware-Accelerated GUI and Audio/MIDI for Physics Piano.

use nih_plug::wrapper::standalone::nih_export_standalone_with_args;
use physics_piano::nih_plugin::PhysicsPiano;

fn main() {
    let mut args: Vec<String> = std::env::args().collect();

    // On Linux systems with modern audio servers (PipeWire / pipewire-jack), the default
    // auto-selected JACK backend frequently aborts due to dynamic quantum negotiation (e.g. 1024 -> 256).
    // Defaulting to ALSA allows pipewire-alsa to handle audio seamlessly with zero friction.
    #[cfg(target_os = "linux")]
    {
        let has_backend = args
            .iter()
            .any(|arg| arg == "-b" || arg == "--backend" || arg.starts_with("--backend="));
        let is_help = args.iter().any(|arg| arg == "-h" || arg == "--help");
        if !has_backend && !is_help {
            args.push("-b".to_string());
            args.push("alsa".to_string());
        }

        // Set period size to 1024 (21.3ms @ 48kHz) by default to prevent ALSA underruns (xruns)
        let has_period = args
            .iter()
            .any(|arg| arg == "-p" || arg == "--period-size" || arg.starts_with("--period-size="));
        if !has_period && !is_help {
            args.push("-p".to_string());
            args.push("1024".to_string());
        }
    }

    nih_export_standalone_with_args::<PhysicsPiano, _>(args);
}
