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
const ID_RECORD: &str = "record";
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

/// The tooltip; a running full recording shows unless the recorder has a problem.
pub fn tooltip_for(status: TrayStatus, full_recording: bool) -> &'static str {
    match status {
        TrayStatus::Recording if full_recording => "Klipt — recording video",
        status => status.tooltip(),
    }
}

/// The start/stop recording menu item's label.
pub fn full_recording_label(full_recording: bool) -> &'static str {
    if full_recording {
        "Stop recording"
    } else {
        "Start recording"
    }
}

/// Error codes that are about one save or recording, not about the recorder: they don't change
/// the icon.
const SAVE_ONLY_CODES: &[&str] =
    &["save_failed", "not_recording", "recording_paused", "full_recording_lost", "full_recording_failed"];

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
    /// The status, and whether a full recording runs.
    status: Mutex<(TrayStatus, bool)>,
    post: Box<dyn Fn(TrayStatus, bool) + Send + Sync>,
}

impl Tray {
    pub fn new(status: TrayStatus, post: Box<dyn Fn(TrayStatus, bool) + Send + Sync>) -> Tray {
        Tray { status: Mutex::new((status, false)), post }
    }

    pub fn status(&self) -> TrayStatus {
        self.status.lock().unwrap_or_else(PoisonError::into_inner).0
    }

    pub fn full_recording(&self) -> bool {
        self.status.lock().unwrap_or_else(PoisonError::into_inner).1
    }

    /// Computes and stores the next status atomically; posts it if it changed. Returns it.
    fn update(&self, next: impl FnOnce(TrayStatus) -> TrayStatus) -> TrayStatus {
        self.update_both(|(status, full)| (next(status), full)).0
    }

    fn update_both(&self, next: impl FnOnce((TrayStatus, bool)) -> (TrayStatus, bool)) -> (TrayStatus, bool) {
        let mut current = self.status.lock().unwrap_or_else(PoisonError::into_inner);
        let new = next(*current);
        if new != *current {
            *current = new;
            // Posted while holding the lock, so updates reach the main thread in order.
            (self.post)(new.0, new.1);
        }
        new
    }

    /// Recording is paused by the user (from the tray).
    pub fn is_paused(&self) -> bool {
        self.status() == TrayStatus::Paused
    }

    pub fn set_status(&self, status: TrayStatus) {
        self.update(|_| status);
    }

    pub fn on_supervisor_event(&self, event: &SupervisorEvent) {
        self.update_both(|(current, full)| {
            let full = match event {
                SupervisorEvent::FullRecordingStarted { .. } => true,
                SupervisorEvent::FullRecordingStopped => false,
                _ => full,
            };
            (status_after(current, event), full)
        });
    }

    /// Flips between paused and recording; returns the new status.
    pub fn toggle_pause(&self) -> TrayStatus {
        // Resuming is optimistic: a failure to start arrives as an error event.
        self.update(|current| if current == TrayStatus::Paused { TrayStatus::Recording } else { TrayStatus::Paused })
    }
}

/// Applies a status to the tray widgets. Runs on the main thread.
fn apply(
    icon: &TrayIcon,
    pause_item: &MenuItem<tauri::Wry>,
    record_item: &MenuItem<tauri::Wry>,
    status: TrayStatus,
    full_recording: bool,
) {
    if let Err(e) = Image::from_bytes(status.icon()).and_then(|image| icon.set_icon(Some(image))) {
        crate::elog!("klipt: can't change the tray icon: {e}");
    }
    if let Err(e) = icon.set_tooltip(Some(tooltip_for(status, full_recording))) {
        crate::elog!("klipt: can't change the tray tooltip: {e}");
    }
    if let Err(e) = pause_item.set_text(status.pause_label()) {
        crate::elog!("klipt: can't change the tray menu: {e}");
    }
    if let Err(e) = record_item.set_text(full_recording_label(full_recording)) {
        crate::elog!("klipt: can't change the tray menu: {e}");
    }
}

/// Builds the tray icon and manages a [`Tray`].
pub fn create(app: &AppHandle) -> Result<(), tauri::Error> {
    let status = TrayStatus::Recording;
    let save = MenuItem::with_id(app, ID_SAVE, "Save clip now", true, None::<&str>)?;
    let record_item = MenuItem::with_id(app, ID_RECORD, full_recording_label(false), true, None::<&str>)?;
    let pause_item = MenuItem::with_id(app, ID_PAUSE, status.pause_label(), true, None::<&str>)?;
    let open = MenuItem::with_id(app, ID_OPEN, "Open Klipt", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, ID_QUIT, "Quit", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&save, &record_item, &pause_item, &separator, &open, &quit])?;
    let icon = TrayIconBuilder::with_id("klipt")
        .icon(Image::from_bytes(status.icon())?)
        .tooltip(status.tooltip())
        .menu(&menu)
        .on_menu_event(on_menu_event)
        .build(app)?;
    let main = app.clone();
    let post = move |status: TrayStatus, full_recording: bool| {
        let (icon, pause_item, record_item) = (icon.clone(), pause_item.clone(), record_item.clone());
        // Queues the update and returns right away; never waits for the main thread.
        if let Err(e) =
            main.run_on_main_thread(move || apply(&icon, &pause_item, &record_item, status, full_recording))
        {
            crate::elog!("klipt: can't update the tray: {e}");
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
        ID_RECORD => {
            if let Some(recorder) = app.try_state::<SupervisorHandle>() {
                recorder.toggle_full_recording();
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
                crate::elog!("klipt: can't open the window: {e}");
                return;
            }
        },
    };
    let shown = window.unminimize().and_then(|()| window.show()).and_then(|()| window.set_focus());
    if let Err(e) = shown {
        crate::elog!("klipt: can't show the window: {e}");
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

/// Sent when the window goes to the tray or is minimized: the UI stops anything playing.
pub const WINDOW_HIDDEN: &str = "window-hidden";

/// Counts hides and shows, so a release waiting for an earlier hide stops when the window is shown.
#[derive(Default)]
pub struct WindowRelease(AtomicU64);

/// Hides the window to the tray; its web view is released after [`RELEASE_AFTER`] if it stays
/// hidden. Also called at a start in the tray, where the window was made hidden.
pub fn hide_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        let _ = app.emit(WINDOW_HIDDEN, ());
        if let Err(e) = window.hide() {
            crate::elog!("klipt: can't hide the window: {e}");
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
        crate::elog!("klipt: can't close the hidden window: {e}");
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

    type Posted<T> = std::sync::Arc<Mutex<Vec<T>>>;

    fn recording_tray_with_full() -> (Tray, Posted<TrayStatus>, Posted<bool>) {
        let (posted, posted_full) = (Posted::default(), Posted::default());
        let (sink, sink_full) = (posted.clone(), posted_full.clone());
        // A post that only records: the tray logic itself never touches tray widgets.
        let tray = Tray::new(
            TrayStatus::Recording,
            Box::new(move |s, full| {
                sink.lock().unwrap().push(s);
                sink_full.lock().unwrap().push(full);
            }),
        );
        (tray, posted, posted_full)
    }

    fn recording_tray() -> (Tray, Posted<TrayStatus>) {
        let (tray, posted, _) = recording_tray_with_full();
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
    fn the_full_recording_shows_in_the_menu_and_tooltip() {
        assert_eq!(full_recording_label(false), "Start recording");
        assert_eq!(full_recording_label(true), "Stop recording");
        assert_eq!(tooltip_for(TrayStatus::Recording, true), "Klipt — recording video");
        assert_eq!(tooltip_for(TrayStatus::Recording, false), "Klipt — recording");
        assert_eq!(tooltip_for(TrayStatus::Error, true), "Klipt — recording stopped");
        let (tray, posted, posted_full) = recording_tray_with_full();
        tray.on_supervisor_event(&SupervisorEvent::FullRecordingStarted { started_at_ms: 1 });
        assert!(tray.full_recording());
        tray.on_supervisor_event(&error("full_recording_lost"));
        tray.on_supervisor_event(&SupervisorEvent::FullRecordingStopped);
        assert!(!tray.full_recording());
        assert_eq!(*posted.lock().unwrap(), [TrayStatus::Recording, TrayStatus::Recording], "posted for each change");
        assert_eq!(*posted_full.lock().unwrap(), [true, false]);
    }

    #[test]
    fn tray_icons_decode() {
        for status in [TrayStatus::Recording, TrayStatus::Paused, TrayStatus::Error] {
            let image = Image::from_bytes(status.icon()).unwrap();
            assert_eq!((image.width(), image.height()), (32, 32));
        }
    }
}
