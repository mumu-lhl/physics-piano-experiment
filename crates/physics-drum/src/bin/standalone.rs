use nice_plug::wrapper::standalone::nice_export_standalone_with_args;
use physics_drum::nice_plugin::PhysicsDrum;

fn main() {
    let mut args: Vec<String> = std::env::args().collect();
    #[cfg(target_os = "linux")]
    {
        let help = args.iter().any(|arg| arg == "-h" || arg == "--help");
        if !help && !args.iter().any(|arg| arg == "-b" || arg == "--backend") {
            args.extend(["-b".into(), "alsa".into()]);
        }
        if !help && !args.iter().any(|arg| arg == "-p" || arg == "--period-size") {
            args.extend(["-p".into(), "1024".into()]);
        }
    }
    nice_export_standalone_with_args::<PhysicsDrum, _>(args);
}
