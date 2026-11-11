//! Starting Klipt at login on Linux, written by hand instead of through the autostart plugin.
//!
//! KDE's portal knows an app by the name of the `.desktop` file that started it. The plugin names
//! its autostart file `Klipt.desktop`, so a login start ran as app "Klipt": the GlobalShortcuts
//! portal refused it ("An app id is required") and the save hotkey never worked after a reboot.
//! Named after the app id, like the menu entry, both starts are the same app to KDE and share one
//! shortcut registration.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// The app id (tauri.conf.json `identifier`), which is also the menu entry's name.
const FILE_NAME: &str = "app.klipt.desktop.desktop";
/// What the autostart plugin wrote before; removed so Klipt doesn't start twice.
const OLD_FILE_NAME: &str = "Klipt.desktop";

fn autostart_dir() -> io::Result<PathBuf> {
    dirs::config_dir().map(|d| d.join("autostart")).ok_or_else(|| io::Error::other("no config directory"))
}

pub fn apply(enabled: bool) -> io::Result<()> {
    let exe = std::env::current_exe()?;
    apply_in(&autostart_dir()?, enabled, &exe)
}

fn apply_in(dir: &Path, enabled: bool, exe: &Path) -> io::Result<()> {
    remove_if_present(&dir.join(OLD_FILE_NAME))?;
    let file = dir.join(FILE_NAME);
    if !enabled {
        return remove_if_present(&file);
    }
    fs::create_dir_all(dir)?;
    // Rewritten every launch, so it points at the copy that's running (repo build vs package).
    fs::write(&file, entry(exe))
}

fn entry(exe: &Path) -> String {
    let exe = exe.display().to_string();
    // Desktop entry Exec quoting: spaces and specials need double quotes, with \ " ` $ escaped.
    let exe = if exe.chars().any(|c| c.is_whitespace() || "\"'\\`$;&|<>()*?#~".contains(c)) {
        let escaped: String =
            exe.chars().flat_map(|c| if "\"`$\\".contains(c) { vec!['\\', c] } else { vec![c] }).collect();
        format!("\"{escaped}\"")
    } else {
        exe
    };
    format!(
        "[Desktop Entry]\nType=Application\nName=Klipt\nComment=Start Klipt in the tray\n\
         Exec={exe} {}\nIcon=app.klipt.desktop\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
        crate::AUTOSTART_ARG
    )
}

fn remove_if_present(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_entry_is_named_after_the_app_id_and_replaces_the_old_one() {
        let dir = tempfile::tempdir().unwrap();
        let autostart = dir.path().join("autostart");
        fs::create_dir_all(&autostart).unwrap();
        fs::write(autostart.join(OLD_FILE_NAME), "old").unwrap();

        apply_in(&autostart, true, Path::new("/usr/bin/klipt-desktop")).unwrap();
        assert!(!autostart.join(OLD_FILE_NAME).exists(), "old entry removed");
        let text = fs::read_to_string(autostart.join(FILE_NAME)).unwrap();
        assert!(text.contains("\nExec=/usr/bin/klipt-desktop --autostart\n"), "{text}");
        assert!(text.contains("\nName=Klipt\n"), "{text}");

        apply_in(&autostart, false, Path::new("/usr/bin/klipt-desktop")).unwrap();
        assert!(!autostart.join(FILE_NAME).exists());
        // Turning off with nothing there is fine too.
        apply_in(&autostart, false, Path::new("/usr/bin/klipt-desktop")).unwrap();
    }

    #[test]
    fn the_entry_matches_the_menu_entrys_app_id() {
        let conf: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        assert_eq!(format!("{}.desktop", conf["identifier"].as_str().unwrap()), FILE_NAME);
        assert!(include_str!("../../../packaging/aur/PKGBUILD").contains(FILE_NAME), "menu entry has the same name");
    }

    #[test]
    fn paths_with_spaces_are_quoted() {
        let text = entry(Path::new("/home/a b/klipt-desktop"));
        assert!(text.contains("\nExec=\"/home/a b/klipt-desktop\" --autostart\n"), "{text}");
    }
}
