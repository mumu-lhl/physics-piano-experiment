use nice_plug::wrapper::standalone::nice_export_standalone_with_args;
use physics_bass::nice_plugin::PhysicsBass;
use physics_presets::device_settings;

fn main() {
    // If this process is a relaunch (see `device_settings::spawn_restart`),
    // wait before opening devices so the old process releases them first.
    // Zero when the env var is absent, so normal launches are unaffected.
    std::thread::sleep(device_settings::restart_delay());
    device_settings::mark_standalone();

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
    // Saved device selections from the in-app selectors take effect on the
    // next launch; explicit device flags on the command line still win.
    device_settings::inject_saved_devices(&mut args, "bass");

    nice_export_standalone_with_args::<PhysicsBass, _>(args);
}
