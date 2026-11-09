//! Global hotkey for `--selftest --hotkey ALT+F9` on Windows, via `RegisterHotKey`.
//! (The desktop app has its own hotkey service; this exists so the owner can test saving
//! from inside a fullscreen game before the app exists.)

pub const MOD_ALT: u32 = 0x0001;
pub const MOD_CONTROL: u32 = 0x0002;
pub const MOD_SHIFT: u32 = 0x0004;
pub const MOD_WIN: u32 = 0x0008;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Accelerator {
    pub modifiers: u32,
    pub vk: u32,
}

/// Parses "ALT+F9", "ctrl+shift+s", "F8"… into Win32 modifier flags and a virtual-key code.
pub fn parse_accelerator(text: &str) -> Result<Accelerator, String> {
    let parts: Vec<&str> = text.split('+').map(str::trim).collect();
    if parts.iter().any(|p| p.is_empty()) {
        return Err(format!("\"{text}\" is not a valid shortcut"));
    }
    let (key, mods) = parts.split_last().ok_or_else(|| "empty shortcut".to_string())?;
    let mut modifiers = 0;
    for m in mods {
        modifiers |= match m.to_ascii_lowercase().as_str() {
            "alt" => MOD_ALT,
            "ctrl" | "control" => MOD_CONTROL,
            "shift" => MOD_SHIFT,
            "win" | "super" | "meta" => MOD_WIN,
            other => return Err(format!("unknown modifier \"{other}\"")),
        };
    }
    Ok(Accelerator { modifiers, vk: virtual_key(key)? })
}

fn virtual_key(key: &str) -> Result<u32, String> {
    let upper = key.to_ascii_uppercase();
    if let Some(n) = upper.strip_prefix('F').and_then(|n| n.parse::<u32>().ok()) {
        if (1..=24).contains(&n) {
            return Ok(0x70 + n - 1); // VK_F1..VK_F24
        }
    }
    if upper.len() == 1 {
        let c = upper.as_bytes()[0];
        if c.is_ascii_uppercase() || c.is_ascii_digit() {
            return Ok(c as u32); // 'A'..'Z', '0'..'9' are their own VK codes
        }
    }
    if let Some(n) = upper.strip_prefix("NUM").and_then(|n| n.parse::<u32>().ok()) {
        if n <= 9 {
            return Ok(0x60 + n); // VK_NUMPAD0..9
        }
    }
    Ok(match upper.as_str() {
        "SPACE" => 0x20,
        "PAGEUP" | "PGUP" => 0x21,
        "PAGEDOWN" | "PGDN" => 0x22,
        "END" => 0x23,
        "HOME" => 0x24,
        "PRINTSCREEN" | "PRTSC" => 0x2C,
        "INSERT" | "INS" => 0x2D,
        "DELETE" | "DEL" => 0x2E,
        "PAUSE" => 0x13,
        "SCROLLLOCK" => 0x91,
        _ => return Err(format!("unknown key \"{key}\"")),
    })
}

/// Registers the hotkey on a dedicated thread (hotkeys are delivered to the registering thread's
/// message queue) and calls `on_press` for every press. Returns once registration succeeded or failed.
#[cfg(windows)]
pub fn listen(accel: Accelerator, on_press: impl Fn() + Send + 'static) -> Result<(), String> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, MOD_NOREPEAT};
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetMessageW, MSG, WM_HOTKEY};

    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("hotkey".into())
        .spawn(move || unsafe {
            let ok = RegisterHotKey(std::ptr::null_mut(), 1, accel.modifiers | MOD_NOREPEAT, accel.vk) != 0;
            let _ = tx.send(if ok {
                Ok(())
            } else {
                Err(format!(
                    "RegisterHotKey failed ({}); another program may already use this shortcut",
                    std::io::Error::last_os_error()
                ))
            });
            if !ok {
                return;
            }
            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                if msg.message == WM_HOTKEY {
                    on_press();
                }
            }
        })
        .map_err(|e| e.to_string())?;
    rx.recv().map_err(|_| "hotkey thread exited".to_string())?
}

#[cfg(not(windows))]
pub fn listen(_accel: Accelerator, _on_press: impl Fn() + Send + 'static) -> Result<(), String> {
    Err("--hotkey is only supported on Windows; on Linux the app uses the GlobalShortcuts portal".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_modifiers_and_function_keys() {
        assert_eq!(parse_accelerator("ALT+F9"), Ok(Accelerator { modifiers: MOD_ALT, vk: 0x78 }));
        assert_eq!(parse_accelerator("alt+f9"), Ok(Accelerator { modifiers: MOD_ALT, vk: 0x78 }));
        assert_eq!(
            parse_accelerator("Ctrl + Shift + S"),
            Ok(Accelerator { modifiers: MOD_CONTROL | MOD_SHIFT, vk: b'S' as u32 })
        );
        assert_eq!(parse_accelerator("F8"), Ok(Accelerator { modifiers: 0, vk: 0x77 }));
        assert_eq!(parse_accelerator("Win+F24"), Ok(Accelerator { modifiers: MOD_WIN, vk: 0x87 }));
        assert_eq!(parse_accelerator("ctrl+num5"), Ok(Accelerator { modifiers: MOD_CONTROL, vk: 0x65 }));
        assert_eq!(parse_accelerator("Alt+Home"), Ok(Accelerator { modifiers: MOD_ALT, vk: 0x24 }));
        assert_eq!(parse_accelerator("alt+7"), Ok(Accelerator { modifiers: MOD_ALT, vk: b'7' as u32 }));
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_accelerator("").is_err());
        assert!(parse_accelerator("ALT+").is_err());
        assert!(parse_accelerator("Hyper+F9").is_err());
        assert!(parse_accelerator("Alt+F25").is_err());
        assert!(parse_accelerator("Alt+Banana").is_err());
    }
}
