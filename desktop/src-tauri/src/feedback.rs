//! What the user hears and sees when a save happens, even with the window hidden while gaming:
//! the chime, system notifications and the tray icon.

use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::recorder::SupervisorEvent;
use crate::state::AppState;
use crate::tray::Tray;

pub const BUSY_TEXT: &str = "Already saving…";

pub fn saved_text(game_name: Option<&str>) -> String {
    format!("Clip saved — {}", game_name.unwrap_or("Unknown game"))
}

pub fn recording_saved_text(game_name: Option<&str>) -> String {
    format!("Recording saved — {}", game_name.unwrap_or("Unknown game"))
}

pub const RECORDING_STARTED_TEXT: &str = "Recording started";

/// Errors that end or prevent a full recording: always told, the window is usually hidden.
pub fn is_recording_problem(code: &str) -> bool {
    matches!(code, "full_recording_lost" | "full_recording_failed")
}

/// The app icon, for notifications on Linux.
#[cfg(target_os = "linux")]
pub const APP_ICON: &[u8] = include_bytes!("../icons/128x128.png");

/// The desktop file the AUR package installs (packaging/aur); KDE takes its name and icon.
#[cfg(target_os = "linux")]
const DESKTOP_ENTRY: &str = "app.klipt.Klipt";

/// The app icon as a file in `data_dir`, written when missing or different. A file path works in
/// development too, where no icon is installed in the icon theme.
#[cfg(target_os = "linux")]
pub fn notification_icon(data_dir: &std::path::Path) -> std::io::Result<std::path::PathBuf> {
    let path = data_dir.join("notification-icon.png");
    if std::fs::read(&path).ok().as_deref() != Some(APP_ICON) {
        std::fs::create_dir_all(data_dir)?;
        std::fs::write(&path, APP_ICON)?;
    }
    Ok(path)
}

/// A Linux notification from "Klipt" with its icon. The notification plugin names the app after
/// the program file (klipt-desktop) and asks for an icon of that name, which doesn't exist.
#[cfg(target_os = "linux")]
pub fn linux_notification(title: &str, body: Option<&str>, icon: &std::path::Path) -> notify_rust::Notification {
    let mut n = notify_rust::Notification::new();
    n.appname("Klipt").summary(title).icon(&icon.to_string_lossy()).hint(notify_rust::Hint::DesktopEntry(DESKTOP_ENTRY.into()));
    if let Some(body) = body {
        n.body(body);
    }
    n
}

/// Shows a system notification; failures are only logged.
pub fn notify(app: &AppHandle, title: &str, body: Option<&str>) {
    #[cfg(target_os = "linux")]
    {
        let icon = app.path().app_data_dir().map_err(|e| e.to_string()).and_then(|dir| {
            notification_icon(&dir).map_err(|e| e.to_string())
        });
        match icon {
            Ok(icon) => {
                let n = linux_notification(title, body, &icon);
                // Sending talks to D-Bus; don't hold up the caller (often the recorder thread).
                std::thread::spawn(move || {
                    if let Err(e) = n.show() {
                        crate::elog!("klipt: can't show a notification: {e}");
                    }
                });
                return;
            }
            Err(e) => crate::elog!("klipt: no icon for notifications ({e}); using the default"),
        }
    }
    let mut builder = app.notification().builder().title(title);
    if let Some(body) = body {
        builder = builder.body(body);
    }
    if let Err(e) = builder.show() {
        crate::elog!("klipt: can't show a notification: {e}");
    }
}

/// A system notification with a "Copy link" button when `link` is given; clicking it puts the
/// link on the clipboard. Without a link it's an ordinary [`notify`].
pub fn notify_with_copy(app: &AppHandle, title: &str, body: &str, link: Option<String>) {
    let Some(link) = link else {
        notify(app, title, Some(body));
        return;
    };
    #[cfg(target_os = "linux")]
    let notification = {
        let icon = app.path().app_data_dir().map_err(|e| e.to_string()).and_then(|dir| notification_icon(&dir).map_err(|e| e.to_string()));
        match icon {
            Ok(icon) => linux_notification(title, Some(body), &icon),
            Err(e) => {
                crate::elog!("klipt: no icon for notifications ({e}); using the default");
                let mut n = notify_rust::Notification::new();
                n.appname("Klipt").summary(title).body(body);
                n
            }
        }
    };
    #[cfg(not(target_os = "linux"))]
    let notification = {
        let mut n = notify_rust::Notification::new();
        n.summary(title).body(body);
        // Windows names the app by its id only when installed (as tauri-plugin-notification does);
        // a development build shows as PowerShell.
        #[cfg(windows)]
        if !cfg!(debug_assertions) {
            n.app_id(&app.config().identifier);
        }
        n
    };
    let mut notification = notification;
    notification.action("copy", "Copy link");
    let app = app.clone();
    // Waiting for the click blocks: on its own thread, as long as the notification is up.
    std::thread::spawn(move || match notification.show() {
        Ok(handle) => handle.wait_for_action(|action| {
            if action == "copy" {
                if let Err(e) = crate::commands::copy_text(&app, &link) {
                    crate::elog!("klipt: can't copy the share link: {e}");
                }
            }
        }),
        Err(e) => crate::elog!("klipt: can't show a notification: {e}"),
    });
}

/// Reacts to a supervisor event before it goes to the UI.
pub fn on_supervisor_event(app: &AppHandle, event: &SupervisorEvent) {
    if let Some(tray) = app.try_state::<Tray>() {
        tray.on_supervisor_event(event);
    }
    let (sound, notification) = match app.try_state::<AppState>() {
        Some(state) => {
            let settings = state.settings.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            (settings.save_sound, settings.save_notification)
        }
        None => (false, false),
    };
    match event {
        SupervisorEvent::ClipSaved(view) => {
            if sound {
                crate::sound::play_saved();
            }
            if notification {
                let game = view.clip.game_name.as_deref();
                let text = if view.clip.is_recording { recording_saved_text(game) } else { saved_text(game) };
                notify(app, &text, None);
            }
        }
        SupervisorEvent::FullRecordingStarted { .. } if notification => notify(app, RECORDING_STARTED_TEXT, None),
        SupervisorEvent::Error { message, code } if is_recording_problem(code) => notify(app, "Klipt", Some(message)),
        SupervisorEvent::SaveBusy if notification => notify(app, BUSY_TEXT, None),
        // A press that saved nothing. The window is usually hidden in game, so the in-app toast
        // goes unseen; say so even when "Clip saved" notifications are off.
        SupervisorEvent::Error { code, message } if is_failed_save(code) => {
            notify(app, NOT_SAVED_TEXT, Some(message));
        }
        _ => {}
    }
}

pub const NOT_SAVED_TEXT: &str = "Clip not saved";

/// Errors that answer a save request (hotkey or tray) with "nothing was saved".
pub fn is_failed_save(code: &str) -> bool {
    matches!(code, "recording_paused" | "not_recording" | "recorder_starting" | "save_failed")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_notifications_name_klipt_and_show_its_icon() {
        let n = linux_notification("Recording started", None, std::path::Path::new("/data/notification-icon.png"));
        assert_eq!(n.appname, "Klipt");
        assert_eq!(n.summary, "Recording started");
        assert_eq!(n.icon, "/data/notification-icon.png");
        assert!(n.hints.contains(&notify_rust::Hint::DesktopEntry("app.klipt.Klipt".into())));
        let n = linux_notification("Klipt", Some("The recording stopped."), std::path::Path::new("/i.png"));
        assert_eq!(n.body, "The recording stopped.");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn the_notification_icon_file_matches_the_app_icon() {
        let dir = tempfile::tempdir().unwrap();
        let path = notification_icon(dir.path()).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), APP_ICON);
        std::fs::write(&path, b"an older icon").unwrap();
        assert_eq!(notification_icon(dir.path()).unwrap(), path);
        assert_eq!(std::fs::read(&path).unwrap(), APP_ICON, "a changed icon is written again");
    }

    #[test]
    fn saved_text_names_the_game_or_says_unknown() {
        assert_eq!(saved_text(Some("Active Matter")), "Clip saved — Active Matter");
        assert_eq!(saved_text(None), "Clip saved — Unknown game");
    }

    #[test]
    fn recording_saved_text_names_the_game() {
        assert_eq!(recording_saved_text(Some("Deadlock")), "Recording saved — Deadlock");
        assert_eq!(recording_saved_text(None), "Recording saved — Unknown game");
    }

    #[test]
    fn recording_problems_always_notify() {
        assert!(is_recording_problem("full_recording_lost"));
        assert!(is_recording_problem("full_recording_failed"));
        assert!(!is_recording_problem("save_failed"));
    }

    #[test]
    fn failed_saves_are_told_apart_from_other_recorder_errors() {
        for code in ["recording_paused", "not_recording", "save_failed"] {
            assert!(is_failed_save(code), "{code}");
        }
        for code in ["recording_stopped", "recorder_crashed", "recorder_missing"] {
            assert!(!is_failed_save(code), "{code}");
        }
    }
}
