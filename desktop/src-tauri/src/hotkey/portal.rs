//! Linux: the save-clip shortcut through the XDG GlobalShortcuts portal (works on Wayland and in
//! fullscreen games; verified on KDE Plasma by the hotkey spike).
//!
//! The first bind makes KDE show a dialog where the user confirms or changes the key. The portal
//! reports the binding it actually made, which can differ from the one requested (KDE reported
//! `Alt+;` when `ALT+F9` was asked for), so [`BoundKey::display`] is the portal's description.
//!
//! Rebinding closes the current session and opens a new one with the new preferred trigger. KDE
//! may keep a key the user set for Klipt in System Settings; whatever it reports is shown.

use std::sync::mpsc;
use std::time::Duration;

use ashpd::desktop::global_shortcuts::{GlobalShortcuts, NewShortcut};
use ashpd::desktop::{ResponseError, Session};
use futures_util::StreamExt;
use tauri::async_runtime::{self, JoinHandle};
use tokio::sync::oneshot;

use super::{
    to_portal_trigger, Action, BoundKey, HotkeyService, Keys, PressSink, SCREENSHOT_DESCRIPTION, SCREENSHOT_ID,
    SHORTCUT_DESCRIPTION, SHORTCUT_ID,
};
use crate::errors::{AppError, Result};

/// How long closing the old portal session may take before a rebind moves on.
const CLOSE_TIMEOUT: Duration = Duration::from_secs(3);

/// The task that owns the current portal session and forwards its activations.
struct Listener {
    task: JoinHandle<()>,
    /// Asks the task to close its session and end.
    stop: oneshot::Sender<()>,
}

#[derive(Default)]
pub struct PortalHotkey {
    sink: PressSink,
    listener: Option<Listener>,
}

impl PortalHotkey {
    pub fn new() -> PortalHotkey {
        PortalHotkey::default()
    }

    /// Closes the current session and waits (briefly) until it is closed, so the next session
    /// can take the shortcut.
    fn unbind(&mut self) {
        let Some(Listener { task, stop }) = self.listener.take() else { return };
        if stop.send(()).is_err() {
            return; // The task already ended.
        }
        let abort = task.inner().abort_handle();
        let closed = async_runtime::block_on(async { tokio::time::timeout(CLOSE_TIMEOUT, task).await.is_ok() });
        if !closed {
            eprintln!("klipt: the old shortcut session didn't close in time");
            abort.abort();
        }
    }
}

impl Drop for PortalHotkey {
    fn drop(&mut self) {
        if let Some(listener) = self.listener.take() {
            // At exit: don't wait. The portal drops the session when Klipt's D-Bus connection
            // closes.
            listener.task.abort();
        }
    }
}

fn portal_error(e: ashpd::Error) -> AppError {
    match e {
        ashpd::Error::Response(ResponseError::Cancelled) => {
            AppError::HotkeyUnavailable("the user dismissed the shortcut dialog".into())
        }
        e => AppError::HotkeyUnavailable(format!("GlobalShortcuts portal: {e}")),
    }
}

/// Opens a session, binds the shortcut and reports the binding through `bound`; then forwards
/// activations to `sink` until `stop` fires (or the portal goes away).
async fn run_session(
    trigger: String,
    screenshot_trigger: Option<String>,
    sink: PressSink,
    bound: mpsc::Sender<Result<BoundKey>>,
    mut stop: oneshot::Receiver<()>,
) {
    let setup = async {
        let portal = GlobalShortcuts::new().await?;
        let session: Session<'static, GlobalShortcuts<'static>> = portal.create_session().await?;
        // Both shortcuts in one session: KDE only delivers presses for shortcuts bound in it.
        let mut shortcuts =
            vec![NewShortcut::new(SHORTCUT_ID, SHORTCUT_DESCRIPTION).preferred_trigger(Some(trigger.as_str()))];
        if let Some(t) = &screenshot_trigger {
            shortcuts.push(NewShortcut::new(SCREENSHOT_ID, SCREENSHOT_DESCRIPTION).preferred_trigger(Some(t.as_str())));
        }
        let response = portal.bind_shortcuts(&session, &shortcuts, None).await?.response()?;
        let display_of = |id: &str| {
            response
                .shortcuts()
                .iter()
                .find(|s| s.id() == id)
                .map(|s| s.trigger_description().trim().to_string())
                .unwrap_or_default()
        };
        let display = display_of(SHORTCUT_ID);
        let screenshot = display_of(SCREENSHOT_ID);
        let activated = portal.receive_activated().await?;
        Ok::<_, ashpd::Error>((portal, session, display, screenshot, activated))
    };
    let (portal, session, display, screenshot, mut activated) = match setup.await {
        Ok(ok) => ok,
        Err(e) => {
            let _ = bound.send(Err(portal_error(e)));
            return;
        }
    };
    if display.is_empty() {
        // The portal kept the shortcut without a key: KDE does this when the requested key
        // is already taken by another shortcut.
        eprintln!("klipt: the portal bound {SHORTCUT_ID} without a key (requested {trigger})");
        let _ = session.close().await;
        let _ = bound.send(Err(AppError::HotkeyTaken));
        return;
    }
    eprintln!("klipt: save-clip shortcut bound to {display} (requested {trigger})");
    let (screenshot, screenshot_error) = match &screenshot_trigger {
        None => (None, None),
        Some(_) if screenshot.is_empty() => {
            (None, Some("The screenshot shortcut is taken by another app. Pick another one in Settings.".to_string()))
        }
        Some(_) => (Some(screenshot), None),
    };
    if bound.send(Ok(BoundKey { display, screenshot, screenshot_error })).is_err() {
        let _ = session.close().await;
        return;
    }
    loop {
        tokio::select! {
            event = activated.next() => match event {
                Some(event) if event.shortcut_id() == SHORTCUT_ID => sink.press(Action::SaveClip),
                Some(event) if event.shortcut_id() == SCREENSHOT_ID => sink.press(Action::Screenshot),
                Some(_) => {}
                None => break,
            },
            _ = &mut stop => {
                // A rebind: close quietly. The press stream stays open for the next session.
                let _ = session.close().await;
                return;
            }
        }
    }
    eprintln!("klipt: the GlobalShortcuts portal stopped sending shortcut presses");
    // Ends the press stream, so the app tells the user the shortcut is gone.
    sink.close();
    let _ = session.close().await;
    drop(portal);
}

impl HotkeyService for PortalHotkey {
    fn bind(&mut self, keys: &Keys) -> Result<BoundKey> {
        let trigger = to_portal_trigger(&keys.save)?;
        let screenshot_trigger = keys.screenshot.as_deref().map(to_portal_trigger).transpose()?;
        self.unbind();
        let (tx, rx) = mpsc::channel();
        let (stop, stop_rx) = oneshot::channel();
        let task = async_runtime::spawn(run_session(trigger, screenshot_trigger, self.sink.clone(), tx, stop_rx));
        // Blocks until the portal answers, which includes the time the user spends in KDE's
        // dialog the first time. Callers run this off the main thread.
        let result = rx
            .recv()
            .unwrap_or_else(|_| Err(AppError::HotkeyUnavailable("the portal task ended without an answer".into())));
        if result.is_ok() {
            self.listener = Some(Listener { task, stop });
        }
        result
    }

    fn events(&self) -> mpsc::Receiver<Action> {
        self.sink.subscribe()
    }
}
