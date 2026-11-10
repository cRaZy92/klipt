//! Windows: the save-clip shortcut through tauri-plugin-global-shortcut (RegisterHotKey).
//!
//! A shortcut another app holds (NVIDIA App takes Alt+F9/F10/Z/R) fails with
//! ERROR_HOTKEY_ALREADY_REGISTERED, which becomes [`AppError::HotkeyTaken`].
//!
//! [`BoundKey::display`] is [`super::label`] of the accelerator: it names the US-layout symbol.
//! Showing the key as the user's layout prints it (GetKeyNameText) is a later polish.

use std::sync::mpsc::Receiver;

use tauri::AppHandle;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

use super::{classify_register_error, label, to_plugin_accelerator, BoundKey, HotkeyService, PressSink};
use crate::errors::{AppError, Result};

pub struct PluginHotkey {
    app: AppHandle,
    sink: PressSink,
    /// The accelerator currently registered.
    current: Option<String>,
}

impl PluginHotkey {
    pub fn new(app: AppHandle) -> PluginHotkey {
        PluginHotkey { app, sink: PressSink::default(), current: None }
    }
}

impl HotkeyService for PluginHotkey {
    fn bind(&mut self, accel: &str) -> Result<BoundKey> {
        let accelerator = to_plugin_accelerator(accel)?;
        let display = label(accel)?;
        let shortcuts = self.app.global_shortcut();
        if let Some(old) = self.current.take() {
            if let Err(e) = shortcuts.unregister(old.as_str()) {
                eprintln!("klipt: can't unregister the old shortcut {old}: {e}");
            }
        }
        let sink = self.sink.clone();
        shortcuts
            .on_shortcut(accelerator.as_str(), move |_app, _shortcut, event| {
                if event.state == ShortcutState::Pressed {
                    sink.press();
                }
            })
            .map_err(|e| match e {
                tauri_plugin_global_shortcut::Error::GlobalHotkey(message) => classify_register_error(&message),
                e => AppError::HotkeyUnavailable(e.to_string()),
            })?;
        self.current = Some(accelerator);
        Ok(BoundKey { display })
    }

    fn events(&self) -> Receiver<()> {
        self.sink.subscribe()
    }
}
