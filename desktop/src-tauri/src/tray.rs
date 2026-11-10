//! The tray icon: shows whether Klipt is recording, and a menu to save, pause/resume, open the
//! window and quit.

use std::sync::{Mutex, PoisonError};

use tauri::image::Image;
use tauri::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Manager};

use crate::errors::Result;
use crate::recorder::{SupervisorEvent, SupervisorHandle};

const ICON_RECORDING: &[u8] = include_bytes!("../icons/tray-recording.png");
const ICON_PAUSED: &[u8] = include_bytes!("../icons/tray-paused.png");
const ICON_ERROR: &[u8] = include_bytes!("../icons/tray-error.png");

const ID_SAVE: &str = "save";
const ID_PAUSE: &str = "pause";
const ID_OPEN: &str = "open";
const ID_QUIT: &str = "quit";

pub const MAIN_WINDOW: &str = "main";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayStatus {
    Recording,
    /// Paused by the user from the tray.
    Paused,
    /// The recorder stopped (restarting, gave up or couldn't start).
    Error,
}

impl TrayStatus {
    pub fn tooltip(self) -> &'static str {
        match self {
            TrayStatus::Recording => "Klipt — recording",
            TrayStatus::Paused => "Klipt — paused",
            TrayStatus::Error => "Klipt — recording stopped",
        }
    }

    pub fn pause_label(self) -> &'static str {
        match self {
            TrayStatus::Paused => "Resume recording",
            _ => "Pause recording",
        }
    }

    fn icon(self) -> &'static [u8] {
        match self {
            TrayStatus::Recording => ICON_RECORDING,
            TrayStatus::Paused => ICON_PAUSED,
            TrayStatus::Error => ICON_ERROR,
        }
    }
}

/// Error codes that are about one save, not about the recorder: they don't change the icon.
const SAVE_ONLY_CODES: &[&str] = &["save_failed", "not_recording", "recording_paused"];

/// The status after a supervisor event. Only the user leaves `Paused`.
pub fn status_after(current: TrayStatus, event: &SupervisorEvent) -> TrayStatus {
    match (current, event) {
        (TrayStatus::Paused, _) => TrayStatus::Paused,
        (_, SupervisorEvent::Recording { .. } | SupervisorEvent::ClipSaved(_)) => TrayStatus::Recording,
        (_, SupervisorEvent::Error { code, .. }) if !SAVE_ONLY_CODES.contains(&code.as_str()) => TrayStatus::Error,
        (current, _) => current,
    }
}

/// Managed by the app; created by [`create`].
pub struct Tray {
    icon: TrayIcon,
    pause_item: MenuItem<tauri::Wry>,
    status: Mutex<TrayStatus>,
}

impl Tray {
    pub fn status(&self) -> TrayStatus {
        *self.status.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn set_status(&self, status: TrayStatus) {
        {
            let mut current = self.status.lock().unwrap_or_else(PoisonError::into_inner);
            if *current == status {
                return;
            }
            *current = status;
        }
        if let Err(e) = Image::from_bytes(status.icon()).and_then(|image| self.icon.set_icon(Some(image))) {
            eprintln!("klipt: can't change the tray icon: {e}");
        }
        if let Err(e) = self.icon.set_tooltip(Some(status.tooltip())) {
            eprintln!("klipt: can't change the tray tooltip: {e}");
        }
        if let Err(e) = self.pause_item.set_text(status.pause_label()) {
            eprintln!("klipt: can't change the tray menu: {e}");
        }
    }

    pub fn on_supervisor_event(&self, event: &SupervisorEvent) {
        self.set_status(status_after(self.status(), event));
    }
}

/// Builds the tray icon and manages a [`Tray`].
pub fn create(app: &AppHandle) -> Result<(), tauri::Error> {
    let status = TrayStatus::Recording;
    let save = MenuItem::with_id(app, ID_SAVE, "Save clip now", true, None::<&str>)?;
    let pause_item = MenuItem::with_id(app, ID_PAUSE, status.pause_label(), true, None::<&str>)?;
    let open = MenuItem::with_id(app, ID_OPEN, "Open Klipt", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, ID_QUIT, "Quit", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&save, &pause_item, &separator, &open, &quit])?;
    let icon = TrayIconBuilder::with_id("klipt")
        .icon(Image::from_bytes(status.icon())?)
        .tooltip(status.tooltip())
        .menu(&menu)
        .on_menu_event(on_menu_event)
        .build(app)?;
    app.manage(Tray { icon, pause_item, status: Mutex::new(status) });
    Ok(())
}

fn on_menu_event(app: &AppHandle, event: MenuEvent) {
    match event.id().as_ref() {
        ID_SAVE => {
            if let Some(recorder) = app.try_state::<SupervisorHandle>() {
                recorder.save();
            }
        }
        ID_PAUSE => toggle_pause(app),
        ID_OPEN => show_main_window(app),
        // The normal exit path: RunEvent::Exit stops the recorder.
        ID_QUIT => app.exit(0),
        _ => {}
    }
}

fn toggle_pause(app: &AppHandle) {
    let (Some(tray), Some(recorder)) = (app.try_state::<Tray>(), app.try_state::<SupervisorHandle>()) else {
        return;
    };
    if tray.status() == TrayStatus::Paused {
        recorder.resume();
        // Optimistic: a failure to start arrives as an error event and turns the icon amber.
        tray.set_status(TrayStatus::Recording);
    } else {
        recorder.pause();
        tray.set_status(TrayStatus::Paused);
    }
}

pub fn show_main_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window(MAIN_WINDOW) else { return };
    let shown = window.unminimize().and_then(|()| window.show()).and_then(|()| window.set_focus());
    if let Err(e) = shown {
        eprintln!("klipt: can't show the window: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn error(code: &str) -> SupervisorEvent {
        SupervisorEvent::Error { code: code.into(), message: String::new() }
    }

    #[test]
    fn recorder_problems_turn_the_icon_to_error_and_recording_turns_it_back() {
        use TrayStatus::*;
        assert_eq!(status_after(Recording, &error("recording_stopped")), Error);
        assert_eq!(status_after(Recording, &error("recorder_crashed")), Error);
        assert_eq!(status_after(Recording, &error("recorder_missing")), Error);
        assert_eq!(status_after(Error, &SupervisorEvent::Recording { encoder: "h264".into() }), Recording);
    }

    #[test]
    fn save_problems_and_busy_do_not_change_the_icon() {
        use TrayStatus::*;
        assert_eq!(status_after(Recording, &error("save_failed")), Recording);
        assert_eq!(status_after(Recording, &error("not_recording")), Recording);
        assert_eq!(status_after(Recording, &SupervisorEvent::SaveBusy), Recording);
        assert_eq!(status_after(Error, &SupervisorEvent::SaveBusy), Error);
    }

    #[test]
    fn only_the_user_leaves_paused() {
        use TrayStatus::*;
        assert_eq!(status_after(Paused, &SupervisorEvent::Recording { encoder: "h264".into() }), Paused);
        assert_eq!(status_after(Paused, &error("recorder_crashed")), Paused);
    }

    #[test]
    fn labels_follow_the_status() {
        assert_eq!(TrayStatus::Recording.tooltip(), "Klipt — recording");
        assert_eq!(TrayStatus::Paused.tooltip(), "Klipt — paused");
        assert_eq!(TrayStatus::Error.tooltip(), "Klipt — recording stopped");
        assert_eq!(TrayStatus::Paused.pause_label(), "Resume recording");
        assert_eq!(TrayStatus::Error.pause_label(), "Pause recording");
    }

    #[test]
    fn tray_icons_decode() {
        for status in [TrayStatus::Recording, TrayStatus::Paused, TrayStatus::Error] {
            let image = Image::from_bytes(status.icon()).unwrap();
            assert_eq!((image.width(), image.height()), (32, 32));
        }
    }
}
