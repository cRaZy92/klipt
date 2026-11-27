//! Klipt desktop app core. All logic lives here; the Svelte UI talks to it through Tauri commands.

pub mod account;
pub mod activity;
pub mod cleanup;
pub mod combine;
pub mod copy_link;
pub mod commands;
pub mod diagnostics;
pub mod disk;
pub mod editor;
pub mod errors;
pub mod export;
pub mod feed_watch;
pub mod feedback;
pub mod filmstrip;
pub mod friends;
pub mod ffmpeg;
pub mod games;
pub mod hotkey;
pub mod import;
#[cfg(target_os = "linux")]
mod autostart_linux;
pub mod library;
pub mod logging;
pub mod media_server;
pub mod player_cache;
pub mod mp4_mute;
pub mod parts;
pub mod update;
pub mod recorder;
pub mod resources;
pub mod screenshot;
pub mod subtitles;
pub mod server_clips;
pub mod settings;
pub mod sound;
pub mod state;
pub mod thumb_image;
pub mod tray;
pub mod upload;
pub mod upload_notice;
pub mod upload_resume;

use tauri::{AppHandle, Manager, RunEvent, WindowEvent};
use tauri_plugin_autostart::MacosLauncher;
#[cfg(not(target_os = "linux"))]
use tauri_plugin_autostart::ManagerExt;

/// Passed by the OS autostart entry: start hidden in the tray.
pub const AUTOSTART_ARG: &str = "--autostart";

pub fn run() {
    let builder = tauri::Builder::default()
        // First, as its docs require: a second launch just shows this instance's window, unless
        // it came from a notification's Copy link button (copy_link).
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            if !copy_link::handle_args(app, &argv) {
                tray::show_main_window(app);
            }
        }))
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
            match logging::init(&data_dir.join("logs")) {
                Ok(()) => crate::elog!(
                    "klipt: Klipt {} starting on {} ({})",
                    app.package_info().version,
                    diagnostics::os_name(),
                    std::env::consts::ARCH
                ),
                Err(e) => crate::elog!("klipt: can't write the log file: {e}"),
            }
            let state = state::AppState::init(data_dir.clone())?;
            if let Some(notice) = &state.startup_notice {
                use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
                app.dialog().message(notice).title("Klipt").kind(MessageDialogKind::Warning).show(|_| {});
            }
            let settings = state.settings.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone();
            // Thumbnails (thumbs/ in the data dir, also in tauri.conf.json) and the clips folder
            // load in the UI through the asset protocol.
            commands::allow_clip_folder(app.handle(), &settings.output_dir);
            if let Err(e) = app.asset_protocol_scope().allow_directory(data_dir.join("thumbs"), false) {
                crate::elog!("klipt: can't allow thumbnails in the asset scope: {e}");
            }
            // Server clips play through a read-ahead cache (see player_cache); leftovers of a
            // download cut off by quitting are cleared, off the start-up path.
            let player_cache = player_cache::PlayerCache::new(
                app.path().app_cache_dir()?.join("player"),
                player_cache::PLAYER_CACHE_MAX_BYTES,
            );
            let sweep = player_cache.clone();
            std::thread::spawn(move || sweep.evict());
            app.manage(media_server::MediaServer::start(player_cache)?);
            app.manage(state);
            app.manage(commands::Uploads::default());
            app.manage(upload_notice::UploadBatch::default());
            app.manage(activity::Reporter::default());
            app.manage(filmstrip::Jobs::default());
            app.manage(hotkey::HotkeyStatusState::default());
            // Development builds start in the tray, like an autostart launch, while
            // `<app data>/dev-start-hidden` exists: rebuilds don't pop the window over a game.
            let dev_start_hidden = cfg!(debug_assertions) && data_dir.join("dev-start-hidden").exists();
            // The tray exists before the recorder so its first events reach the icon.
            tray::create(app.handle())?;
            app.manage(recorder::spawn_for_app(app.handle(), data_dir, &settings));
            apply_autostart(app.handle(), settings.start_with_os);
            // The window starts hidden (tauri.conf.json); an autostart launch stays in the tray.
            app.manage(tray::WindowRelease::default());
            // Started by a notification's Copy link button: copy it and stay in the tray.
            copy_link::register_scheme();
            let args: Vec<String> = std::env::args().collect();
            let copy_request = copy_link::handle_args(app.handle(), &args);
            if !dev_start_hidden && !copy_request && !args.iter().any(|arg| arg == AUTOSTART_ARG) {
                tray::show_main_window(app.handle());
            } else {
                tray::hide_main_window(app.handle());
            }
            app.manage(hotkey::start_for_app(app.handle(), hotkey::Keys::from_settings(&settings)));
            update::start(app.handle());
            cleanup::start(app.handle());
            feed_watch::start(app.handle());
            app.manage(upload_resume::Resuming::default());
            upload_resume::start(app.handle());
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the window hides it to the tray; "Quit" in the tray exits.
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == tray::MAIN_WINDOW {
                    api.prevent_close();
                    tray::hide_main_window(window.app_handle());
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            recorder::save_clip,
            commands::get_settings,
            commands::set_settings,
            commands::get_account,
            commands::check_server,
            commands::get_default_server,
            commands::open_signup,
            commands::login,
            commands::logout,
            commands::get_hotkey_status,
            commands::list_displays,
            commands::list_audio_devices,
            commands::open_clips_folder,
            commands::clips_folder_size,
            commands::save_clip_now,
            commands::toggle_full_recording,
            upload_resume::pending_uploads,
            upload_resume::resume_pending_uploads,
            commands::get_full_recording,
            commands::take_screenshot,
            commands::list_clips,
            commands::list_games,
            commands::rename_clip,
            commands::set_clip_game,
            commands::set_clip_favorite,
            commands::delete_clip,
            commands::show_in_folder,
            commands::clip_preview_url,
            commands::player_video_url,
            commands::copy_share_link,
            commands::copy_text_to_clipboard,
            commands::feed_clips,
            commands::feed_games,
            commands::my_storage,
            commands::list_users,
            commands::open_website,
            server_clips::my_uploads,
            server_clips::update_upload,
            thumb_image::set_thumbnail_image,
            thumb_image::set_upload_thumbnail,
            server_clips::delete_upload,
            server_clips::download_upload,
            server_clips::record_view,
            server_clips::set_server_favorite,
            server_clips::list_albums,
            server_clips::create_album,
            server_clips::set_album_clip,
            server_clips::get_album,
            server_clips::update_album,
            server_clips::delete_album,
            import::check_import,
            import::import_folder,
            import::check_drop,
            import::import_drop,
            import::cancel_import,
            feed_watch::feed_new_count,
            feed_watch::mark_feed_seen,
            server_clips::clip_viewers,
            update::install_update,
            update::check_for_update,
            update::app_info,
            commands::upload_clip,
            commands::open_editor,
            commands::close_editor,
            commands::save_edit,
            commands::export_copy,
            commands::make_part_clip,
            commands::combine_clips,
            resources::resource_usage,
            tray::release_window,
            update::pending_update,
            account::refresh_account,
            account::update_profile,
            account::set_profile_visibility,
            friends::list_friends,
            friends::add_friend,
            friends::remove_friend,
            friends::make_referral,
            friends::list_referrals,
            friends::revoke_referral,
            account::change_password,
            account::system_info,
            account::send_feedback,
            account::my_achievements,
            account::set_looks,
            account::set_avatar,
            account::remove_avatar,
            account::read_picture,
            account::open_logs_folder,
            hotkey::apps::shortcut_apps,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Klipt")
        .run(|app, event| {
            // Releasing the hidden window closes the last one; Klipt stays in the tray. Quit from
            // the tray exits with a code, which goes through.
            if let RunEvent::ExitRequested { code: None, api, .. } = &event {
                api.prevent_exit();
            }
            if let RunEvent::Exit = event {
                // Stop the recorder helper cleanly instead of leaving it to die with us.
                if let Some(recorder) = app.try_state::<recorder::SupervisorHandle>() {
                    recorder.shutdown();
                }
                // And the editor's filmstrip ffmpeg, which would otherwise run on after we quit.
                if let Some(jobs) = app.try_state::<filmstrip::Jobs>() {
                    jobs.stop_all();
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
    #[cfg(target_os = "linux")]
    let result = autostart_linux::apply(enabled).map_err(|e| e.to_string());
    #[cfg(not(target_os = "linux"))]
    let result = {
        let autostart = app.autolaunch();
        match autostart.is_enabled() {
            _ if enabled => autostart.enable(),
            Ok(false) => Ok(()),
            _ => autostart.disable(),
        }
        .map_err(|e| e.to_string())
    };
    #[cfg(target_os = "linux")]
    let _ = app;
    if let Err(e) = result {
        crate::elog!("klipt: can't {} starting with the OS: {e}", if enabled { "turn on" } else { "turn off" });
    }
}
