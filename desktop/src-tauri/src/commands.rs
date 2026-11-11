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

use crate::editor;
use crate::errors::{AppError, Result};
use crate::export::{self, Edit};
use crate::ffmpeg::Tools;
use crate::games::mappings;
use crate::hotkey::{self, HotkeyHandle, HotkeyStatus, HotkeyStatusState};
use crate::library::{Clip, ClipView, GameCount, ListFilter};
use crate::recorder::{AudioDevice, Display, RecordConfig, SupervisorHandle};
use crate::settings::{Effects, FieldError, Settings};
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
    /// Payload: `{ id, progress }`, progress 0..=1 of an "Export copy…".
    pub const EXPORT_PROGRESS: &str = "export-progress";
    /// Payload: `{ id }`: the clip's thumbnail was made again (the editor picked a frame).
    pub const THUMBNAIL_CHANGED: &str = "thumbnail-changed";
}

/// Server titles are 1–100 characters.
const MAX_TITLE_CHARS: usize = 100;

/// The longest the server address check may take.
const CHECK_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
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

/// Clips with an upload or an "Export copy…" running. Exports of one clip share a temp file, so
/// a clip must never be exported twice at once, and its edit can't change mid-upload.
#[derive(Default, Clone)]
pub struct Uploads(Arc<Mutex<HashSet<String>>>);

/// Removes the clip from [`Uploads`] when dropped, however the upload (or export) ends.
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

/// What `set_settings` returns.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsSaved {
    pub settings: Settings,
    /// The recorder restarted with the new settings, which cleared its buffer.
    pub recorder_restarted: bool,
    /// The new save-clip shortcut as the system reports it, when it was rebound.
    pub hotkey_display: Option<String>,
    /// The screenshot shortcut as the system reports it, when it was rebound.
    pub screenshot_hotkey_display: Option<String>,
    /// Why the screenshot shortcut couldn't be bound (the rest was saved).
    pub screenshot_hotkey_error: Option<String>,
}

/// One settings save at a time: each diffs against the settings the previous one saved.
static SAVING_SETTINGS: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// How long listing displays or microphones may take (the recorder may be busy starting).
const LIST_TIMEOUT: Duration = Duration::from_secs(15);

/// The settings to save: validated (as `invalid_settings` with per-field messages) and with the
/// hotkey in canonical form.
pub fn checked_settings(mut settings: Settings) -> Result<Settings> {
    let mut errors = settings.validate();
    if errors.iter().all(|e| e.field != "hotkey") {
        match hotkey::normalize(&settings.hotkey) {
            Ok(accel) => settings.hotkey = accel,
            Err(_) => errors.push(FieldError {
                field: "hotkey",
                message: "Klipt can't use that shortcut. Pick a different one.".into(),
            }),
        }
    }
    if errors.is_empty() {
        Ok(settings)
    } else {
        Err(AppError::InvalidSettings(errors))
    }
}

/// Validates, applies and saves. Applying: a new shortcut is bound first (a taken one fails
/// the whole save with `hotkey_taken` and the old shortcut stays), then the file is written,
/// then the recorder restarts if a recording setting changed and autostart follows
/// `startWithOs`. Save sound and notification are read live. The server address isn't
/// changed here (logging in sets it).
#[tauri::command]
pub async fn set_settings(app: AppHandle, settings: Settings) -> Result<SettingsSaved> {
    let mut new = checked_settings(settings)?;
    let _saving = SAVING_SETTINGS.lock().await;
    let state = app.state::<AppState>();
    let old = lock(&state.settings).clone();
    new.server_url = old.server_url.clone();
    let effects = Effects::between(&old, &new);
    if new.output_dir != old.output_dir {
        // Otherwise the recorder would fail later with only a generic "recording" error.
        if let Err(e) = check_writable_dir(&new.output_dir) {
            eprintln!("klipt: can't use {} for clips: {e}", new.output_dir.display());
            return Err(AppError::InvalidSettings(vec![FieldError {
                field: "outputDir",
                message: "Klipt can't save clips in that folder. Pick another one.".into(),
            }]));
        }
    }

    let (mut hotkey_display, mut screenshot_hotkey_display, mut screenshot_hotkey_error) = (None, None, None);
    if effects.rebind_hotkey {
        let key = rebind_hotkey(&app, hotkey::Keys::from_settings(&new)).await?;
        hotkey_display = Some(key.display);
        screenshot_hotkey_display = key.screenshot;
        screenshot_hotkey_error = key.screenshot_error;
    }
    let saved = {
        let mut current = lock(&state.settings);
        // A login may have changed the server address meanwhile.
        new.server_url = current.server_url.clone();
        new.save(&state.data_dir).map(|()| *current = new.clone())
    };
    if let Err(e) = saved {
        if effects.rebind_hotkey {
            // The file still has the old shortcut: bind it again, so what works matches it.
            if let Err(e) = rebind_hotkey(&app, hotkey::Keys::from_settings(&old)).await {
                eprintln!("klipt: can't restore the old shortcut after a failed save: {e}");
            }
        }
        return Err(e);
    }

    allow_clip_folder(&app, &new.output_dir);
    if effects.restart_recorder {
        app.state::<SupervisorHandle>().configure(RecordConfig::from_settings(&new), new.recording_mode);
    }
    if effects.autostart {
        crate::apply_autostart(&app, new.start_with_os);
    }
    if new.cleanup_enabled {
        // Turned on or shortened: delete what's now too old without waiting for the next run.
        let handle = app.clone();
        tauri::async_runtime::spawn_blocking(move || crate::cleanup::run_now(&handle));
    }
    // While paused nothing restarts: the new settings apply on resume.
    let paused = app.try_state::<crate::tray::Tray>().is_some_and(|t| t.is_paused());
    Ok(SettingsSaved {
        settings: new,
        recorder_restarted: effects.restart_recorder && !paused,
        hotkey_display,
        screenshot_hotkey_display,
        screenshot_hotkey_error,
    })
}

/// Creates `dir` if needed and checks a file can be written in it.
pub fn check_writable_dir(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let probe = dir.join(format!(".klipt-write-test-{}", uuid::Uuid::new_v4().simple()));
    std::fs::write(&probe, b"")?;
    std::fs::remove_file(&probe)
}

async fn rebind_hotkey(app: &AppHandle, keys: hotkey::Keys) -> Result<hotkey::BoundKey> {
    let handle = app.state::<HotkeyHandle>().inner().clone();
    tauri::async_runtime::spawn_blocking(move || handle.rebind(&keys))
        .await
        .map_err(|e| AppError::Internal(format!("hotkey rebind task failed: {e}")))?
}

/// The displays the recorder can capture.
#[tauri::command]
pub async fn list_displays(app: AppHandle) -> Result<Vec<Display>> {
    tauri::async_runtime::spawn_blocking(move || app.state::<SupervisorHandle>().list_displays(LIST_TIMEOUT))
        .await
        .map_err(|e| AppError::Internal(format!("display listing failed: {e}")))?
}

/// The microphones the recorder can record from (the default one isn't listed).
#[tauri::command]
pub async fn list_audio_devices(app: AppHandle) -> Result<Vec<AudioDevice>> {
    tauri::async_runtime::spawn_blocking(move || app.state::<SupervisorHandle>().list_audio_devices(LIST_TIMEOUT))
        .await
        .map_err(|e| AppError::Internal(format!("microphone listing failed: {e}")))?
}

/// Takes a screenshot now (the "Screenshot" button); it arrives as `clip-saved`.
#[tauri::command]
pub fn take_screenshot(app: AppHandle) {
    crate::screenshot::take_in_background(&app);
}

/// Opens the clips folder in the file manager (creating it if needed).
#[tauri::command]
pub fn open_clips_folder(app: AppHandle, state: State<'_, AppState>) -> Result<()> {
    let dir = lock(&state.settings).output_dir.clone();
    std::fs::create_dir_all(&dir)?;
    app.opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(|e| AppError::Io(std::io::Error::other(e.to_string())))
}

/// How many bytes the clips folder (with its subfolders) takes on disk.
#[tauri::command]
pub async fn clips_folder_size(app: AppHandle) -> Result<u64> {
    let dir = lock(&app.state::<AppState>().settings).output_dir.clone();
    tauri::async_runtime::spawn_blocking(move || crate::cleanup::folder_size(&dir))
        .await
        .map_err(|e| AppError::Internal(format!("folder size failed: {e}")))
}

/// Lets the UI load files from the clips folder through the asset protocol. Not recursive: clips
/// are written flat, and a folder like `~` must not expose everything under it.
pub fn allow_clip_folder(app: &AppHandle, dir: &Path) {
    if let Err(e) = app.asset_protocol_scope().allow_directory(dir, false) {
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
    // The same lock as Settings saves, and the change made to the current settings, so a save
    // finishing meanwhile isn't overwritten.
    let _saving = SAVING_SETTINGS.lock().await;
    {
        let mut current = lock(&state.settings);
        if current.server_url.as_deref() != Some(base.as_str()) {
            let mut next = current.clone();
            next.server_url = Some(base.clone());
            next.save(&state.data_dir)?;
            *current = next;
        }
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

/// Stars or unstars a clip.
#[tauri::command]
pub fn set_clip_favorite(state: State<'_, AppState>, id: String, favorite: bool) -> Result<()> {
    lock(&state.library).set_favorite(&id, favorite)
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
    app: AppHandle,
    state: State<'_, AppState>,
    uploads: State<'_, Uploads>,
    id: String,
    delete_file: bool,
) -> Result<()> {
    if uploads.is_active(&id) {
        return Err(AppError::AlreadyUploading);
    }
    lock(&state.library).remove(&id, delete_file)?;
    if let Ok(cache) = cache_dir(&app) {
        editor::remove_edit_cache(&cache, &id);
    }
    Ok(())
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

/// Copies any text (a feed clip's share link) to the clipboard.
#[tauri::command]
pub fn copy_text_to_clipboard(app: AppHandle, text: String) -> Result<()> {
    copy_text(&app, &text)
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
    /// Friends in the clip, by username.
    tagged: Vec<String>,
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
    tagged: Option<Vec<String>>,
) -> Result<()> {
    let title = clean_title(&title)?;
    let tagged: Vec<String> =
        tagged.unwrap_or_default().into_iter().map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).take(20).collect();
    let clip = lock(&state.library).get(&id)?;
    if !clip.path.is_file() {
        return Err(AppError::FileMissing);
    }
    let tools = state.tools.clone().ok_or(AppError::ToolsMissing)?;
    let auth = auth::load(&state.data_dir).ok_or(AppError::Unauthorized)?;
    let cache_dir = cache_dir(&app)?;
    let guard = uploads.try_begin(&id)?;
    let job = Job { clip, title, visibility, tagged, auth, tools, data_dir: state.data_dir.clone(), cache_dir };
    tauri::async_runtime::spawn(async move {
        let id = job.clip.id.clone();
        let (data_dir, token) = (job.data_dir.clone(), job.auth.token.clone());
        // The work runs in its own task so a panic anywhere in it still ends in `upload-failed`
        // instead of a card stuck on "Uploading…".
        let worker_app = app.clone();
        let result = match tauri::async_runtime::spawn(async move { run_upload(&worker_app, &job).await }).await {
            Ok(result) => result,
            Err(e) => Err(AppError::Internal(format!("upload task failed: {e}"))),
        };
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
                // `stale`: the login this upload used was already replaced (another upload hit
                // 401 first and the user logged in again, or they logged out): the UI just
                // retries with the current login instead of asking for it again.
                let mut stale = false;
                if matches!(e, AppError::Unauthorized) {
                    // The token is dead. The clip, its edit and the upload progress stay.
                    match clear_auth_if_current(&data_dir, &token) {
                        Ok(cleared) => stale = !cleared,
                        Err(e) => eprintln!("klipt: can't forget the expired login: {e}"),
                    }
                }
                emit(
                    &app,
                    events::UPLOAD_FAILED,
                    serde_json::json!({ "id": id, "code": e.code(), "message": e.user_message(), "stale": stale }),
                );
            }
        }
    });
    Ok(())
}

/// Forgets the saved login only if it is still the one with `token`; returns whether it did. An
/// upload that started with an older login must not log the user out of a newer one.
pub fn clear_auth_if_current(data_dir: &Path, token: &str) -> Result<bool> {
    match auth::load(data_dir) {
        Some(saved) if saved.token == token => auth::clear(data_dir).map(|()| true),
        _ => Ok(false),
    }
}

/// `<app cache>`: upload exports and the editor's audio.
fn cache_dir(app: &AppHandle) -> Result<PathBuf> {
    app.path().app_cache_dir().map_err(|e| AppError::Io(std::io::Error::other(e.to_string())))
}

fn emit(app: &AppHandle, event: &str, payload: impl Serialize + Clone) {
    if let Err(e) = app.emit(event, payload) {
        eprintln!("klipt: can't send {event} to the window: {e}");
    }
}

fn progress(app: &AppHandle, id: &str, phase: &'static str, sent: u64, total: u64) {
    emit(app, events::UPLOAD_PROGRESS, ProgressEvent { id, sent, total, phase });
}

/// The chosen thumbnail frame as a time in the uploaded, trimmed video (kept inside the cut).
pub(crate) fn upload_thumbnail_ms(clip: &Clip) -> Option<u64> {
    let at = clip.thumbnail_ms?;
    let start = clip.trim_start_ms.unwrap_or(0);
    let end = clip.trim_end_ms.unwrap_or(clip.duration_ms).max(start);
    Some(at.clamp(start, end.saturating_sub(1).max(start)) - start)
}

async fn run_upload(app: &AppHandle, job: &Job) -> Result<String> {
    let clip = &job.clip;
    let id = clip.id.as_str();
    let server = job.auth.server_url.as_str();
    let client = ApiClient::new(server)?;
    let screenshot = clip.kind == crate::library::ClipKind::Screenshot;
    // A screenshot goes up as the PNG itself; a clip as its export (trim + mix).
    let exported = if screenshot { clip.path.clone() } else { editor::upload_export_path(&job.cache_dir, id) };

    // Resume only an upload of this very export on this very server; anything else starts over.
    let resume = resume_point(clip.upload_id.as_deref(), clip.upload_offset, server).filter(|_| exported.is_file());
    if resume.is_none() && clip.upload_id.is_some() {
        set_upload_progress(app, id, None);
    }
    if resume.is_none() && !screenshot {
        progress(app, id, "exporting", 0, 1000);
        let (tools, input, cache_dir) = (job.tools.clone(), clip.path.clone(), job.cache_dir.clone());
        let (clip_id, tracks, export_app) = (clip.id.clone(), export::Tracks::of(clip), app.clone());
        let edit = Edit {
            trim_start_ms: clip.trim_start_ms,
            trim_end_ms: clip.trim_end_ms,
            game_volume: clip.game_volume,
            mic_volume: clip.mic_volume,
            voice_volume: clip.voice_volume,
            master_volume: clip.master_volume,
        };
        tauri::async_runtime::spawn_blocking(move || {
            export::export(&tools, &input, &cache_dir, &clip_id, &edit, tracks, |p| {
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
        thumbnail_ms: if screenshot { None } else { upload_thumbnail_ms(clip) },
        tagged: job.tagged.clone(),
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
    if !screenshot {
        if let Err(e) = std::fs::remove_file(&exported) {
            eprintln!("klipt: can't delete the uploaded export {}: {e}", exported.display());
        }
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

// ---------------------------------------------------------------------------------------------
// Editor

/// What the editor needs to play a clip: the muted video plus its game and mic audio as separate
/// files, so each has its own volume.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorData {
    pub clip: ClipView,
    pub video_path: PathBuf,
    pub game_audio_path: PathBuf,
    /// None when the clip has no mic track: the editor hides the Mic slider.
    pub mic_audio_path: Option<PathBuf>,
    /// None when the clip has no voice chat track: the editor hides the Voice chat slider.
    pub voice_audio_path: Option<PathBuf>,
    /// Loopback HTTP URLs for the media elements (WebKitGTK can't stream media from `asset://`).
    pub video_url: String,
    pub game_audio_url: String,
    pub mic_audio_url: Option<String>,
    pub voice_audio_url: Option<String>,
    pub duration_ms: u64,
}

/// Opens a clip in the editor: demuxes its audio tracks (reusing earlier copies) and lets the
/// webview load the clip and those files. A clip whose file is missing doesn't open.
#[tauri::command]
pub async fn open_editor(app: AppHandle, id: String) -> Result<EditorData> {
    let (clip, thumb, tools) = {
        let state = app.state::<AppState>();
        let library = lock(&state.library);
        (library.get(&id)?, library.thumb_path(&id), state.tools.clone())
    };
    if !clip.path.is_file() {
        return Err(AppError::FileMissing);
    }
    let tools = tools.ok_or(AppError::ToolsMissing)?;
    let cache_dir = cache_dir(&app)?;
    let (input, tracks, stored_ms) = (clip.path.clone(), export::Tracks::of(&clip), clip.duration_ms);
    let (game, mic, voice, duration_ms) = tauri::async_runtime::spawn_blocking(move || -> Result<_> {
        let (game, mic, voice) = editor::prepare_audio(&tools, &input, tracks, &cache_dir, &id)?;
        let duration_ms = if stored_ms > 0 { stored_ms } else { crate::ffmpeg::probe(&tools, &input)?.duration_ms };
        Ok((game, mic, voice, duration_ms))
    })
    .await
    .map_err(|e| AppError::Internal(format!("editor setup failed: {e}")))??;

    // Only these files, not their folders: the clip may be outside the current clips folder (it
    // was saved before the folder changed), and the cache folder holds other clips' files.
    let scope = app.asset_protocol_scope();
    for file in [&clip.path, &game].into_iter().chain(mic.as_ref()).chain(voice.as_ref()) {
        if let Err(e) = scope.allow_file(file) {
            eprintln!("klipt: can't allow {} in the asset scope: {e}", file.display());
        }
    }
    let media = app.state::<crate::media_server::MediaServer>();
    let video_url = media.url_for(&clip.path);
    let game_audio_url = media.url_for(&game);
    let mic_audio_url = mic.as_deref().map(|m| media.url_for(m));
    let voice_audio_url = voice.as_deref().map(|v| media.url_for(v));
    Ok(EditorData {
        video_url,
        game_audio_url,
        mic_audio_url,
        voice_audio_url,
        video_path: clip.path.clone(),
        clip: ClipView { clip, file_missing: false, thumb_path: thumb.is_file().then_some(thumb) },
        game_audio_path: game,
        mic_audio_path: mic,
        voice_audio_path: voice,
        duration_ms,
    })
}

fn edit_from(
    trim_start_ms: Option<u64>,
    trim_end_ms: Option<u64>,
    game_volume: f32,
    mic_volume: f32,
    voice_volume: Option<f32>,
    master_volume: Option<f32>,
) -> Edit {
    Edit {
        trim_start_ms,
        trim_end_ms,
        game_volume,
        mic_volume,
        voice_volume: voice_volume.unwrap_or(1.0),
        master_volume: master_volume.unwrap_or(1.0),
    }
}

/// Saves the trim and volumes (the clip file is untouched). No trim is `null` start/end. Refused
/// while the clip uploads; a changed edit makes an interrupted upload start over with the new cut.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn save_edit(
    app: AppHandle,
    state: State<'_, AppState>,
    uploads: State<'_, Uploads>,
    id: String,
    trim_start_ms: Option<u64>,
    trim_end_ms: Option<u64>,
    game_volume: f32,
    mic_volume: f32,
    voice_volume: Option<f32>,
    master_volume: Option<f32>,
    thumbnail_ms: Option<u64>,
) -> Result<()> {
    let cache_dir = cache_dir(&app)?;
    let edit = edit_from(trim_start_ms, trim_end_ms, game_volume, mic_volume, voice_volume, master_volume);
    let clip = {
        let library = lock(&state.library);
        editor::save_edit(&library, &uploads, &cache_dir, &id, edit)?;
        let clip = library.get(&id)?;
        if clip.thumbnail_ms == thumbnail_ms {
            return Ok(());
        }
        library.set_thumbnail(&id, thumbnail_ms)?;
        library.get(&id)?
    };
    // The library's thumbnail follows; ffmpeg runs off the UI thread.
    let (thumb, tools) = (lock(&state.library).thumb_path(&id), state.tools.clone());
    if let Some(tools) = tools {
        std::thread::spawn(move || {
            let at = clip.thumbnail_ms.unwrap_or_else(|| crate::recorder::thumbnail_at_ms(clip.duration_ms));
            if let Err(e) = crate::ffmpeg::thumbnail(&tools, &clip.path, &thumb, at) {
                eprintln!("klipt: can't redo the thumbnail of {}: {e}", clip.path.display());
            }
            emit(&app, events::THUMBNAIL_CHANGED, serde_json::json!({ "id": clip.id }));
        });
    }
    Ok(())
}

#[derive(Clone, Serialize)]
struct ExportProgressEvent<'a> {
    id: &'a str,
    progress: f32,
}

/// Exports the clip with the editor's current (possibly unsaved) edit and writes it to
/// `dest_path`. Reports `export-progress`; returns when the file is written. Refused while the
/// clip uploads.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn export_copy(
    app: AppHandle,
    id: String,
    dest_path: PathBuf,
    trim_start_ms: Option<u64>,
    trim_end_ms: Option<u64>,
    game_volume: f32,
    mic_volume: f32,
    voice_volume: Option<f32>,
    master_volume: Option<f32>,
) -> Result<()> {
    let (clip, tools) = {
        let state = app.state::<AppState>();
        let clip = lock(&state.library).get(&id)?;
        (clip, state.tools.clone())
    };
    if !clip.path.is_file() {
        return Err(AppError::FileMissing);
    }
    if editor::same_file(&dest_path, &clip.path) {
        return Err(AppError::OverwriteOriginal);
    }
    let tools = tools.ok_or(AppError::ToolsMissing)?;
    let cache_dir = cache_dir(&app)?;
    let guard = app.state::<Uploads>().try_begin(&id).map_err(|_| AppError::ClipBusy)?;
    let edit = edit_from(trim_start_ms, trim_end_ms, game_volume, mic_volume, voice_volume, master_volume);
    let worker_app = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let staged = export::export(&tools, &clip.path, &cache_dir, &editor::copy_export_id(&id), &edit, export::Tracks::of(&clip), |p| {
            emit(&worker_app, events::EXPORT_PROGRESS, ExportProgressEvent { id: &id, progress: p });
        })?;
        editor::move_file(&staged, &dest_path)
    })
    .await
    .map_err(|e| AppError::Internal(format!("export task failed: {e}")))
    .and_then(|r| r);
    drop(guard);
    if let Err(e) = &result {
        eprintln!("klipt: export copy failed: {e}");
    }
    result
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
            favorite: false,
            thumbnail_ms: None,
            has_voice: false,
            voice_volume: 1.0,
            master_volume: 1.0,
            kind: crate::library::ClipKind::Clip,
        }
    }

    #[test]
    fn upload_thumbnail_is_relative_to_the_cut_and_inside_it() {
        let mut c = clip(None, None, None);
        c.duration_ms = 30_000;
        assert_eq!(upload_thumbnail_ms(&c), None);
        c.thumbnail_ms = Some(5_000);
        assert_eq!(upload_thumbnail_ms(&c), Some(5_000));
        c.trim_start_ms = Some(4_000);
        c.trim_end_ms = Some(10_000);
        assert_eq!(upload_thumbnail_ms(&c), Some(1_000));
        c.thumbnail_ms = Some(2_000);
        assert_eq!(upload_thumbnail_ms(&c), Some(0), "before the cut: its first frame");
        c.thumbnail_ms = Some(20_000);
        assert_eq!(upload_thumbnail_ms(&c), Some(5_999), "after the cut: its last frame");
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
    fn saved_settings_are_validated_and_the_hotkey_normalized() {
        let ok = checked_settings(Settings { hotkey: "shift+ctrl+s".into(), ..Settings::default() }).unwrap();
        assert_eq!(ok.hotkey, "Ctrl+Shift+S");
        let fields = |e: AppError| match e {
            AppError::InvalidSettings(fields) => fields.into_iter().map(|f| f.field).collect::<Vec<_>>(),
            e => panic!("{e:?}"),
        };
        let bad_key = checked_settings(Settings { hotkey: "Alt+Nope".into(), ..Settings::default() }).unwrap_err();
        assert_eq!(fields(bad_key), ["hotkey"]);
        let both = checked_settings(Settings { fps: 45, hotkey: "".into(), ..Settings::default() }).unwrap_err();
        assert_eq!(fields(both), ["fps", "hotkey"], "an empty hotkey is reported once");
    }

    #[test]
    fn device_name_is_never_empty() {
        assert!(!device_name().trim().is_empty());
    }

    #[test]
    fn a_dead_token_clears_only_its_own_login() {
        let dir = tempfile::tempdir().unwrap();
        let login = |token: &str| Auth { server_url: "https://a.com".into(), token: token.into(), username: "adam".into() };
        assert!(!clear_auth_if_current(dir.path(), "old").unwrap(), "nothing saved: nothing to clear");

        // The user already logged in again: an old upload's 401 leaves the new login alone.
        auth::save(dir.path(), &login("new")).unwrap();
        assert!(!clear_auth_if_current(dir.path(), "old").unwrap());
        assert_eq!(auth::load(dir.path()).unwrap().token, "new");

        // The current login's 401 forgets it.
        assert!(clear_auth_if_current(dir.path(), "new").unwrap());
        assert!(auth::load(dir.path()).is_none());
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

// ---------------------------------------------------------------------------------------------
// Feed (everyone's uploaded clips, from the server)

const FEED_TIMEOUT: Duration = Duration::from_secs(20);

async fn server_get(app: &AppHandle, path: &str, query: Vec<(&str, String)>) -> Result<(serde_json::Value, ApiClient)> {
    let data_dir = app.state::<AppState>().data_dir.clone();
    let saved = auth::load(&data_dir).ok_or(AppError::Unauthorized)?;
    let client = ApiClient::with_timeout(&saved.server_url, FEED_TIMEOUT)?;
    match client.get_json(&saved.token, path, &query).await {
        Ok(value) => Ok((value, client)),
        Err(e) => {
            let e = AppError::from(e);
            if matches!(e, AppError::Unauthorized) {
                // The token is dead: forget it, or the next launch would look logged in again.
                if let Err(e) = clear_auth_if_current(&data_dir, &saved.token) {
                    eprintln!("klipt: can't forget the expired login: {e}");
                }
            }
            Err(e)
        }
    }
}

/// Drops feed clips whose links or media aren't on the server itself. The page may load images
/// and video from any http(s) address (CSP), so a server can't point them elsewhere this way.
pub(crate) fn keep_own_media(page: &mut serde_json::Value, client: &ApiClient) {
    if let Some(items) = page.get_mut("items").and_then(|v| v.as_array_mut()) {
        items.retain(|clip| {
            ["shareUrl", "videoUrl", "thumbnailUrl"]
                .iter()
                .all(|key| clip.get(key).and_then(|v| v.as_str()).is_some_and(|url| client.same_origin(url)))
        });
    }
}

/// One page of the shared feed: the server's `ClipList`.
#[tauri::command]
pub async fn feed_clips(
    app: AppHandle,
    game: Option<String>,
    cursor: Option<String>,
    search: Option<String>,
    with_me: Option<bool>,
    kind: Option<String>,
    favorites: Option<bool>,
) -> Result<serde_json::Value> {
    let mut query = vec![("scope", "feed".to_string())];
    if favorites == Some(true) {
        query.push(("favorites", "1".to_string()));
    }
    query.extend(kind.filter(|k| k == "clip" || k == "screenshot").map(|k| ("kind", k)));
    if with_me == Some(true) {
        query.push(("with", "me".to_string()));
    }
    query.extend(game.filter(|g| !g.is_empty()).map(|g| ("game", g)));
    query.extend(search.map(|s| s.trim().chars().take(100).collect::<String>()).filter(|s| !s.is_empty()).map(|s| ("q", s)));
    query.extend(cursor.filter(|c| !c.is_empty()).map(|c| ("cursor", c)));
    let (mut page, client) = server_get(&app, "/api/clips", query).await?;
    keep_own_media(&mut page, &client);
    Ok(page)
}

/// The feed's games with clip counts: the server's `GameCount[]`.
#[tauri::command]
pub async fn feed_games(app: AppHandle) -> Result<serde_json::Value> {
    Ok(server_get(&app, "/api/games", vec![("scope", "feed".to_string())]).await?.0)
}

/// How much server storage your own clips use: the server's `MyStorage` (`{ clips, bytes }`).
#[tauri::command]
pub async fn my_storage(app: AppHandle) -> Result<serde_json::Value> {
    Ok(server_get(&app, "/api/me/storage", Vec::new()).await?.0)
}

/// The server's active users (`[{ username }]`), for tagging friends in a clip.
#[tauri::command]
pub async fn list_users(app: AppHandle) -> Result<serde_json::Value> {
    Ok(server_get(&app, "/api/users", Vec::new()).await?.0)
}

/// Opens the Klipt website in the browser: its home page, or `url` (e.g. a clip's share link),
/// which must be on the server. A full URL rather than a path, so a server at a subpath
/// (`https://example.com/klipt`) doesn't get its path twice.
#[tauri::command]
pub fn open_website(app: AppHandle, url: Option<String>) -> Result<()> {
    let saved = auth::load(&app.state::<AppState>().data_dir).ok_or(AppError::Unauthorized)?;
    let url = website_url(&saved.server_url, url)?;
    app.opener().open_url(url, None::<&str>).map_err(|e| AppError::Io(std::io::Error::other(e.to_string())))
}

fn website_url(server_url: &str, url: Option<String>) -> Result<String> {
    match url.filter(|u| !u.is_empty()) {
        None => Ok(server_url.to_string()),
        Some(url) => {
            let client = ApiClient::new(server_url).map_err(|_| AppError::InvalidServerAddress)?;
            if !client.same_origin(&url) {
                return Err(AppError::InvalidInput("that link isn't on your Klipt server".into()));
            }
            Ok(url)
        }
    }
}

#[cfg(test)]
mod feed_tests {
    use super::*;

    #[test]
    fn website_links_keep_a_subpath_once_and_stay_on_the_server() {
        let base = "https://example.com/klipt";
        assert_eq!(website_url(base, None).unwrap(), base);
        assert_eq!(
            website_url(base, Some("https://example.com/klipt/c/abc".into())).unwrap(),
            "https://example.com/klipt/c/abc"
        );
        assert!(website_url(base, Some("https://evil.example/c/abc".into())).is_err());
        assert!(website_url(base, Some("not a url".into())).is_err());
    }

    #[test]
    fn feed_clips_with_media_elsewhere_are_dropped() {
        let client = ApiClient::new("https://k.example").unwrap();
        let clip = |thumb: &str| {
            serde_json::json!({
                "shareUrl": "https://k.example/c/a",
                "videoUrl": "https://k.example/media/a.mp4",
                "thumbnailUrl": thumb,
            })
        };
        let mut page = serde_json::json!({
            "items": [clip("https://k.example/media/a.jpg"), clip("https://tracker.example/pixel.gif")],
            "nextCursor": null,
        });
        keep_own_media(&mut page, &client);
        let items = page["items"].as_array().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["thumbnailUrl"], "https://k.example/media/a.jpg");
    }
}

#[cfg(test)]
mod settings_tests {
    use super::*;

    #[test]
    fn a_clips_folder_must_be_writable() {
        let dir = tempfile::tempdir().unwrap();
        let fresh = dir.path().join("new").join("Klipt");
        check_writable_dir(&fresh).unwrap();
        assert!(fresh.is_dir(), "created");
        assert_eq!(std::fs::read_dir(&fresh).unwrap().count(), 0, "the probe file is gone");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let locked = dir.path().join("locked");
            std::fs::create_dir(&locked).unwrap();
            std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555)).unwrap();
            let result = check_writable_dir(&locked);
            std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
            assert!(result.is_err(), "read-only folder accepted");
        }
    }
}
