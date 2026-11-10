//! Klipt desktop app core. All logic lives here; the Svelte UI talks to it through Tauri commands.

pub mod errors;
pub mod feedback;
pub mod ffmpeg;
pub mod games;
pub mod hotkey;
pub mod library;
pub mod recorder;
pub mod settings;
pub mod sound;
pub mod state;
pub mod tray;

use tauri::{AppHandle, Manager, RunEvent, WindowEvent};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

/// Passed by the OS autostart entry: start hidden in the tray.
pub const AUTOSTART_ARG: &str = "--autostart";

pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec![AUTOSTART_ARG])));
    #[cfg(windows)]
    let builder = builder.plugin(tauri_plugin_global_shortcut::Builder::new().build());
    builder
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let state = state::AppState::init(data_dir.clone())?;
            let settings = state.settings.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone();
            app.manage(state);
            // The tray exists before the recorder so its first events reach the icon.
            tray::create(app.handle())?;
            app.manage(recorder::spawn_for_app(app.handle(), data_dir, &settings));
            apply_autostart(app.handle(), settings.start_with_os);
            // The window starts hidden (tauri.conf.json); an autostart launch stays in the tray.
            if !std::env::args().any(|arg| arg == AUTOSTART_ARG) {
                tray::show_main_window(app.handle());
            }
            hotkey::start_for_app(app.handle(), settings.hotkey.clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window hides it to the tray; "Quit" in the tray exits.
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == tray::MAIN_WINDOW {
                    api.prevent_close();
                    if let Err(e) = window.hide() {
                        eprintln!("klipt: can't hide the window: {e}");
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![recorder::save_clip])
        .build(tauri::generate_context!())
        .expect("error while building Klipt")
        .run(|app, event| {
            if let RunEvent::Exit = event {
                // Stop the recorder helper cleanly instead of leaving it to die with us.
                if let Some(recorder) = app.try_state::<recorder::SupervisorHandle>() {
                    recorder.shutdown();
                }
            }
        });
}

/// Turns starting with the OS on or off to match `startWithOs`. Debug builds leave it alone, so a
/// dev run doesn't register the dev binary to start at login.
fn apply_autostart(app: &AppHandle, enabled: bool) {
    if cfg!(debug_assertions) {
        return;
    }
    let autostart = app.autolaunch();
    let result = match autostart.is_enabled() {
        Ok(current) if current == enabled => Ok(()),
        _ if enabled => autostart.enable(),
        _ => autostart.disable(),
    };
    if let Err(e) = result {
        eprintln!("klipt: can't {} starting with the OS: {e}", if enabled { "turn on" } else { "turn off" });
    }
}
