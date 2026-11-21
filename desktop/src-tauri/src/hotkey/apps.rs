//! Apps that are running and come with their own shortcuts (overlays, recorders). Their keys can
//! swallow Klipt's even when Windows lets Klipt register them (overlays read the keyboard
//! directly), so Settings → Hotkey names them, and warns when one uses the same keys.

use serde::Serialize;
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};

use super::normalize;
use crate::commands::{lock, off_main};
use crate::errors::Result;
use crate::state::AppState;

struct Known {
    name: &'static str,
    /// Process names, lower-case, without `.exe`.
    processes: &'static [&'static str],
    /// Its default shortcuts, in Klipt's accelerator form.
    shortcuts: &'static [&'static str],
}

const KNOWN: &[Known] = &[
    Known {
        name: "NVIDIA overlay",
        processes: &["nvidia overlay", "nvidia share"],
        shortcuts: &["Alt+Z", "Alt+F1", "Alt+F9", "Alt+F10", "Alt+R"],
    },
    Known {
        name: "AMD Software",
        processes: &["radeonsoftware", "amdrsserv"],
        shortcuts: &["Ctrl+Shift+S", "Ctrl+Shift+E", "Ctrl+Shift+I", "Ctrl+Shift+R", "Alt+R", "Alt+Z"],
    },
    Known {
        name: "Xbox Game Bar",
        processes: &["gamebar", "gamebarftserver"],
        shortcuts: &["Super+G", "Super+Alt+G", "Super+Alt+R", "Super+Alt+PrintScreen"],
    },
    Known {
        name: "Discord",
        processes: &["discord", "discordptb", "discordcanary", "vesktop"],
        shortcuts: &["Shift+Backquote"],
    },
    Known { name: "Steam", processes: &["steam", "steamwebhelper"], shortcuts: &["Shift+Tab", "F12"] },
    Known { name: "Medal", processes: &["medal"], shortcuts: &["F8", "F9"] },
    Known { name: "Outplayed", processes: &["outplayed"], shortcuts: &["Ctrl+F12"] },
    Known {
        name: "ShareX",
        processes: &["sharex"],
        shortcuts: &["PrintScreen", "Ctrl+PrintScreen", "Shift+PrintScreen"],
    },
];

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutApp {
    pub name: String,
    /// Its default shortcuts.
    pub shortcuts: Vec<String>,
    /// Klipt's shortcuts that are among them.
    pub clashes: Vec<String>,
}

/// Two accelerators are the same keys (modifier order and spelling don't matter).
fn same_keys(a: &str, b: &str) -> bool {
    let parts = |s: &str| {
        let canonical = normalize(s).unwrap_or_else(|_| s.to_string());
        let mut p: Vec<String> = canonical.split('+').map(|x| x.to_ascii_lowercase()).collect();
        p.sort();
        p
    };
    parts(a) == parts(b)
}

fn process_key(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    lower.strip_suffix(".exe").unwrap_or(&lower).to_string()
}

/// Which known apps are among `running` (process names), and where they clash with `ours`.
pub fn find(running: &[String], ours: &[String]) -> Vec<ShortcutApp> {
    let running: Vec<String> = running.iter().map(|n| process_key(n)).collect();
    KNOWN
        .iter()
        .filter(|k| k.processes.iter().any(|p| running.iter().any(|r| r == p)))
        .map(|k| ShortcutApp {
            name: k.name.into(),
            shortcuts: k.shortcuts.iter().map(|s| s.to_string()).collect(),
            clashes: ours
                .iter()
                .filter(|o| !o.is_empty() && k.shortcuts.iter().any(|s| same_keys(o, s)))
                .cloned()
                .collect(),
        })
        .collect()
}

fn running_processes() -> Vec<String> {
    let Ok(mut system) = System::new() else { return Vec::new() };
    system.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
    system.processes().values().map(|p| p.name().to_string_lossy().to_string()).collect()
}

/// Running apps with their own shortcuts, for Settings → Hotkey.
#[tauri::command]
pub async fn shortcut_apps(app: tauri::AppHandle) -> Result<Vec<ShortcutApp>> {
    use tauri::Manager;
    off_main(move || {
        let s = lock(&app.state::<AppState>().settings).clone();
        find(&running_processes(), &[s.hotkey, s.screenshot_hotkey, s.record_hotkey])
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_running_apps_and_their_clashes() {
        let running = vec!["NVIDIA Overlay.exe".to_string(), "explorer.exe".into(), "Discord.exe".into()];
        let ours = vec!["Alt+F10".to_string(), "Alt+Shift+Backquote".into(), String::new()];
        let found = find(&running, &ours);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].name, "NVIDIA overlay");
        assert_eq!(found[0].clashes, vec!["Alt+F10".to_string()]);
        assert_eq!(found[1].name, "Discord");
        assert!(found[1].clashes.is_empty(), "Shift+` isn't Alt+Shift+`");
    }

    #[test]
    fn modifier_order_does_not_matter() {
        assert!(same_keys("Shift+Ctrl+S", "Ctrl+Shift+S"));
        assert!(!same_keys("Ctrl+S", "Ctrl+Shift+S"));
    }
}
