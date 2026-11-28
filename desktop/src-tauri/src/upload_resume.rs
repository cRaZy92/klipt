// Carries on uploads that a quit, crash or sleep cut off: the library remembers them
// (`pending_uploads`), and this runs them again at launch and after logging in.

use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;

use tauri::{AppHandle, Manager};

use crate::commands::{self, lock};
use crate::errors::{AppError, Result};
use crate::library::PendingUpload;
use crate::state::AppState;
use crate::upload::auth;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResumeAction {
    Start,
    /// The clip is gone: forget the upload.
    Drop,
    /// Not logged in: keep it for after the login.
    Wait,
}

/// What to do with a remembered upload whose clip's file is `clip_path` (None: the clip is no
/// longer in the library).
pub fn resume_action(clip_path: Option<&Path>, logged_in: bool) -> ResumeAction {
    if !logged_in {
        return ResumeAction::Wait;
    }
    match clip_path {
        Some(path) if path.is_file() => ResumeAction::Start,
        _ => ResumeAction::Drop,
    }
}

/// How long kept uploads (no connection yet, e.g. right after an autostart) wait to be tried again.
pub const RETRY_EVERY: Duration = Duration::from_secs(120);

/// When to try the kept uploads again after a pass: while some are left and there's a login.
pub fn retry_after(rows_left: usize, logged_in: bool) -> Option<Duration> {
    (rows_left > 0 && logged_in).then_some(RETRY_EVERY)
}

/// What a remembered upload's card shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CardStatus {
    /// Uploading now.
    Running,
    /// The resume pass will get to it.
    Queued,
    /// Not logged in.
    NeedsLogin,
    /// It stopped and is tried again in a while; "Try again" starts it now.
    Waiting,
}

pub fn card_status(active: bool, logged_in: bool, pass_running: bool, tried_this_pass: bool) -> CardStatus {
    if active {
        CardStatus::Running
    } else if !logged_in {
        CardStatus::NeedsLogin
    } else if pass_running && !tried_this_pass {
        CardStatus::Queued
    } else {
        CardStatus::Waiting
    }
}

/// The resume pass: whether one runs, the rows it tried, and how many it has still to try.
#[derive(Default)]
pub struct Resuming {
    running: AtomicBool,
    tried: Mutex<HashSet<String>>,
    left: AtomicUsize,
    /// A retry is already scheduled.
    retry_scheduled: AtomicBool,
}

impl Resuming {
    /// Rows the pass has yet to try (they count as queued for the "uploaded" notice).
    pub fn left(&self) -> usize {
        self.left.load(Ordering::SeqCst)
    }
}

/// Starts a resume pass in the background (at launch).
pub fn start(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move { resume_all(app).await });
}

/// Tells the window a remembered upload ended without running.
fn report_failed(app: &AppHandle, id: &str, e: &AppError) {
    commands::emit_upload_failed(app, id, e);
}

/// Runs the remembered uploads one after another, oldest first, until they're done or one has
/// to wait for a login. Ones kept for want of a connection are tried again in a while.
pub async fn resume_all(app: AppHandle) {
    let Some(resuming) = app.try_state::<Resuming>() else { return };
    if resuming.running.swap(true, Ordering::SeqCst) {
        return;
    }
    lock(&resuming.tried).clear();
    let mut logged_in;
    loop {
        let state = app.state::<AppState>();
        logged_in = auth::load(&state.data_dir).is_some();
        // A clip mid-move looks missing (and would be dropped): wait for the next pass.
        if crate::move_clips::is_moving() {
            break;
        }
        let rows = match lock(&state.library).pending_uploads() {
            Ok(rows) => rows,
            Err(e) => {
                crate::elog!("klipt: can't read unfinished uploads: {e}");
                break;
            }
        };
        // Each row once per pass: one that fails and stays (no connection) waits for the next.
        let todo: Vec<_> = {
            let tried = lock(&resuming.tried);
            rows.into_iter().filter(|r| !tried.contains(&r.clip_id)).collect()
        };
        let Some(row) = todo.first().cloned() else { break };
        resuming.left.store(todo.len() - 1, Ordering::SeqCst);
        lock(&resuming.tried).insert(row.clip_id.clone());
        let path = match lock(&state.library).get(&row.clip_id) {
            Ok(clip) => Some(clip.path),
            Err(AppError::ClipNotFound(_)) => None,
            Err(e) => {
                crate::elog!("klipt: can't read clip {} to carry on its upload: {e}", row.clip_id);
                continue;
            }
        };
        match resume_action(path.as_deref(), logged_in) {
            ResumeAction::Wait => break,
            ResumeAction::Drop => {
                crate::elog!("klipt: dropping the unfinished upload of clip {}: the clip is gone", row.clip_id);
                if let Err(e) = lock(&state.library).remove_pending_upload(&row.clip_id) {
                    crate::elog!("klipt: can't forget the upload of clip {}: {e}", row.clip_id);
                }
                report_failed(&app, &row.clip_id, &AppError::FileMissing);
            }
            ResumeAction::Start => {
                crate::elog!("klipt: carrying on the upload of clip {}", row.clip_id);
                match commands::prepare_upload(
                    &app,
                    &row.clip_id,
                    &row.title,
                    row.visibility,
                    commands::UploadOptions {
                        tagged: Some(row.tagged.clone()),
                        replace: Some(row.replace),
                        high_quality: row.high_quality,
                        albums: Some(row.albums.clone()),
                    },
                ) {
                    Ok((job, guard)) => {
                        commands::run_job(app.clone(), job, guard).await;
                    }
                    // The window is already on it.
                    Err(AppError::AlreadyUploading) => {}
                    Err(e) => {
                        crate::elog!("klipt: can't carry on the upload of clip {}: {e}", row.clip_id);
                        if !e.keeps_upload_queued() {
                            let _ = lock(&state.library).remove_pending_upload(&row.clip_id);
                        }
                        report_failed(&app, &row.clip_id, &e);
                    }
                }
            }
        }
    }
    resuming.left.store(0, Ordering::SeqCst);
    resuming.running.store(false, Ordering::SeqCst);
    let rows_left = lock(&app.state::<AppState>().library).pending_uploads().map(|r| r.len()).unwrap_or(0);
    if let Some(wait) = retry_after(rows_left, logged_in) {
        if !resuming.retry_scheduled.swap(true, Ordering::SeqCst) {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(wait).await;
                app.state::<Resuming>().retry_scheduled.store(false, Ordering::SeqCst);
                // Through `start` (a plain fn): awaiting `resume_all` in itself can't be `Send`.
                start(&app);
            });
        }
    }
}

/// A remembered upload with what its card shows.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingCard {
    #[serde(flatten)]
    pub upload: PendingUpload,
    pub status: CardStatus,
}

/// The unfinished uploads, so the window can show their cards.
#[tauri::command]
pub fn pending_uploads(app: AppHandle) -> Result<Vec<PendingCard>> {
    let state = app.state::<AppState>();
    let logged_in = auth::load(&state.data_dir).is_some();
    let resuming = app.state::<Resuming>();
    let pass_running = resuming.running.load(Ordering::SeqCst);
    let rows = lock(&state.library).pending_uploads()?;
    let uploads = app.state::<commands::Uploads>();
    let tried = lock(&resuming.tried);
    Ok(rows
        .into_iter()
        .map(|upload| {
            let status = card_status(uploads.is_active(&upload.clip_id), logged_in, pass_running, tried.contains(&upload.clip_id));
            PendingCard { upload, status }
        })
        .collect())
}

/// Carries on the unfinished uploads (after logging in). Returns once the pass has started.
#[tauri::command]
pub fn resume_pending_uploads(app: AppHandle) {
    start(&app);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn card_status_follows_what_the_core_does() {
        assert_eq!(card_status(true, true, false, false), CardStatus::Running);
        assert_eq!(card_status(false, false, false, false), CardStatus::NeedsLogin);
        assert_eq!(card_status(false, true, true, false), CardStatus::Queued);
        // Tried in this pass and kept (no connection): retryable, and retried later by itself.
        assert_eq!(card_status(false, true, true, true), CardStatus::Waiting);
        assert_eq!(card_status(false, true, false, false), CardStatus::Waiting);
    }

    #[test]
    fn kept_uploads_are_tried_again_later() {
        assert_eq!(retry_after(2, true), Some(RETRY_EVERY));
        assert_eq!(retry_after(0, true), None);
        assert_eq!(retry_after(2, false), None);
    }

    #[test]
    fn resume_waits_without_login() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.mp4");
        std::fs::write(&file, b"x").unwrap();
        assert_eq!(resume_action(Some(&file), false), ResumeAction::Wait);
        assert_eq!(resume_action(None, false), ResumeAction::Wait);
    }

    #[test]
    fn resume_drops_a_deleted_clip() {
        assert_eq!(resume_action(None, true), ResumeAction::Drop);
    }

    #[test]
    fn resume_drops_a_clip_whose_file_is_gone() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(resume_action(Some(&dir.path().join("gone.mp4")), true), ResumeAction::Drop);
    }

    #[test]
    fn resume_starts_a_present_clip() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.mp4");
        std::fs::write(&file, b"x").unwrap();
        assert_eq!(resume_action(Some(&file), true), ResumeAction::Start);
    }
}
