//! Linux: the save-clip shortcut through the XDG GlobalShortcuts portal (works on Wayland and in
//! fullscreen games; verified on KDE Plasma by the hotkey spike).
//!
//! The first bind makes KDE show a dialog where the user confirms or changes the key. The portal
//! reports the binding it actually made, which can differ from the one requested (KDE reported
//! `Alt+;` when `ALT+F9` was asked for), so [`BoundKey::display`] is the portal's description.

use std::sync::mpsc;

use ashpd::desktop::global_shortcuts::{GlobalShortcuts, NewShortcut};
use ashpd::desktop::{ResponseError, Session};
use futures_util::StreamExt;
use tauri::async_runtime::{self, JoinHandle};

use super::{to_portal_trigger, BoundKey, HotkeyService, PressSink, SHORTCUT_DESCRIPTION, SHORTCUT_ID};
use crate::errors::{AppError, Result};

#[derive(Default)]
pub struct PortalHotkey {
    sink: PressSink,
    /// The task that owns the current portal session and forwards its activations.
    listener: Option<JoinHandle<()>>,
}

impl PortalHotkey {
    pub fn new() -> PortalHotkey {
        PortalHotkey::default()
    }

    fn unbind(&mut self) {
        if let Some(listener) = self.listener.take() {
            // Stops forwarding presses. The aborted task can't close its portal session; the
            // portal drops it when Klipt's D-Bus connection closes. (Rebinding is Task 12.)
            listener.abort();
        }
    }
}

impl Drop for PortalHotkey {
    fn drop(&mut self) {
        self.unbind();
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
/// activations to `sink` for as long as the task lives.
async fn run_session(trigger: String, sink: PressSink, bound: mpsc::Sender<Result<BoundKey>>) {
    let setup = async {
        let portal = GlobalShortcuts::new().await?;
        let session: Session<'static, GlobalShortcuts<'static>> = portal.create_session().await?;
        let shortcut = NewShortcut::new(SHORTCUT_ID, SHORTCUT_DESCRIPTION).preferred_trigger(Some(trigger.as_str()));
        let response = portal.bind_shortcuts(&session, &[shortcut], None).await?.response()?;
        let display = response
            .shortcuts()
            .iter()
            .find(|s| s.id() == SHORTCUT_ID)
            .map(|s| s.trigger_description().trim().to_string())
            .unwrap_or_default();
        let activated = portal.receive_activated().await?;
        Ok::<_, ashpd::Error>((portal, session, display, activated))
    };
    let (portal, session, display, mut activated) = match setup.await {
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
    if bound.send(Ok(BoundKey { display })).is_err() {
        let _ = session.close().await;
        return;
    }
    while let Some(event) = activated.next().await {
        if event.shortcut_id() == SHORTCUT_ID {
            sink.press();
        }
    }
    eprintln!("klipt: the GlobalShortcuts portal stopped sending shortcut presses");
    let _ = session.close().await;
    drop(portal);
}

impl HotkeyService for PortalHotkey {
    fn bind(&mut self, accel: &str) -> Result<BoundKey> {
        let trigger = to_portal_trigger(accel)?;
        self.unbind();
        let (tx, rx) = mpsc::channel();
        let listener = async_runtime::spawn(run_session(trigger, self.sink.clone(), tx));
        // Blocks until the portal answers, which includes the time the user spends in KDE's
        // dialog the first time. Callers run this off the main thread.
        let result = rx
            .recv()
            .unwrap_or_else(|_| Err(AppError::HotkeyUnavailable("the portal task ended without an answer".into())));
        if result.is_ok() {
            self.listener = Some(listener);
        }
        result
    }

    fn events(&self) -> mpsc::Receiver<()> {
        self.sink.subscribe()
    }
}
