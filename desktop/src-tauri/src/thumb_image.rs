//! A thumbnail picked from an image instead of a frame of the video: set in the editor (kept in the
//! library and sent after each upload) or on an online clip from its card (sent to the server
//! directly). The server answers `PUT /api/clips/:id/thumbnail` with the updated clip.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;
use tauri::{AppHandle, Manager};

use crate::commands::{lock, off_main};
use crate::errors::{AppError, Result};
use crate::ffmpeg::Tools;
use crate::state::AppState;
use crate::upload::api::ApiClient;
use crate::upload::auth;

/// A picture is small, but the connection may be slow.
const SEND_TIMEOUT: Duration = Duration::from_secs(60);

/// Cuts `source` to the clip's shape into `image`, then makes the card thumbnail `thumb` from it.
pub fn make_local(tools: &Tools, source: &Path, image: &Path, thumb: &Path, width: u32, height: u32) -> Result<()> {
    crate::ffmpeg::crop_image(tools, source, image, width, height).map_err(not_a_picture)?;
    crate::ffmpeg::thumbnail(tools, image, thumb, 0)
}

/// ffmpeg couldn't read the file: almost always one that isn't a picture.
fn not_a_picture(e: AppError) -> AppError {
    match e {
        AppError::ToolFailed { .. } => AppError::InvalidInput("Klipt can't read that file as a picture".into()),
        e => e,
    }
}

/// Sends `image` (a JPEG) as server clip `server_id`'s thumbnail; returns the updated clip.
pub async fn send(client: &ApiClient, token: &str, server_id: &str, image: &Path) -> Result<Value> {
    let bytes = tokio::fs::read(image).await?;
    Ok(client.put_bytes(token, &format!("/api/clips/{server_id}/thumbnail"), "image/jpeg", bytes).await?)
}

/// Like [`send`], but a dead login is forgotten so the app asks to log in again.
async fn send_as(client: &ApiClient, saved: &auth::Auth, data_dir: &Path, server_id: &str, image: &Path) -> Result<Value> {
    let bytes = tokio::fs::read(image).await?;
    client
        .put_bytes(&saved.token, &format!("/api/clips/{server_id}/thumbnail"), "image/jpeg", bytes)
        .await
        .map_err(|e| crate::server_clips::failed(data_dir, saved, e))
}

/// Sets (with `path`) or clears local clip `id`'s thumbnail picture. Cleared, the thumbnail is the
/// chosen (or automatic) frame again. A picture set on an uploaded clip goes to the server too;
/// returns why that failed, if it did (the picture is set on this PC either way).
#[tauri::command]
pub async fn set_thumbnail_image(app: AppHandle, id: String, path: Option<PathBuf>) -> Result<Option<String>> {
    let set = path.is_some();
    let app2 = app.clone();
    let (server_id, image) = off_main(move || {
        let app = app2;
        let state = app.state::<AppState>();
        let (clip, image, thumb) = {
            let library = lock(&state.library);
            (library.get(&id)?, library.thumb_image_path(&id), library.thumb_path(&id))
        };
        let tools = state.tools.clone().ok_or(AppError::ToolsMissing)?;
        match path {
            Some(source) => make_local(&tools, &source, &image, &thumb, clip.width, clip.height)?,
            None => {
                if let Err(e) = std::fs::remove_file(&image) {
                    if e.kind() != std::io::ErrorKind::NotFound {
                        return Err(e.into());
                    }
                }
                let at = clip.thumbnail_ms.unwrap_or_else(|| crate::recorder::thumbnail_at_ms(clip.duration_ms));
                crate::ffmpeg::thumbnail(&tools, &clip.path, &thumb, at)?;
            }
        }
        crate::commands::thumbnail_changed(&app, &id);
        Ok::<_, AppError>((clip.server_clip_id, image))
    })
    .await??;
    let Some(server_id) = server_id.filter(|_| set) else { return Ok(None) };
    let state = app.state::<AppState>();
    let Some(saved) = auth::load(&state.data_dir) else {
        return Ok(Some("not logged in, so the online clip keeps its thumbnail".into()));
    };
    let client = ApiClient::with_timeout(&saved.server_url, SEND_TIMEOUT)?;
    Ok(send_as(&client, &saved, &state.data_dir, &server_id, &image).await.err().map(|e| e.to_string()))
}

/// Sets online clip `id`'s thumbnail from the picture at `path`, cut to the clip's `width`:`height`.
/// A local clip uploaded as it gets the picture too, so a later re-upload keeps it. Returns the
/// updated server clip.
#[tauri::command]
pub async fn set_upload_thumbnail(app: AppHandle, id: String, path: PathBuf, width: u32, height: u32) -> Result<Value> {
    let id = crate::server_clips::checked_id(&id)?.to_string();
    let state = app.state::<AppState>();
    let tools = state.tools.clone().ok_or(AppError::ToolsMissing)?;
    let saved = auth::load(&state.data_dir).ok_or(AppError::Unauthorized)?;
    let client = ApiClient::with_timeout(&saved.server_url, SEND_TIMEOUT)?;
    let cache = app.path().app_cache_dir().map_err(|e| AppError::Io(std::io::Error::other(e.to_string())))?;
    let cropped = cache.join(format!("thumb-{id}.jpg"));
    let crop = {
        let (tools, path, cropped) = (tools.clone(), path.clone(), cropped.clone());
        off_main(move || crate::ffmpeg::crop_image(&tools, &path, &cropped, width, height).map_err(not_a_picture)).await?
    };
    crop?;
    let sent = send_as(&client, &saved, &state.data_dir, &id, &cropped).await;
    let result = match sent {
        Ok(clip) => {
            let local = lock(&state.library).find_by_server_id(&id);
            if let Ok(Some(local)) = local {
                let (image, thumb) = {
                    let library = lock(&state.library);
                    (library.thumb_image_path(&local.id), library.thumb_path(&local.id))
                };
                let (app2, source) = (app.clone(), cropped.clone());
                let made = off_main(move || {
                    make_local(&tools, &source, &image, &thumb, local.width, local.height)?;
                    crate::commands::thumbnail_changed(&app2, &local.id);
                    Ok::<_, AppError>(())
                })
                .await;
                if let Err(e) | Ok(Err(e)) = made {
                    crate::elog!("klipt: can't set the local thumbnail of online clip {id}: {e}");
                }
            }
            Ok(clip)
        }
        Err(e) => Err(e),
    };
    let _ = std::fs::remove_file(&cropped);
    result
}
