// No console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Linux: WebKitGTK caps a page at 30 fps while the system power profile is "power-saver"
    // (it asks GLib's power profile monitor). Measured on the owner's machine: 31 fps in
    // power-saver, 60 fps otherwise, so the whole UI felt like a laggy website. "none" makes GLib
    // fall back to a monitor that never reports power-saver. Set before any thread starts; the
    // web process inherits it. A value already in the environment wins.
    #[cfg(target_os = "linux")]
    if std::env::var_os("GIO_USE_POWER_PROFILE_MONITOR").is_none() {
        std::env::set_var("GIO_USE_POWER_PROFILE_MONITOR", "none");
    }
    klipt_desktop_lib::run()
}
