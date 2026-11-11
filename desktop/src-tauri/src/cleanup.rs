//! Automatic cleanup of old clips on this PC (Settings → Clips folder), and the folder's size.
//!
//! When it's on, clips saved more than `cleanup_days` days ago are deleted (file, library row
//! and thumbnail) at startup, every few hours, and right after the setting is saved. Favorites,
//! uploaded clips and clips with an upload in progress or paused are always kept.

use std::fs;
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::commands::Uploads;
use crate::library::Library;
use crate::settings::Settings;
use crate::state::AppState;

const DAY_MS: i64 = 24 * 60 * 60 * 1000;
/// The first run waits a little, so it doesn't compete with startup.
const FIRST_RUN_AFTER: Duration = Duration::from_secs(60);
const RUN_EVERY: Duration = Duration::from_secs(6 * 60 * 60);
/// Tells the UI to reload the library.
pub const CLIPS_CLEANED: &str = "clips-cleaned";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub deleted: u32,
    pub freed_bytes: u64,
}

/// Deletes the clips `settings` say are too old. `busy` reports clips being uploaded or exported
/// right now (they're kept). The library lock is held per clip, never across the whole run.
pub fn clean(library: &Mutex<Library>, settings: &Settings, now_ms: i64, busy: impl Fn(&str) -> bool) -> Report {
    let mut report = Report::default();
    if !settings.cleanup_enabled {
        return report;
    }
    let before = now_ms - i64::from(settings.cleanup_days) * DAY_MS;
    let candidates = match lock(library).cleanup_candidates(before) {
        Ok(clips) => clips,
        Err(e) => {
            eprintln!("klipt: cleanup can't list old clips: {e}");
            return report;
        }
    };
    for clip in candidates.into_iter().filter(|c| !busy(&c.id)) {
        let size = fs::metadata(&clip.path).map(|m| m.len()).unwrap_or(0);
        let library = lock(library);
        // Starred or uploaded since the list was made: keep it.
        match library.get(&clip.id) {
            Ok(now) if !now.favorite && now.server_clip_id.is_none() && now.upload_id.is_none() => {}
            _ => continue,
        }
        match library.remove(&clip.id, true) {
            Ok(()) => {
                report.deleted += 1;
                report.freed_bytes += size;
            }
            // Usually the file is open in a player; the next run tries again.
            Err(e) => eprintln!("klipt: cleanup can't delete {}: {e}", clip.path.display()),
        }
    }
    if report.deleted > 0 {
        eprintln!("klipt: cleanup deleted {} old clips ({} bytes)", report.deleted, report.freed_bytes);
    }
    report
}

/// Runs [`clean`] with the app's current settings and tells the UI when clips went away.
pub fn run_now(app: &AppHandle) -> Report {
    let state = app.state::<AppState>();
    let settings = lock(&state.settings).clone();
    let uploads = app.state::<Uploads>();
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let report = clean(&state.library, &settings, now_ms, |id| uploads.is_active(id));
    if report.deleted > 0 {
        let _ = app.emit(CLIPS_CLEANED, report);
    }
    report
}

/// Starts the periodic cleanup (it does nothing while the setting is off).
pub fn start(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_RUN_AFTER).await;
        loop {
            let handle = app.clone();
            if let Err(e) = tauri::async_runtime::spawn_blocking(move || run_now(&handle)).await {
                eprintln!("klipt: cleanup failed: {e}");
            }
            tokio::time::sleep(RUN_EVERY).await;
        }
    });
}

/// The total size of the files in `dir` and its subfolders. Links aren't followed; unreadable
/// entries are skipped.
pub fn folder_size(dir: &Path) -> u64 {
    let Ok(entries) = fs::read_dir(dir) else { return 0 };
    entries
        .flatten()
        .map(|entry| match entry.file_type() {
            Ok(t) if t.is_dir() => folder_size(&entry.path()),
            Ok(t) if t.is_file() => entry.metadata().map(|m| m.len()).unwrap_or(0),
            _ => 0,
        })
        .sum()
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::NewClip;

    fn add(library: &Mutex<Library>, dir: &Path, name: &str, created_at: i64) -> String {
        let path = dir.join(name);
        fs::write(&path, vec![0u8; 100]).unwrap();
        let clip = NewClip {
            path,
            created_at,
            duration_ms: 1000,
            width: 1920,
            height: 1080,
            has_mic: false,
            game_name: None,
            steam_app_id: None,
            exe: None,
            title: name.into(),
        };
        lock(library).insert(clip).unwrap().id
    }

    #[test]
    fn deletes_old_clips_only_when_on_and_keeps_busy_ones() {
        let dir = tempfile::tempdir().unwrap();
        let library = Mutex::new(Library::open(&dir.path().join("library.db")).unwrap());
        let now = 100 * DAY_MS;
        let old = add(&library, dir.path(), "old.mp4", now - 31 * DAY_MS);
        let busy = add(&library, dir.path(), "busy.mp4", now - 40 * DAY_MS);
        let recent = add(&library, dir.path(), "recent.mp4", now - 29 * DAY_MS);

        let off = Settings { cleanup_enabled: false, cleanup_days: 30, ..Settings::default() };
        assert_eq!(clean(&library, &off, now, |_| false), Report::default());

        let on = Settings { cleanup_enabled: true, cleanup_days: 30, ..Settings::default() };
        let report = clean(&library, &on, now, |id| id == busy);
        assert_eq!(report, Report { deleted: 1, freed_bytes: 100 });
        assert!(!dir.path().join("old.mp4").exists());
        let library = lock(&library);
        assert!(library.get(&old).is_err());
        assert!(library.get(&busy).is_ok());
        assert!(library.get(&recent).is_ok());
    }

    #[test]
    fn folder_size_adds_up_files_in_subfolders() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.mp4"), vec![0u8; 10]).unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();
        fs::write(dir.path().join("sub").join("b.mp4"), vec![0u8; 5]).unwrap();
        assert_eq!(folder_size(dir.path()), 15);
        assert_eq!(folder_size(&dir.path().join("missing")), 0);
    }
}
