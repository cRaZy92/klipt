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

/// Shows a system notification; failures are only logged.
pub fn notify(app: &AppHandle, title: &str, body: Option<&str>) {
    let mut builder = app.notification().builder().title(title);
    if let Some(body) = body {
        builder = builder.body(body);
    }
    if let Err(e) = builder.show() {
        eprintln!("klipt: can't show a notification: {e}");
    }
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
                notify(app, &saved_text(view.clip.game_name.as_deref()), None);
            }
        }
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
    matches!(code, "recording_paused" | "not_recording" | "save_failed")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_text_names_the_game_or_says_unknown() {
        assert_eq!(saved_text(Some("Active Matter")), "Clip saved — Active Matter");
        assert_eq!(saved_text(None), "Clip saved — Unknown game");
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
