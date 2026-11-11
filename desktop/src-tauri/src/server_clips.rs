//! Your uploads on the server, for My clips: list them, rename, change visibility, delete, and
//! download one back into the local library. Clips are the server's `Clip` JSON, passed through.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::Value;
use tauri::{AppHandle, Manager};

use crate::commands::{clean_title, clear_auth_if_current, keep_own_media};
use crate::errors::{AppError, Result};
use crate::library::{ClipView, NewClip};
use crate::state::AppState;
use crate::upload::api::ApiClient;
use crate::upload::auth::{self, Auth};
use crate::upload::Visibility;

const LIST_TIMEOUT: Duration = Duration::from_secs(20);
/// A video can be large and the connection slow.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const PAGE_SIZE: &str = "60";
/// 60 × 50 = 3000 uploads: far more than a friend group makes, and a bound if a cursor loops.
const MAX_PAGES: usize = 50;

/// The saved login and a client for its server, or `Unauthorized`.
fn login(app: &AppHandle, timeout: Duration) -> Result<(Auth, ApiClient, PathBuf)> {
    let data_dir = app.state::<AppState>().data_dir.clone();
    let saved = auth::load(&data_dir).ok_or(AppError::Unauthorized)?;
    let client = ApiClient::with_timeout(&saved.server_url, timeout)?;
    Ok((saved, client, data_dir))
}

/// Converts a server error; a dead token is forgotten so the app asks to log in again.
fn failed(data_dir: &Path, saved: &Auth, e: crate::upload::ApiError) -> AppError {
    let e = AppError::from(e);
    if matches!(e, AppError::Unauthorized) {
        if let Err(e) = clear_auth_if_current(data_dir, &saved.token) {
            eprintln!("klipt: can't forget the expired login: {e}");
        }
    }
    e
}

/// Server clip ids are 12 URL-safe characters; anything else never reaches a URL path.
fn checked_id(id: &str) -> Result<&str> {
    let ok = id.len() == 12 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    ok.then_some(id).ok_or_else(|| AppError::InvalidInput("not a server clip id".into()))
}

/// All your uploads, newest first; None when you aren't logged in.
#[tauri::command]
pub async fn my_uploads(app: AppHandle) -> Result<Option<Vec<Value>>> {
    let (saved, client, data_dir) = match login(&app, LIST_TIMEOUT) {
        Ok(found) => found,
        Err(AppError::Unauthorized) => return Ok(None),
        Err(e) => return Err(e),
    };
    let mut items = Vec::new();
    let mut cursor: Option<String> = None;
    for _ in 0..MAX_PAGES {
        let mut query = vec![("scope", "mine".to_string()), ("limit", PAGE_SIZE.to_string())];
        query.extend(cursor.take().map(|c| ("cursor", c)));
        let mut page =
            client.get_json(&saved.token, "/api/clips", &query).await.map_err(|e| failed(&data_dir, &saved, e))?;
        keep_own_media(&mut page, &client);
        if let Some(list) = page.get_mut("items").and_then(Value::as_array_mut) {
            items.append(list);
        }
        cursor = page.get("nextCursor").and_then(Value::as_str).filter(|c| !c.is_empty()).map(String::from);
        if cursor.is_none() {
            break;
        }
    }
    Ok(Some(items))
}

/// Renames the server copy and/or changes who can see it. Returns the updated clip.
#[tauri::command]
pub async fn update_upload(
    app: AppHandle,
    id: String,
    title: Option<String>,
    visibility: Option<Visibility>,
) -> Result<Value> {
    let id = checked_id(&id)?;
    let mut body = serde_json::Map::new();
    if let Some(title) = title {
        body.insert("title".into(), clean_title(&title)?.into());
    }
    if let Some(visibility) = visibility {
        body.insert("visibility".into(), serde_json::to_value(visibility)?);
    }
    let (saved, client, data_dir) = login(&app, LIST_TIMEOUT)?;
    let clip = client
        .patch_json(&saved.token, &format!("/api/clips/{id}"), &Value::Object(body))
        .await
        .map_err(|e| failed(&data_dir, &saved, e))?;
    if let Some(visibility) = clip.get("visibility").and_then(Value::as_str) {
        let state = app.state::<AppState>();
        crate::commands::lock(&state.library).set_server_visibility(id, visibility)?;
    }
    Ok(clip)
}

/// Deletes the server copy. A local clip it was uploaded from stays, as not uploaded.
#[tauri::command]
pub async fn delete_upload(app: AppHandle, id: String) -> Result<()> {
    let id = checked_id(&id)?;
    let (saved, client, data_dir) = login(&app, LIST_TIMEOUT)?;
    match client.delete(&saved.token, &format!("/api/clips/{id}")).await {
        Ok(()) => {}
        // Already gone (e.g. deleted on the website): the local link goes too.
        Err(crate::upload::ApiError::Server { status: 404, .. }) => {}
        Err(e) => return Err(failed(&data_dir, &saved, e)),
    }
    let state = app.state::<AppState>();
    crate::commands::lock(&state.library).unlink_server_clip(id)?;
    Ok(())
}

/// Downloads your upload into the clips folder and adds it to the library, linked to the server
/// copy (so it shows as uploaded and isn't listed twice).
#[tauri::command]
pub async fn download_upload(app: AppHandle, id: String) -> Result<ClipView> {
    let id = checked_id(&id)?.to_string();
    let (saved, client, data_dir) = login(&app, DOWNLOAD_TIMEOUT)?;
    let state = app.state::<AppState>();
    // A library entry whose file is gone is replaced by the download.
    let stale = match crate::commands::lock(&state.library).find_by_server_id(&id)? {
        Some(local) if local.path.is_file() => {
            return Err(AppError::InvalidInput("that clip is already on this computer".into()));
        }
        found => found.map(|c| c.id),
    };
    let clip = client
        .get_json(&saved.token, &format!("/api/clips/{id}"), &[])
        .await
        .map_err(|e| failed(&data_dir, &saved, e))?;
    let info = ServerClipInfo::from_json(&clip).ok_or_else(|| AppError::Internal("unexpected clip from the server".into()))?;

    let output_dir = crate::commands::lock(&state.settings).output_dir.clone();
    std::fs::create_dir_all(&output_dir)?;
    let dest = free_path(&output_dir, &file_name(&info.title, &id));
    let mut resp = client.get_media(&info.video_url).await.map_err(|e| failed(&data_dir, &saved, e))?;
    let part = dest.with_extension("mp4.part");
    let written = async {
        let mut file = std::fs::File::create(&part)?;
        while let Some(chunk) = resp.chunk().await.map_err(|e| AppError::Network(e.without_url().to_string()))? {
            file.write_all(&chunk)?;
        }
        file.sync_all()?;
        std::fs::rename(&part, &dest)?;
        Ok::<(), AppError>(())
    }
    .await;
    if let Err(e) = written {
        let _ = std::fs::remove_file(&part);
        return Err(e);
    }

    let new = NewClip {
        path: dest.clone(),
        created_at: info.created_at_ms,
        duration_ms: info.duration_ms,
        width: info.width,
        height: info.height,
        // Uploads have one mixed audio track.
        has_mic: false,
        game_name: info.game_name,
        steam_app_id: info.steam_app_id,
        exe: None,
        title: info.title,
    };
    let (clip, thumb) = {
        let library = crate::commands::lock(&state.library);
        let clip = library.insert(new)?;
        library.mark_uploaded(&clip.id, &id, &info.share_url, &info.visibility)?;
        if let Some(stale) = stale {
            if let Err(e) = library.remove(&stale, false) {
                eprintln!("klipt: can't drop the old entry of a downloaded clip: {e}");
            }
        }
        let thumb = library.thumb_path(&clip.id);
        (library.get(&clip.id)?, thumb)
    };
    if let Some(tools) = state.tools.as_ref() {
        let at = crate::recorder::thumbnail_at_ms(clip.duration_ms);
        if let Err(e) = crate::ffmpeg::thumbnail(tools, &clip.path, &thumb, at) {
            eprintln!("klipt: can't make a thumbnail for {}: {e}", clip.path.display());
        }
    }
    Ok(ClipView { file_missing: !clip.path.is_file(), thumb_path: thumb.is_file().then_some(thumb), clip })
}

/// What a download needs from the server's clip JSON.
struct ServerClipInfo {
    title: String,
    game_name: Option<String>,
    steam_app_id: Option<u32>,
    duration_ms: u64,
    width: u32,
    height: u32,
    created_at_ms: i64,
    share_url: String,
    video_url: String,
    visibility: String,
}

impl ServerClipInfo {
    fn from_json(v: &Value) -> Option<ServerClipInfo> {
        let s = |k: &str| v.get(k).and_then(Value::as_str).map(String::from);
        let n = |k: &str| v.get(k).and_then(Value::as_u64);
        let created_at_ms = s("createdAt")
            .and_then(|t| chrono_free_parse_ms(&t))
            .unwrap_or_else(|| {
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
            });
        Some(ServerClipInfo {
            title: s("title")?,
            game_name: s("gameName"),
            steam_app_id: n("steamAppId").and_then(|id| u32::try_from(id).ok()),
            duration_ms: n("durationMs").unwrap_or(0),
            width: n("width").and_then(|w| u32::try_from(w).ok()).unwrap_or(0),
            height: n("height").and_then(|h| u32::try_from(h).ok()).unwrap_or(0),
            created_at_ms,
            share_url: s("shareUrl")?,
            video_url: s("videoUrl")?,
            visibility: s("visibility").unwrap_or_else(|| "feed".into()),
        })
    }
}

/// Parses the server's `createdAt` (ISO 8601 UTC, e.g. `2026-11-10T21:03:05.123Z`) to Unix ms.
fn chrono_free_parse_ms(t: &str) -> Option<i64> {
    let t = t.strip_suffix('Z')?;
    let (date, time) = t.split_once('T')?;
    let mut d = date.split('-').map(|p| p.parse::<i64>().ok());
    let (y, m, day) = (d.next()??, d.next()??, d.next()??);
    let (hms, frac) = time.split_once('.').unwrap_or((time, "0"));
    let mut h = hms.split(':').map(|p| p.parse::<i64>().ok());
    let (hh, mm, ss) = (h.next()??, h.next()??, h.next()??);
    let ms: i64 = format!("{frac:0<3}")[..3].parse().ok()?;
    // Days from 1970-01-01 (civil calendar).
    let (y, m) = if m <= 2 { (y - 1, m + 9) } else { (y, m - 3) };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * m + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(((days * 24 + hh) * 60 + mm) * 60_000 + ss * 1000 + ms)
}

/// `<title> (<id>).mp4`, with characters Windows and Linux don't allow in file names replaced.
fn file_name(title: &str, id: &str) -> String {
    let clean: String = title
        .chars()
        .map(|c| if c.is_control() || "/\\:*?\"<>|".contains(c) { '_' } else { c })
        .collect::<String>()
        .trim()
        .trim_end_matches('.')
        .chars()
        .take(80)
        .collect();
    let clean = if clean.is_empty() { "Clip".to_string() } else { clean };
    format!("{clean} ({id}).mp4")
}

/// `dir/name`, or `dir/name (2)` … when that's taken.
fn free_path(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let stem = name.trim_end_matches(".mp4");
    (2..).map(|n| dir.join(format!("{stem} {n}.mp4"))).find(|p| !p.exists()).unwrap_or(first)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_ids_are_checked_before_they_reach_a_path() {
        assert!(checked_id("AbC_12-xyz09").is_ok());
        for bad in ["", "short", "../../../etc", "AbC_12-xyz0/", "AbC_12-xyz09x"] {
            assert!(checked_id(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn iso_times_parse_to_unix_ms() {
        assert_eq!(chrono_free_parse_ms("1970-01-01T00:00:00.000Z"), Some(0));
        assert_eq!(chrono_free_parse_ms("2026-11-10T21:03:05.123Z"), Some(1_794_344_585_123));
        assert_eq!(chrono_free_parse_ms("2000-02-29T12:00:00Z"), Some(951_825_600_000));
        assert_eq!(chrono_free_parse_ms("yesterday"), None);
    }

    #[test]
    fn download_names_are_safe_and_unique() {
        assert_eq!(file_name("Deadlock — 10 Nov 2026", "AbC_12-xyz09"), "Deadlock — 10 Nov 2026 (AbC_12-xyz09).mp4");
        assert_eq!(file_name("a/b:c*?", "AbC_12-xyz09"), "a_b_c__ (AbC_12-xyz09).mp4");
        assert_eq!(file_name("  ...", "AbC_12-xyz09"), "Clip (AbC_12-xyz09).mp4");
        let dir = tempfile::tempdir().unwrap();
        let name = "x (AbC_12-xyz09).mp4";
        assert_eq!(free_path(dir.path(), name), dir.path().join(name));
        std::fs::write(dir.path().join(name), b"").unwrap();
        assert_eq!(free_path(dir.path(), name), dir.path().join("x (AbC_12-xyz09) 2.mp4"));
    }

    #[test]
    fn server_clip_json_gives_what_a_download_needs() {
        let v = serde_json::json!({
            "id": "AbC_12-xyz09", "title": "Ace", "gameName": "Deadlock", "steamAppId": 1422450,
            "durationMs": 30000, "width": 1920, "height": 1080, "createdAt": "1970-01-01T00:00:01.000Z",
            "shareUrl": "https://k.example/c/AbC_12-xyz09", "videoUrl": "https://k.example/media/AbC_12-xyz09.mp4",
            "thumbnailUrl": "https://k.example/media/AbC_12-xyz09.jpg", "visibility": "link",
        });
        let info = ServerClipInfo::from_json(&v).unwrap();
        assert_eq!((info.title.as_str(), info.steam_app_id, info.created_at_ms), ("Ace", Some(1_422_450), 1000));
        assert_eq!(info.visibility, "link");
        assert!(ServerClipInfo::from_json(&serde_json::json!({ "title": "x" })).is_none());
    }
}
