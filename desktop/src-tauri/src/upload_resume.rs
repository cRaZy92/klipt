// Carries on uploads that a quit, crash or sleep cut off: the library remembers them
// (`pending_uploads`), and this runs them again at launch and after logging in.

use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{AppHandle, Manager, State};

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

/// Set while a resume pass runs, so launch and login don't start two.
#[derive(Default)]
pub struct Resuming(AtomicBool);

/// Starts a resume pass in the background (at launch).
pub fn start(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move { resume_all(app).await });
}

/// Runs the remembered uploads one after another, oldest first, until they're done or one has
/// to wait for a login.
pub async fn resume_all(app: AppHandle) {
    let Some(resuming) = app.try_state::<Resuming>() else { return };
    if resuming.0.swap(true, Ordering::SeqCst) {
        return;
    }
    let mut tried: HashSet<String> = HashSet::new();
    loop {
        let state = app.state::<AppState>();
        let logged_in = auth::load(&state.data_dir).is_some();
        let rows = match lock(&state.library).pending_uploads() {
            Ok(rows) => rows,
            Err(e) => {
                crate::elog!("klipt: can't read unfinished uploads: {e}");
                break;
            }
        };
        // Each row once per pass: one that fails and stays (no connection) waits for the next.
        let Some(row) = rows.into_iter().find(|r| !tried.contains(&r.clip_id)) else { break };
        tried.insert(row.clip_id.clone());
        let path = lock(&state.library).get(&row.clip_id).ok().map(|c| c.path);
        match resume_action(path.as_deref(), logged_in) {
            ResumeAction::Wait => break,
            ResumeAction::Drop => {
                crate::elog!("klipt: dropping the unfinished upload of clip {}: the clip is gone", row.clip_id);
                if let Err(e) = lock(&state.library).remove_pending_upload(&row.clip_id) {
                    crate::elog!("klipt: can't forget the upload of clip {}: {e}", row.clip_id);
                }
            }
            ResumeAction::Start => {
                crate::elog!("klipt: carrying on the upload of clip {}", row.clip_id);
                match commands::prepare_upload(&app, &row.clip_id, &row.title, row.visibility, Some(row.tagged.clone()), Some(row.replace)) {
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
                    }
                }
            }
        }
    }
    resuming.0.store(false, Ordering::SeqCst);
}

/// The unfinished uploads, so the window can show their cards.
#[tauri::command]
pub fn pending_uploads(state: State<'_, AppState>) -> Result<Vec<PendingUpload>> {
    lock(&state.library).pending_uploads()
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
