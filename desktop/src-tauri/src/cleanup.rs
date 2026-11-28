//! Automatic cleanup of old clips on this PC (Settings → Clips folder), and the folder's size.
//!
//! When it's on, clips saved more than `cleanup_days` days ago are deleted (file, library row
//! and thumbnail) at startup, every few hours, and right after the setting is saved. Only the kinds
//! checked in Settings are deleted (see [`CleanupKinds`]). Imported clips and clips with an upload
//! in progress or paused are always kept.

use std::fs;
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::commands::Uploads;
use crate::library::{Clip, Library};
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

/// Which kinds of clips the cleanup may delete (Settings → Cleanup).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CleanupKinds {
    /// Never uploaded.
    pub local: bool,
    pub uploaded: bool,
    pub favorites: bool,
}

impl CleanupKinds {
    pub fn from_settings(settings: &Settings) -> Self {
        CleanupKinds {
            local: settings.cleanup_local,
            uploaded: settings.cleanup_uploaded,
            favorites: settings.cleanup_favorites,
        }
    }

    /// Whether the cleanup may delete `clip` (once it's old enough): every kind it has is checked,
    /// and no upload of it is in progress or paused.
    pub fn allows(&self, clip: &Clip) -> bool {
        let kind_checked = if clip.server_clip_id.is_some() { self.uploaded } else { self.local };
        clip.upload_id.is_none() && (!clip.favorite || self.favorites) && kind_checked
    }
}

/// Deletes the clips `settings` say are too old. `busy` reports clips being uploaded or exported
/// right now (they're kept). The library lock is held per clip, never across the whole run.
pub fn clean(library: &Mutex<Library>, settings: &Settings, now_ms: i64, busy: impl Fn(&str) -> bool) -> Report {
    let mut report = Report::default();
    if !settings.cleanup_enabled {
        return report;
    }
    let before = now_ms - i64::from(settings.cleanup_days) * DAY_MS;
    let kinds = CleanupKinds::from_settings(settings);
    let candidates = match lock(library).cleanup_candidates(before) {
        Ok(clips) => clips,
        Err(e) => {
            crate::elog!("klipt: cleanup can't list old clips: {e}");
            return report;
        }
    };
    for clip in candidates.into_iter().filter(|c| kinds.allows(c) && !busy(&c.id)) {
        let size = fs::metadata(&clip.path).map(|m| m.len()).unwrap_or(0);
        let library = lock(library);
        // Starred or uploaded since the list was made: check it again.
        match library.get(&clip.id) {
            Ok(now) if kinds.allows(&now) => {}
            _ => continue,
        }
        match library.remove(&clip.id, true) {
            Ok(()) => {
                report.deleted += 1;
                report.freed_bytes += size;
            }
            // Usually the file is open in a player; the next run tries again.
            Err(e) => crate::elog!("klipt: cleanup can't delete {}: {e}", clip.path.display()),
        }
    }
    if report.deleted > 0 {
        crate::elog!("klipt: cleanup deleted {} old clips ({} bytes)", report.deleted, report.freed_bytes);
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
                crate::elog!("klipt: cleanup failed: {e}");
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
            has_voice: false,
            kind: crate::library::ClipKind::Clip,
            is_recording: false,
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

        let on = Settings { cleanup_enabled: true, cleanup_days: 30, cleanup_local: true, ..Settings::default() };
        let report = clean(&library, &on, now, |id| id == busy);
        assert_eq!(report, Report { deleted: 1, freed_bytes: 100 });
        assert!(!dir.path().join("old.mp4").exists());
        let library = lock(&library);
        assert!(library.get(&old).is_err());
        assert!(library.get(&busy).is_ok());
        assert!(library.get(&recent).is_ok());
    }

    fn get(library: &Mutex<Library>, id: &str) -> Clip {
        lock(library).get(id).unwrap()
    }

    #[test]
    fn allows_a_clip_only_when_every_kind_it_has_is_checked() {
        let dir = tempfile::tempdir().unwrap();
        let library = Mutex::new(Library::open(&dir.path().join("library.db")).unwrap());
        let local = add(&library, dir.path(), "local.mp4", 0);
        let uploaded = add(&library, dir.path(), "up.mp4", 0);
        let uploaded_fav = add(&library, dir.path(), "upfav.mp4", 0);
        let local_fav = add(&library, dir.path(), "fav.mp4", 0);
        let paused = add(&library, dir.path(), "paused.mp4", 0);
        {
            let lib = lock(&library);
            lib.mark_uploaded(&uploaded, "s1", "https://k/c/s1", "friends").unwrap();
            lib.mark_uploaded(&uploaded_fav, "s2", "https://k/c/s2", "friends").unwrap();
            lib.set_favorite(&uploaded_fav, true).unwrap();
            lib.set_favorite(&local_fav, true).unwrap();
            lib.set_upload_progress(&paused, Some(("tus-1", 10))).unwrap();
        }
        let k = |local, uploaded, favorites| CleanupKinds { local, uploaded, favorites };
        let (local, uploaded, uploaded_fav, local_fav, paused) = (
            get(&library, &local),
            get(&library, &uploaded),
            get(&library, &uploaded_fav),
            get(&library, &local_fav),
            get(&library, &paused),
        );
        assert!(k(true, false, false).allows(&local));
        assert!(!k(false, true, true).allows(&local));
        assert!(k(false, true, false).allows(&uploaded));
        assert!(!k(true, false, true).allows(&uploaded));
        assert!(!k(false, true, false).allows(&uploaded_fav));
        assert!(k(false, true, true).allows(&uploaded_fav));
        assert!(!k(true, false, false).allows(&local_fav));
        assert!(k(true, false, true).allows(&local_fav));
        assert!(!k(true, true, true).allows(&paused));
    }

    #[test]
    fn deletes_only_the_checked_kinds() {
        let dir = tempfile::tempdir().unwrap();
        let library = Mutex::new(Library::open(&dir.path().join("library.db")).unwrap());
        let now = 100 * DAY_MS;
        let local = add(&library, dir.path(), "local.mp4", now - 40 * DAY_MS);
        let uploaded = add(&library, dir.path(), "up.mp4", now - 40 * DAY_MS);
        let fav = add(&library, dir.path(), "fav.mp4", now - 40 * DAY_MS);
        {
            let lib = lock(&library);
            lib.mark_uploaded(&uploaded, "s1", "https://k/c/s1", "friends").unwrap();
            lib.set_favorite(&fav, true).unwrap();
        }
        // The defaults: uploaded clips only.
        let on = Settings { cleanup_enabled: true, cleanup_days: 30, ..Settings::default() };
        assert_eq!(clean(&library, &on, now, |_| false).deleted, 1);
        let lib = lock(&library);
        assert!(lib.get(&uploaded).is_err());
        assert!(lib.get(&local).is_ok());
        assert!(lib.get(&fav).is_ok());
    }

    #[test]
    fn rechecks_each_clip_with_the_same_rule() {
        let dir = tempfile::tempdir().unwrap();
        let library = Mutex::new(Library::open(&dir.path().join("library.db")).unwrap());
        let now = 100 * DAY_MS;
        let local = add(&library, dir.path(), "local.mp4", now - 40 * DAY_MS);
        let on = Settings { cleanup_enabled: true, cleanup_days: 30, cleanup_local: true, ..Settings::default() };
        // Starred after the list was made, just before it's deleted.
        let report = clean(&library, &on, now, |id| {
            lock(&library).set_favorite(id, true).unwrap();
            false
        });
        assert_eq!(report.deleted, 0);
        assert!(lock(&library).get(&local).is_ok());
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
