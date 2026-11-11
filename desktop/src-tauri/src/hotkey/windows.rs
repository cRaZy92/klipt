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

use super::{classify_register_error, label, to_plugin_accelerator, Action, BoundKey, HotkeyService, Keys, PressSink};
use crate::errors::{AppError, Result};

pub struct PluginHotkey {
    app: AppHandle,
    sink: PressSink,
    /// The accelerators currently registered.
    current: Vec<String>,
}

impl PluginHotkey {
    pub fn new(app: AppHandle) -> PluginHotkey {
        PluginHotkey { app, sink: PressSink::default(), current: Vec::new() }
    }
}

impl PluginHotkey {
    fn register(&mut self, accel: &str, action: Action) -> Result<String> {
        let accelerator = to_plugin_accelerator(accel)?;
        let display = label(accel)?;
        let sink = self.sink.clone();
        self.app
            .global_shortcut()
            .on_shortcut(accelerator.as_str(), move |_app, _shortcut, event| {
                if event.state == ShortcutState::Pressed {
                    sink.press(action);
                }
            })
            .map_err(|e| match e {
                tauri_plugin_global_shortcut::Error::GlobalHotkey(message) => classify_register_error(&message),
                e => AppError::HotkeyUnavailable(e.to_string()),
            })?;
        self.current.push(accelerator);
        Ok(display)
    }
}

impl HotkeyService for PluginHotkey {
    fn bind(&mut self, keys: &Keys) -> Result<BoundKey> {
        for old in std::mem::take(&mut self.current) {
            if let Err(e) = self.app.global_shortcut().unregister(old.as_str()) {
                eprintln!("klipt: can't unregister the old shortcut {old}: {e}");
            }
        }
        let display = self.register(&keys.save, Action::SaveClip)?;
        let (screenshot, screenshot_error) = match &keys.screenshot {
            None => (None, None),
            Some(accel) => match self.register(accel, Action::Screenshot) {
                Ok(shown) => (Some(shown), None),
                Err(e) => (None, Some(format!("The screenshot shortcut didn't work: {}", e.user_message()))),
            },
        };
        Ok(BoundKey { display, screenshot, screenshot_error })
    }

    fn events(&self) -> Receiver<Action> {
        self.sink.subscribe()
    }
}
