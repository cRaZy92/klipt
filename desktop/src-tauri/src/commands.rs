//! Tauri commands for the UI: thin wrappers over the app core. Every command returns
//! `Result<T, AppError>`, which reaches the UI as `{ code, message }`.
//!
//! Uploads are long: `upload_clip` validates, starts a background task and returns at once. The
//! task reports through `upload-progress`, then `upload-done` or `upload-failed` (see [`events`]).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_opener::OpenerExt;

use crate::errors::{AppError, Result};
use crate::export::{self, Edit};
use crate::ffmpeg::Tools;
use crate::games::mappings;
use crate::hotkey::{HotkeyStatus, HotkeyStatusState};
use crate::library::{Clip, ClipView, GameCount, ListFilter};
use crate::recorder::SupervisorHandle;
use crate::settings::Settings;
use crate::state::AppState;
use crate::upload::auth::{self, Auth};
use crate::upload::{tus, ApiClient, ApiError, FinalizeRequest, Me, Visibility};

/// Upload events the UI listens to.
pub mod events {
    /// Payload: `{ id, sent, total, phase }`, phase `exporting` | `uploading` | `finalizing`.
    /// While exporting, `sent`/`total` are thousandths of the export.
    pub const UPLOAD_PROGRESS: &str = "upload-progress";
    /// Payload: `{ id, shareUrl, copied }`; `copied` says whether the link is on the clipboard.
    pub const UPLOAD_DONE: &str = "upload-done";
    /// Payload: `{ id, code, message }`. `unauthorized` means the user must log in again;
    /// the upload resumes from where it stopped when `upload_clip` is called again.
    pub const UPLOAD_FAILED: &str = "upload-failed";
}

/// Server titles are 1–100 characters.
const MAX_TITLE_CHARS: usize = 100;

/// The longest the server address check may take.
const CHECK_TIMEOUT: Duration = Duration::from_secs(10);

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

// ---------------------------------------------------------------------------------------------
// Pure helpers (unit-tested below)

/// Turns what the user typed into a server base URL: trimmed, `https://` added when there is no
/// scheme, no trailing slash. Only http(s) URLs with a host are accepted.
pub fn normalize_server_url(input: &str) -> Result<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(AppError::InvalidServerAddress);
    }
    let with_scheme = if trimmed.contains("://") { trimmed.to_string() } else { format!("https://{trimmed}") };
    let url = reqwest::Url::parse(&with_scheme).map_err(|_| AppError::InvalidServerAddress)?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none_or(str::is_empty) {
        return Err(AppError::InvalidServerAddress);
    }
    if url.query().is_some() || url.fragment().is_some() || !url.username().is_empty() {
        return Err(AppError::InvalidServerAddress);
    }
    Ok(url.as_str().trim_end_matches('/').to_string())
}

/// True when both URLs have the same scheme, host and port.
pub fn same_origin(a: &str, b: &str) -> bool {
    match (reqwest::Url::parse(a), reqwest::Url::parse(b)) {
        (Ok(a), Ok(b)) => a.origin() == b.origin() && a.origin().is_tuple(),
        _ => false,
    }
}

/// The stored tus upload to resume, if it belongs to `server`. A stored URL on another host is
/// dropped: the login token must never be sent anywhere but the current server.
pub fn resume_point(stored_url: Option<&str>, stored_offset: Option<u64>, server: &str) -> Option<(String, u64)> {
    let url = stored_url?;
    same_origin(url, server).then(|| (url.to_string(), stored_offset.unwrap_or(0)))
}

/// Errors from the login request, as the login screen shows them.
pub fn login_error(e: ApiError) -> AppError {
    match e {
        // The server answers 401 for a wrong password and 400 for a malformed request (an empty
        // field): both mean "check what you typed", not "your session expired".
        ApiError::Unauthorized | ApiError::Server { status: 400, .. } => AppError::WrongCredentials,
        e => e.into(),
    }
}

/// The trimmed title, or `InvalidTitle`.
pub fn clean_title(title: &str) -> Result<String> {
    let title = title.trim();
    if title.is_empty() || title.chars().count() > MAX_TITLE_CHARS {
        return Err(AppError::InvalidTitle);
    }
    Ok(title.to_string())
}

/// The Steam app id to store when the user tags `clip` with `name`: another clip's id for the
/// same game, else the id detected for this clip if it had no name yet (Linux detection can know
/// the app id but not the name), else none.
pub fn steam_app_id_for(games: &[GameCount], clip: &Clip, name: Option<&str>) -> Option<u32> {
    let name = name?;
    games
        .iter()
        .find(|g| g.name == name)
        .and_then(|g| g.steam_app_id)
        .or_else(|| clip.game_name.is_none().then_some(clip.steam_app_id).flatten())
}

/// Whether tagging should also teach Klipt the clip's exe, so future clips of it are named.
/// Only for clips that were Unknown, have an exe and no Steam app id: on Linux the exe of a Steam
/// game is often a shared wrapper (Proton's wine64-preloader), and a mapping on it would name
/// every Proton game the same.
pub fn should_save_mapping(clip: &Clip, name: Option<&str>) -> bool {
    name.is_some() && clip.game_name.is_none() && clip.exe.is_some() && clip.steam_app_id.is_none()
}

/// The machine's name, sent as the login's device name so sessions are recognizable.
pub fn device_name() -> String {
    let name = gethostname::gethostname().to_string_lossy().trim().to_string();
    if name.is_empty() || name == "localhost" {
        std::env::var("COMPUTERNAME").ok().filter(|n| !n.trim().is_empty()).unwrap_or_else(|| "Klipt desktop".into())
    } else {
        name
    }
}

// ---------------------------------------------------------------------------------------------
// One upload per clip at a time

/// Clips with an upload running. Exports of one clip share a temp file, so a clip must never be
/// exported twice at once.
#[derive(Default, Clone)]
pub struct Uploads(Arc<Mutex<HashSet<String>>>);

/// Removes the clip from [`Uploads`] when dropped, however the upload ends.
pub struct UploadGuard {
    uploads: Uploads,
    id: String,
}

impl Uploads {
    pub fn try_begin(&self, id: &str) -> Result<UploadGuard> {
        if !lock(&self.0).insert(id.to_string()) {
            return Err(AppError::AlreadyUploading);
        }
        Ok(UploadGuard { uploads: self.clone(), id: id.to_string() })
    }

    pub fn is_active(&self, id: &str) -> bool {
        lock(&self.0).contains(id)
    }
}

impl Drop for UploadGuard {
    fn drop(&mut self) {
        lock(&self.uploads.0).remove(&self.id);
    }
}

// ---------------------------------------------------------------------------------------------
// Settings and account

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Settings {
    lock(&state.settings).clone()
}

/// Validates and saves. The recorder picks up recording changes in Task 12's flow.
#[tauri::command]
pub fn set_settings(app: AppHandle, state: State<'_, AppState>, settings: Settings) -> Result<Settings> {
    let errors = settings.validate();
    if !errors.is_empty() {
        return Err(AppError::InvalidSettings(errors));
    }
    settings.save(&state.data_dir)?;
    allow_clip_folder(&app, &settings.output_dir);
    *lock(&state.settings) = settings.clone();
    Ok(settings)
}

/// Lets the UI load files from the clips folder through the asset protocol.
pub fn allow_clip_folder(app: &AppHandle, dir: &Path) {
    if let Err(e) = app.asset_protocol_scope().allow_directory(dir, true) {
        eprintln!("klipt: can't allow {} in the asset scope: {e}", dir.display());
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub server_url: String,
    pub username: String,
}

/// The saved login, if any. Doesn't contact the server: a revoked token shows up as
/// `unauthorized` on the next upload.
#[tauri::command]
pub fn get_account(state: State<'_, AppState>) -> Option<Account> {
    auth::load(&state.data_dir).map(|a| Account { server_url: a.server_url, username: a.username })
}

/// Checks that `server_url` is a Klipt server and returns its normalized address.
#[tauri::command]
pub async fn check_server(server_url: String) -> Result<String> {
    let base = normalize_server_url(&server_url)?;
    check_server_at(&base).await?;
    Ok(base)
}

pub async fn check_server_at(base: &str) -> Result<()> {
    let client = ApiClient::with_timeout(base, CHECK_TIMEOUT).map_err(|_| AppError::InvalidServerAddress)?;
    let not_found = |detail: String| AppError::ServerNotFound(detail);
    let resp = client.http.get(client.url("/api/health")).send().await.map_err(|e| not_found(e.without_url().to_string()))?;
    if !resp.status().is_success() {
        return Err(not_found(format!("health answered {}", resp.status())));
    }
    let body: serde_json::Value = resp.json().await.map_err(|_| not_found("health isn't JSON".into()))?;
    if body.get("ok") != Some(&serde_json::Value::Bool(true)) {
        return Err(not_found("health didn't say ok".into()));
    }
    Ok(())
}

#[tauri::command]
pub async fn login(app: AppHandle, server_url: String, username: String, password: String) -> Result<Account> {
    let base = normalize_server_url(&server_url)?;
    let client = ApiClient::new(&base).map_err(|_| AppError::InvalidServerAddress)?;
    let session = client.login(username.trim(), &password, &device_name()).await.map_err(login_error)?;
    let Me { username, .. } = session.me;
    let state = app.state::<AppState>();
    auth::save(&state.data_dir, &Auth { server_url: base.clone(), token: session.token, username: username.clone() })?;
    let mut settings = lock(&state.settings).clone();
    if settings.server_url.as_deref() != Some(base.as_str()) {
        settings.server_url = Some(base.clone());
        settings.save(&state.data_dir)?;
        *lock(&state.settings) = settings;
    }
    Ok(Account { server_url: base, username })
}

/// Forgets the login. Telling the server is best effort: logging out works offline too.
#[tauri::command]
pub async fn logout(app: AppHandle) -> Result<()> {
    let data_dir = app.state::<AppState>().data_dir.clone();
    if let Some(saved) = auth::load(&data_dir) {
        if let Ok(client) = ApiClient::with_timeout(&saved.server_url, CHECK_TIMEOUT) {
            if let Err(e) = client.logout(&saved.token).await {
                eprintln!("klipt: server logout failed (logging out locally anyway): {e}");
            }
        }
    }
    auth::clear(&data_dir)
}

#[tauri::command]
pub fn get_hotkey_status(status: State<'_, HotkeyStatusState>) -> HotkeyStatus {
    status.get()
}

#[tauri::command]
pub fn save_clip_now(recorder: State<'_, SupervisorHandle>) {
    recorder.save();
}

// ---------------------------------------------------------------------------------------------
// Library

#[tauri::command]
pub fn list_clips(state: State<'_, AppState>, filter: Option<ListFilter>) -> Result<Vec<ClipView>> {
    lock(&state.library).list(&filter.unwrap_or_default())
}

#[tauri::command]
pub fn list_games(state: State<'_, AppState>) -> Result<Vec<GameCount>> {
    lock(&state.library).games()
}

#[tauri::command]
pub fn rename_clip(state: State<'_, AppState>, id: String, title: String) -> Result<()> {
    let title = clean_title(&title)?;
    lock(&state.library).rename(&id, &title)
}

/// Tags a clip with a game (`None` or blank = Unknown). Tagging an Unknown clip also remembers
/// its exe, so the next clip of that game is named automatically.
#[tauri::command]
pub fn set_clip_game(state: State<'_, AppState>, id: String, name: Option<String>) -> Result<()> {
    let name = name.map(|n| n.trim().to_string()).filter(|n| !n.is_empty());
    let library = lock(&state.library);
    let clip = library.get(&id)?;
    let steam_app_id = steam_app_id_for(&library.games()?, &clip, name.as_deref());
    library.set_game(&id, name.as_deref(), steam_app_id)?;
    drop(library);
    if let (true, Some(exe), Some(name)) = (should_save_mapping(&clip, name.as_deref()), &clip.exe, &name) {
        if let Err(e) = mappings::set(&state.data_dir, exe, name) {
            eprintln!("klipt: can't remember {exe} as {name}: {e}");
        }
    }
    Ok(())
}

#[tauri::command]
pub fn delete_clip(
    state: State<'_, AppState>,
    uploads: State<'_, Uploads>,
    id: String,
    delete_file: bool,
) -> Result<()> {
    if uploads.is_active(&id) {
        return Err(AppError::AlreadyUploading);
    }
    lock(&state.library).remove(&id, delete_file)
}

#[tauri::command]
pub fn show_in_folder(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<()> {
    let clip = lock(&state.library).get(&id)?;
    if !clip.path.is_file() {
        return Err(AppError::FileMissing);
    }
    app.opener().reveal_item_in_dir(&clip.path).map_err(|e| AppError::Io(std::io::Error::other(e.to_string())))
}

/// Copies the clip's share link to the clipboard and returns it.
#[tauri::command]
pub fn copy_share_link(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<String> {
    let clip = lock(&state.library).get(&id)?;
    let url = clip.share_url.ok_or_else(|| AppError::InvalidInput("the clip isn't uploaded".into()))?;
    copy_text(&app, &url)?;
    Ok(url)
}

fn copy_text(app: &AppHandle, text: &str) -> Result<()> {
    app.clipboard().write_text(text).map_err(|e| AppError::Io(std::io::Error::other(e.to_string())))
}

// ---------------------------------------------------------------------------------------------
// Upload

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProgressEvent<'a> {
    id: &'a str,
    sent: u64,
    total: u64,
    phase: &'static str,
}

struct Job {
    clip: Clip,
    title: String,
    visibility: Visibility,
    auth: Auth,
    tools: Tools,
    data_dir: PathBuf,
    cache_dir: PathBuf,
}

/// Starts uploading a clip: export (trim + mix), resumable upload, finalize. Returns once the
/// upload has started; the result arrives as `upload-done` or `upload-failed`.
#[tauri::command]
pub fn upload_clip(
    app: AppHandle,
    state: State<'_, AppState>,
    uploads: State<'_, Uploads>,
    id: String,
    title: String,
    visibility: Visibility,
) -> Result<()> {
    let title = clean_title(&title)?;
    let clip = lock(&state.library).get(&id)?;
    if !clip.path.is_file() {
        return Err(AppError::FileMissing);
    }
    let tools = state.tools.clone().ok_or(AppError::ToolsMissing)?;
    let auth = auth::load(&state.data_dir).ok_or(AppError::Unauthorized)?;
    let cache_dir = app.path().app_cache_dir().map_err(|e| AppError::Io(std::io::Error::other(e.to_string())))?;
    let guard = uploads.try_begin(&id)?;
    let job = Job { clip, title, visibility, auth, tools, data_dir: state.data_dir.clone(), cache_dir };
    tauri::async_runtime::spawn(async move {
        let id = job.clip.id.clone();
        let result = run_upload(&app, &job).await;
        // Free the clip before telling the UI, so a "Try again" right away isn't refused.
        drop(guard);
        match result {
            Ok(share_url) => {
                let copied = match copy_text(&app, &share_url) {
                    Ok(()) => true,
                    Err(e) => {
                        eprintln!("klipt: can't copy the share link: {e}");
                        false
                    }
                };
                emit(&app, events::UPLOAD_DONE, serde_json::json!({ "id": id, "shareUrl": share_url, "copied": copied }));
            }
            Err(e) => {
                eprintln!("klipt: upload of clip {id} failed: {e}");
                if matches!(e, AppError::Unauthorized) {
                    // The token is dead. The clip, its edit and the upload progress stay.
                    if let Err(e) = auth::clear(&job.data_dir) {
                        eprintln!("klipt: can't forget the expired login: {e}");
                    }
                }
                emit(
                    &app,
                    events::UPLOAD_FAILED,
                    serde_json::json!({ "id": id, "code": e.code(), "message": e.user_message() }),
                );
            }
        }
    });
    Ok(())
}

fn emit(app: &AppHandle, event: &str, payload: impl Serialize + Clone) {
    if let Err(e) = app.emit(event, payload) {
        eprintln!("klipt: can't send {event} to the window: {e}");
    }
}

fn progress(app: &AppHandle, id: &str, phase: &'static str, sent: u64, total: u64) {
    emit(app, events::UPLOAD_PROGRESS, ProgressEvent { id, sent, total, phase });
}

async fn run_upload(app: &AppHandle, job: &Job) -> Result<String> {
    let clip = &job.clip;
    let id = clip.id.as_str();
    let server = job.auth.server_url.as_str();
    let client = ApiClient::new(server)?;
    let exported = job.cache_dir.join("exports").join(format!("{id}.mp4"));

    // Resume only an upload of this very export on this very server; anything else starts over.
    let resume = resume_point(clip.upload_id.as_deref(), clip.upload_offset, server).filter(|_| exported.is_file());
    if resume.is_none() {
        if clip.upload_id.is_some() {
            set_upload_progress(app, id, None);
        }
        progress(app, id, "exporting", 0, 1000);
        let (tools, input, cache_dir) = (job.tools.clone(), clip.path.clone(), job.cache_dir.clone());
        let (clip_id, has_mic, export_app) = (clip.id.clone(), clip.has_mic, app.clone());
        let edit = Edit {
            trim_start_ms: clip.trim_start_ms,
            trim_end_ms: clip.trim_end_ms,
            game_volume: clip.game_volume,
            mic_volume: clip.mic_volume,
        };
        tauri::async_runtime::spawn_blocking(move || {
            export::export(&tools, &input, &cache_dir, &clip_id, &edit, has_mic, |p| {
                progress(&export_app, &clip_id, "exporting", (p * 1000.0) as u64, 1000);
            })
        })
        .await
        .map_err(|e| AppError::Io(std::io::Error::other(e.to_string())))??;
    }

    let token = job.auth.token.as_str();
    let outcome = tus::upload(
        &client,
        token,
        &exported,
        resume,
        |sent, total| progress(app, id, "uploading", sent, total),
        |url, offset| set_upload_progress(app, id, Some((url, offset))),
    )
    .await?;

    progress(app, id, "finalizing", 0, 0);
    let request = FinalizeRequest {
        upload_id: outcome.upload_id,
        title: job.title.clone(),
        game_name: clip.game_name.clone(),
        steam_app_id: clip.steam_app_id,
        visibility: job.visibility,
    };
    let server_clip = client.finalize(token, &request).await?;
    let visibility = match server_clip.visibility {
        Visibility::Feed => "feed",
        Visibility::Link => "link",
    };
    {
        let state = app.state::<AppState>();
        let library = lock(&state.library);
        library.mark_uploaded(id, &server_clip.id, &server_clip.share_url, visibility)?;
        if let Err(e) = library.rename(id, &job.title) {
            eprintln!("klipt: can't save the uploaded title of clip {id}: {e}");
        }
    }
    if let Err(e) = std::fs::remove_file(&exported) {
        eprintln!("klipt: can't delete the uploaded export {}: {e}", exported.display());
    }
    Ok(server_clip.share_url)
}

fn set_upload_progress(app: &AppHandle, id: &str, progress: Option<(&str, u64)>) {
    let state = app.state::<AppState>();
    let result = lock(&state.library).set_upload_progress(id, progress);
    if let Err(e) = result {
        eprintln!("klipt: can't remember the upload progress of clip {id}: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn server_urls_are_normalized() {
        assert_eq!(normalize_server_url("  clips.example.com/ ").unwrap(), "https://clips.example.com");
        assert_eq!(normalize_server_url("http://localhost:3000").unwrap(), "http://localhost:3000");
        assert_eq!(normalize_server_url("https://example.com/klipt//").unwrap(), "https://example.com/klipt");
        assert_eq!(normalize_server_url("HTTPS://Clips.Example.com").unwrap(), "https://clips.example.com");
        assert_eq!(normalize_server_url("192.168.1.5:8080").unwrap(), "https://192.168.1.5:8080");
    }

    #[test]
    fn bad_server_urls_are_rejected() {
        for bad in ["", "   ", "ftp://example.com", "https://", "https://x.com/?a=1", "https://u:p@x.com", "http:// spaced"] {
            assert!(matches!(normalize_server_url(bad), Err(AppError::InvalidServerAddress)), "{bad:?}");
        }
    }

    #[test]
    fn origins_compare_scheme_host_and_port() {
        assert!(same_origin("https://a.com/api/uploads/x", "https://a.com"));
        assert!(same_origin("https://a.com:443/api/uploads/x", "https://a.com"));
        assert!(!same_origin("http://a.com/api/uploads/x", "https://a.com"));
        assert!(!same_origin("https://evil.com/api/uploads/x", "https://a.com"));
        assert!(!same_origin("https://a.com:8443/x", "https://a.com"));
        assert!(!same_origin("not a url", "https://a.com"));
    }

    #[test]
    fn resume_only_on_the_same_server() {
        let url = "https://a.com/api/uploads/abc";
        assert_eq!(resume_point(Some(url), Some(16), "https://a.com"), Some((url.to_string(), 16)));
        assert_eq!(resume_point(Some(url), None, "https://a.com"), Some((url.to_string(), 0)));
        assert_eq!(resume_point(Some(url), Some(16), "https://b.com"), None);
        assert_eq!(resume_point(None, Some(16), "https://a.com"), None);
    }

    #[test]
    fn login_errors_say_wrong_password_not_session_expired() {
        let e = login_error(ApiError::Unauthorized);
        assert!(matches!(e, AppError::WrongCredentials));
        assert_eq!(e.user_message(), "Wrong username or password.");
        assert!(matches!(
            login_error(ApiError::Server { status: 400, code: "invalid_request".into() }),
            AppError::WrongCredentials
        ));
        assert!(matches!(login_error(ApiError::Network("down".into())), AppError::Network(_)));
        assert!(matches!(login_error(ApiError::Server { status: 500, code: "x".into() }), AppError::ServerError(_)));
    }

    #[test]
    fn titles_are_trimmed_and_limited() {
        assert_eq!(clean_title("  Nice shot ").unwrap(), "Nice shot");
        assert!(matches!(clean_title("   "), Err(AppError::InvalidTitle)));
        assert!(clean_title(&"é".repeat(100)).is_ok());
        assert!(matches!(clean_title(&"é".repeat(101)), Err(AppError::InvalidTitle)));
    }

    #[test]
    fn a_clip_uploads_once_at_a_time() {
        let uploads = Uploads::default();
        let guard = uploads.try_begin("a").unwrap();
        assert!(uploads.is_active("a"));
        let again = uploads.try_begin("a");
        assert!(matches!(again, Err(AppError::AlreadyUploading)));
        assert_eq!(again.err().unwrap().user_message(), "This clip is already uploading.");
        let other = uploads.try_begin("b").unwrap();
        drop(guard);
        assert!(!uploads.is_active("a"));
        assert!(uploads.try_begin("a").is_ok(), "free again after the first upload ends");
        drop(other);
    }

    fn clip(game: Option<&str>, steam: Option<u32>, exe: Option<&str>) -> Clip {
        Clip {
            id: "c".into(),
            path: PathBuf::from("/x.mp4"),
            created_at: 0,
            duration_ms: 1000,
            width: 1920,
            height: 1080,
            has_mic: false,
            game_name: game.map(String::from),
            steam_app_id: steam,
            exe: exe.map(String::from),
            title: "t".into(),
            trim_start_ms: None,
            trim_end_ms: None,
            game_volume: 1.0,
            mic_volume: 1.0,
            server_clip_id: None,
            share_url: None,
            visibility: None,
            upload_id: None,
            upload_offset: None,
        }
    }

    #[test]
    fn tagging_picks_a_known_steam_id_or_keeps_the_detected_one() {
        let games = [GameCount { name: "Active Matter".into(), steam_app_id: Some(2887580), count: 3 }];
        assert_eq!(steam_app_id_for(&games, &clip(None, None, None), Some("Active Matter")), Some(2887580));
        assert_eq!(steam_app_id_for(&games, &clip(None, Some(42), None), Some("Other")), Some(42));
        assert_eq!(steam_app_id_for(&games, &clip(Some("Old"), Some(42), None), Some("Other")), None);
        assert_eq!(steam_app_id_for(&games, &clip(None, Some(42), None), None), None);
    }

    #[test]
    fn mappings_are_saved_only_for_unknown_non_steam_clips_with_an_exe() {
        assert!(should_save_mapping(&clip(None, None, Some("game.exe")), Some("Game")));
        assert!(!should_save_mapping(&clip(None, None, Some("game.exe")), None));
        assert!(!should_save_mapping(&clip(Some("Known"), None, Some("game.exe")), Some("Game")));
        assert!(!should_save_mapping(&clip(None, None, None), Some("Game")));
        assert!(!should_save_mapping(&clip(None, Some(1), Some("wine64-preloader")), Some("Game")));
    }

    #[test]
    fn device_name_is_never_empty() {
        assert!(!device_name().trim().is_empty());
    }

    #[tokio::test]
    async fn check_server_accepts_a_klipt_health_endpoint() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/health"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "ok": true })))
            .mount(&server)
            .await;
        check_server_at(&server.uri()).await.unwrap();
    }

    #[tokio::test]
    async fn check_server_rejects_other_sites() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/health"))
            .respond_with(ResponseTemplate::new(200).set_body_string("<html>hi</html>"))
            .mount(&server)
            .await;
        assert!(matches!(check_server_at(&server.uri()).await, Err(AppError::ServerNotFound(_))));
        let empty = MockServer::start().await;
        assert!(matches!(check_server_at(&empty.uri()).await, Err(AppError::ServerNotFound(_))));
    }
}
