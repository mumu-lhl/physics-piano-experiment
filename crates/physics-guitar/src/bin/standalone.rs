//! Standalone Desktop Application with Hardware-Accelerated GUI and Audio/MIDI for Physics Guitar.

use nih_plug::wrapper::standalone::nih_export_standalone_with_args;
use physics_guitar::nih_plugin::PhysicsGuitar;

fn main() {
    let mut args: Vec<String> = std::env::args().collect();

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

        let has_period = args
            .iter()
            .any(|arg| arg == "-p" || arg == "--period-size" || arg.starts_with("--period-size="));
        if !has_period && !is_help {
            args.push("-p".to_string());
            args.push("1024".to_string());
        }
    }

    nih_export_standalone_with_args::<PhysicsGuitar, _>(args);
}
