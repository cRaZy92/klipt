//! The tray icon: shows whether Klipt is recording, and a menu to save, pause/resume, open the
//! window and quit.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use tauri::image::Image;
use tauri::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow, WebviewWindowBuilder};

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
///
/// Status changes are decided and stored under one lock, then handed to `post`, which must not
/// block: in the app it queues the icon/menu update on the main thread. Tray APIs block until the
/// main thread runs them, and the supervisor thread (which reports events) is joined by the main
/// thread on exit, so calling them directly from there could deadlock Quit.
pub struct Tray {
    status: Mutex<TrayStatus>,
    post: Box<dyn Fn(TrayStatus) + Send + Sync>,
}

impl Tray {
    pub fn new(status: TrayStatus, post: Box<dyn Fn(TrayStatus) + Send + Sync>) -> Tray {
        Tray { status: Mutex::new(status), post }
    }

    pub fn status(&self) -> TrayStatus {
        *self.status.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Computes and stores the next status atomically; posts it if it changed. Returns it.
    fn update(&self, next: impl FnOnce(TrayStatus) -> TrayStatus) -> TrayStatus {
        let mut current = self.status.lock().unwrap_or_else(PoisonError::into_inner);
        let new = next(*current);
        if new != *current {
            *current = new;
            // Posted while holding the lock, so updates reach the main thread in order.
            (self.post)(new);
        }
        new
    }

    /// Recording is paused by the user (from the tray).
    pub fn is_paused(&self) -> bool {
        *self.status.lock().unwrap_or_else(PoisonError::into_inner) == TrayStatus::Paused
    }

    pub fn set_status(&self, status: TrayStatus) {
        self.update(|_| status);
    }

    pub fn on_supervisor_event(&self, event: &SupervisorEvent) {
        self.update(|current| status_after(current, event));
    }

    /// Flips between paused and recording; returns the new status.
    pub fn toggle_pause(&self) -> TrayStatus {
        // Resuming is optimistic: a failure to start arrives as an error event.
        self.update(|current| if current == TrayStatus::Paused { TrayStatus::Recording } else { TrayStatus::Paused })
    }
}

/// Applies a status to the tray widgets. Runs on the main thread.
fn apply(icon: &TrayIcon, pause_item: &MenuItem<tauri::Wry>, status: TrayStatus) {
    if let Err(e) = Image::from_bytes(status.icon()).and_then(|image| icon.set_icon(Some(image))) {
        eprintln!("klipt: can't change the tray icon: {e}");
    }
    if let Err(e) = icon.set_tooltip(Some(status.tooltip())) {
        eprintln!("klipt: can't change the tray tooltip: {e}");
    }
    if let Err(e) = pause_item.set_text(status.pause_label()) {
        eprintln!("klipt: can't change the tray menu: {e}");
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
    let main = app.clone();
    let post = move |status: TrayStatus| {
        let (icon, pause_item) = (icon.clone(), pause_item.clone());
        // Queues the update and returns right away; never waits for the main thread.
        if let Err(e) = main.run_on_main_thread(move || apply(&icon, &pause_item, status)) {
            eprintln!("klipt: can't update the tray: {e}");
        }
    };
    app.manage(Tray::new(status, Box::new(post)));
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
    match tray.toggle_pause() {
        TrayStatus::Paused => recorder.pause(),
        _ => recorder.resume(),
    }
}

/// Shows the window, making it again if it was released (see [`hide_main_window`]).
pub fn show_main_window(app: &AppHandle) {
    // Any release waiting for the hidden window is off.
    if let Some(release) = app.try_state::<WindowRelease>() {
        release.0.fetch_add(1, Ordering::SeqCst);
    }
    let window = match app.get_webview_window(MAIN_WINDOW) {
        Some(window) => window,
        None => match create_main_window(app) {
            Ok(window) => window,
            Err(e) => {
                eprintln!("klipt: can't open the window: {e}");
                return;
            }
        },
    };
    let shown = window.unminimize().and_then(|()| window.show()).and_then(|()| window.set_focus());
    if let Err(e) = shown {
        eprintln!("klipt: can't show the window: {e}");
    }
}

fn create_main_window(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let config = app.config().app.windows.iter().find(|w| w.label == MAIN_WINDOW).cloned();
    let config = config.ok_or_else(|| tauri::Error::WindowNotFound)?;
    WebviewWindowBuilder::from_config(app, &config)?.build()
}

/// How long the window stays hidden before its web view is closed, which frees its memory (a few
/// hundred MB). Reopening within that time is instant; after it, the window loads again (~1 s).
const RELEASE_AFTER: Duration = Duration::from_secs(5 * 60);
/// [`RELEASE_AFTER`], or `KLIPT_RELEASE_AFTER_SECS` in a development build, to try it quickly.
fn release_after() -> Duration {
    let dev = cfg!(debug_assertions).then(|| std::env::var("KLIPT_RELEASE_AFTER_SECS").ok()).flatten();
    dev.and_then(|s| s.parse().ok()).map_or(RELEASE_AFTER, Duration::from_secs)
}

/// When the UI says it's busy (editor open, unsaved settings, upload or import running), how long
/// until it's asked again.
const RELEASE_RETRY: Duration = Duration::from_secs(60);
/// Event to the UI: the window has been hidden a while. It answers with `release_window` when
/// nothing would be lost.
pub const RELEASE_CHECK: &str = "release-window-check";

/// Counts hides and shows, so a release waiting for an earlier hide stops when the window is shown.
#[derive(Default)]
pub struct WindowRelease(AtomicU64);

/// Hides the window to the tray; its web view is released after [`RELEASE_AFTER`] if it stays
/// hidden. Also called at a start in the tray, where the window was made hidden.
pub fn hide_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        if let Err(e) = window.hide() {
            eprintln!("klipt: can't hide the window: {e}");
        }
    }
    let Some(release) = app.try_state::<WindowRelease>() else { return };
    let generation = release.0.fetch_add(1, Ordering::SeqCst) + 1;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(release_after()).await;
        loop {
            let current = app.state::<WindowRelease>().0.load(Ordering::SeqCst);
            let Some(window) = app.get_webview_window(MAIN_WINDOW) else { return };
            if current != generation || window.is_visible().unwrap_or(true) {
                return;
            }
            let _ = app.emit(RELEASE_CHECK, ());
            tokio::time::sleep(RELEASE_RETRY).await;
        }
    });
}

/// The UI's answer to [`RELEASE_CHECK`]: closes the window if it's still hidden. Klipt keeps
/// running in the tray (`RunEvent::ExitRequested` is turned down in lib.rs).
#[tauri::command]
pub async fn release_window(app: AppHandle) {
    let Some(window) = app.get_webview_window(MAIN_WINDOW) else { return };
    if window.is_visible().unwrap_or(true) {
        return;
    }
    // Linux: closing the window alone leaves WebKit's web process (most of the memory) running,
    // kept for reuse; end it first.
    #[cfg(target_os = "linux")]
    {
        use webkit2gtk::WebViewExt;
        let _ = window.with_webview(|webview| webview.inner().terminate_web_process());
    }
    if let Err(e) = window.destroy() {
        eprintln!("klipt: can't close the hidden window: {e}");
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

    fn recording_tray() -> (Tray, std::sync::Arc<Mutex<Vec<TrayStatus>>>) {
        let posted = std::sync::Arc::new(Mutex::new(vec![]));
        let sink = posted.clone();
        // A post that only records: the tray logic itself never touches tray widgets.
        let tray = Tray::new(TrayStatus::Recording, Box::new(move |s| sink.lock().unwrap().push(s)));
        (tray, posted)
    }

    #[test]
    fn events_post_only_changes_and_never_touch_widgets_directly() {
        let (tray, posted) = recording_tray();
        tray.on_supervisor_event(&error("recording_stopped"));
        tray.on_supervisor_event(&error("recorder_crashed"));
        tray.on_supervisor_event(&SupervisorEvent::SaveBusy);
        tray.on_supervisor_event(&SupervisorEvent::Recording { encoder: "h264".into() });
        assert_eq!(*posted.lock().unwrap(), [TrayStatus::Error, TrayStatus::Recording]);
    }

    #[test]
    fn an_event_after_pause_does_not_undo_it() {
        let (tray, posted) = recording_tray();
        assert_eq!(tray.toggle_pause(), TrayStatus::Paused);
        tray.on_supervisor_event(&SupervisorEvent::Recording { encoder: "h264".into() });
        assert_eq!(tray.status(), TrayStatus::Paused);
        assert_eq!(tray.toggle_pause(), TrayStatus::Recording);
        assert_eq!(*posted.lock().unwrap(), [TrayStatus::Paused, TrayStatus::Recording]);
    }

    #[test]
    fn tray_icons_decode() {
        for status in [TrayStatus::Recording, TrayStatus::Paused, TrayStatus::Error] {
            let image = Image::from_bytes(status.icon()).unwrap();
            assert_eq!((image.width(), image.height()), (32, 32));
        }
    }
}
