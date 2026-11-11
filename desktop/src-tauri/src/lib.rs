//! Klipt desktop app core. All logic lives here; the Svelte UI talks to it through Tauri commands.

pub mod commands;
pub mod editor;
pub mod errors;
pub mod export;
pub mod feedback;
pub mod ffmpeg;
pub mod games;
pub mod hotkey;
pub mod library;
pub mod media_server;
pub mod update;
pub mod recorder;
pub mod settings;
pub mod sound;
pub mod state;
pub mod tray;
pub mod upload;

use tauri::{AppHandle, Manager, RunEvent, WindowEvent};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

/// Passed by the OS autostart entry: start hidden in the tray.
pub const AUTOSTART_ARG: &str = "--autostart";

pub fn run() {
    let builder = tauri::Builder::default()
        // First, as its docs require: a second launch just shows this instance's window.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| tray::show_main_window(app)))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec![AUTOSTART_ARG])));
    #[cfg(windows)]
    let builder = builder
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_updater::Builder::new().build());
    builder
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let state = state::AppState::init(data_dir.clone())?;
            let settings = state.settings.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone();
            // Thumbnails (thumbs/ in the data dir, also in tauri.conf.json) and the clips folder
            // load in the UI through the asset protocol.
            commands::allow_clip_folder(app.handle(), &settings.output_dir);
            if let Err(e) = app.asset_protocol_scope().allow_directory(data_dir.join("thumbs"), false) {
                eprintln!("klipt: can't allow thumbnails in the asset scope: {e}");
            }
            app.manage(media_server::MediaServer::start()?);
            app.manage(state);
            app.manage(commands::Uploads::default());
            app.manage(hotkey::HotkeyStatusState::default());
            // The tray exists before the recorder so its first events reach the icon.
            tray::create(app.handle())?;
            app.manage(recorder::spawn_for_app(app.handle(), data_dir, &settings));
            apply_autostart(app.handle(), settings.start_with_os);
            // The window starts hidden (tauri.conf.json); an autostart launch stays in the tray.
            if !std::env::args().any(|arg| arg == AUTOSTART_ARG) {
                tray::show_main_window(app.handle());
            }
            app.manage(hotkey::start_for_app(app.handle(), settings.hotkey.clone()));
            update::start(app.handle());
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
        .invoke_handler(tauri::generate_handler![
            recorder::save_clip,
            commands::get_settings,
            commands::set_settings,
            commands::get_account,
            commands::check_server,
            commands::login,
            commands::logout,
            commands::get_hotkey_status,
            commands::list_displays,
            commands::list_audio_devices,
            commands::open_clips_folder,
            commands::save_clip_now,
            commands::list_clips,
            commands::list_games,
            commands::rename_clip,
            commands::set_clip_game,
            commands::delete_clip,
            commands::show_in_folder,
            commands::copy_share_link,
            commands::copy_text_to_clipboard,
            commands::feed_clips,
            commands::feed_games,
            commands::open_website,
            update::install_update,
            commands::upload_clip,
            commands::open_editor,
            commands::save_edit,
            commands::export_copy,
        ])
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

/// Turns starting with the OS on or off to match `startWithOs` (at launch and when Settings
/// change it). Debug builds leave it alone, so a dev run doesn't register the dev binary to
/// start at login. When on, the entry is rewritten every time, so it points at the copy that's
/// running (e.g. the packaged `/usr/bin/klipt-desktop` after a build from the repo set it up).
pub fn apply_autostart(app: &AppHandle, enabled: bool) {
    if cfg!(debug_assertions) {
        return;
    }
    let autostart = app.autolaunch();
    let result = match autostart.is_enabled() {
        _ if enabled => autostart.enable(),
        Ok(false) => Ok(()),
        _ => autostart.disable(),
    };
    if let Err(e) = result {
        eprintln!("klipt: can't {} starting with the OS: {e}", if enabled { "turn on" } else { "turn off" });
    }
}
