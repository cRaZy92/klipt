//! Your account on the server from the app: display name, password, telling the server which
//! app version runs on what PC, and sending a problem report or an idea (with the log).

use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Manager};
use tauri_plugin_opener::OpenerExt;

use crate::commands::{clear_auth_if_current, lock, off_main, Account};
use crate::errors::{AppError, Result};
use crate::state::AppState;
use crate::upload::api::{ApiClient, ApiError};
use crate::upload::auth::{self, Auth};

const TIMEOUT: Duration = Duration::from_secs(30);
/// How much of the log a report sends (its end).
pub const REPORT_LOG_BYTES: u64 = 1_500_000;

/// Friendly words for the server's refusals on these calls.
fn refusal(code: &str) -> Option<&'static str> {
    Some(match code {
        "wrong_password" => "That isn't your current password.",
        "weak_password" => "The new password needs at least 8 characters.",
        "invalid_display_name" => "Names are up to 32 characters.",
        "not_unlocked" => "Earn the achievement that unlocks it first.",
        "not_an_image" => "Klipt can't read that file as a picture.",
        "unsupported_media_type" => "Use a JPEG, PNG or WebP picture.",
        "rate_limited" => "Too many tries. Wait a minute and try again.",
        _ => return None,
    })
}

fn api_error(e: ApiError) -> AppError {
    match &e {
        ApiError::Server { status: 429, .. } => AppError::Refused(refusal("rate_limited").unwrap().into()),
        ApiError::Server { code, .. } => match refusal(code) {
            Some(message) => AppError::Refused(message.into()),
            None => e.into(),
        },
        _ => e.into(),
    }
}

/// The saved login and a client for its server.
fn saved(app: &AppHandle) -> Result<(Auth, ApiClient)> {
    let auth = auth::load(&app.state::<AppState>().data_dir).ok_or(AppError::Unauthorized)?;
    let client = ApiClient::with_timeout(&auth.server_url, TIMEOUT)?;
    Ok((auth, client))
}

/// A dead token is forgotten, like the feed does, so the app asks to log in again.
fn forget_if_unauthorized(app: &AppHandle, auth: &Auth, e: &AppError) {
    if matches!(e, AppError::Unauthorized) {
        if let Err(e) = clear_auth_if_current(&app.state::<AppState>().data_dir, &auth.token) {
            crate::elog!("klipt: can't forget the expired login: {e}");
        }
    }
}

/// Keeps what the server answered (`me`: display name, admin flag, picture, looks) in the saved login.
fn save_name(app: &AppHandle, auth: &Auth, me: &serde_json::Value) -> Result<Account> {
    let data_dir = app.state::<AppState>().data_dir.clone();
    // Only if this login is still the saved one (a logout or another login may have happened).
    let mut current = auth::load(&data_dir).filter(|a| a.token == auth.token).ok_or(AppError::Unauthorized)?;
    let mut next = current.clone();
    next.display_name = custom_name_of(me);
    next.is_admin = me.get("isAdmin").and_then(|v| v.as_bool()).unwrap_or(current.is_admin);
    // Older servers send neither: keep what's there.
    if let Some(url) = me.get("avatarUrl") {
        next.avatar_url = url.as_str().map(String::from);
    }
    if let Some(looks) = me.get("looks").filter(|l| l.is_object()) {
        next.looks = Some(looks.clone());
    }
    if next != current {
        auth::save(&data_dir, &next)?;
        current = next;
    }
    Ok(Account::from_auth(&current))
}

fn custom_name_of(me: &serde_json::Value) -> Option<String> {
    me.get("customName").and_then(|v| v.as_str()).map(String::from).filter(|s| !s.is_empty())
}

/// Tells the server, in the background, which version of the app runs on what OS and GPU.
/// Best effort: a failure is only logged.
pub fn report_device(app: &AppHandle, auth: &Auth) {
    let version = app.package_info().version.to_string();
    let (server, token) = (auth.server_url.clone(), auth.token.clone());
    tauri::async_runtime::spawn(async move {
        let gpu =
            tauri::async_runtime::spawn_blocking(|| crate::diagnostics::gpus().join("; ")).await.unwrap_or_default();
        let os = crate::diagnostics::os_name();
        let body = serde_json::json!({ "appVersion": version, "os": os, "gpu": if gpu.is_empty() { None } else { Some(gpu) } });
        let sent = match ApiClient::with_timeout(&server, TIMEOUT) {
            Ok(client) => client.post_json(&token, "/api/me/device", &body).await.map(|_| ()),
            Err(e) => Err(e),
        };
        match sent {
            Ok(()) => crate::elog!("klipt: told the server this is Klipt {version} on {os}"),
            // An older server without the endpoint: nothing to do.
            Err(ApiError::Server { status: 404, .. }) => {}
            Err(e) => crate::elog!("klipt: can't report this device to the server: {e}"),
        }
    });
}

/// Gets the account from the server (a name changed on the website shows up in the app) and
/// reports this device. None when logged out.
#[tauri::command]
pub async fn refresh_account(app: AppHandle) -> Result<Option<Account>> {
    let Ok((auth, client)) = saved(&app) else { return Ok(None) };
    let me = match client.get_json(&auth.token, "/api/me", &[]).await {
        Ok(me) => me,
        Err(e) => {
            let e = AppError::from(e);
            forget_if_unauthorized(&app, &auth, &e);
            return Err(e);
        }
    };
    report_device(&app, &auth);
    // The activity report also announces achievements earned since the app last looked.
    let app3 = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = crate::activity::report_now(&app3).await {
            crate::elog!("klipt: can't report activity to the server: {e}");
        }
    });
    let app2 = app.clone();
    off_main(move || save_name(&app2, &auth, &me)).await?.map(Some)
}

/// Sets the name friends see; empty clears it.
#[tauri::command]
pub async fn update_profile(app: AppHandle, display_name: String) -> Result<Account> {
    let (auth, client) = saved(&app)?;
    let body = serde_json::json!({ "displayName": display_name.trim() });
    let me = client.patch_json(&auth.token, "/api/me", &body).await.map_err(api_error).inspect_err(|e| {
        forget_if_unauthorized(&app, &auth, e);
    })?;
    let app2 = app.clone();
    off_main(move || save_name(&app2, &auth, &me)).await?
}

/// Changes the password; the server signs out every other device (this one stays logged in).
#[tauri::command]
pub async fn change_password(app: AppHandle, current_password: String, new_password: String) -> Result<()> {
    let (auth, client) = saved(&app)?;
    let body = serde_json::json!({ "currentPassword": current_password, "newPassword": new_password });
    client.post_json(&auth.token, "/api/me/password", &body).await.map_err(api_error).inspect_err(|e| {
        forget_if_unauthorized(&app, &auth, e);
    })?;
    Ok(())
}

/// Your achievements (`MyAchievements`), for the Profile screen.
#[tauri::command]
pub async fn my_achievements(app: AppHandle) -> Result<serde_json::Value> {
    let (auth, client) = saved(&app)?;
    client.get_json(&auth.token, "/api/me/achievements", &[]).await.map_err(api_error).inspect_err(|e| {
        forget_if_unauthorized(&app, &auth, e);
    })
}

/// Equips looks (any of `accent`, `theme`, `frame`, `banner`; null frame/banner = none).
#[tauri::command]
pub async fn set_looks(app: AppHandle, looks: serde_json::Value) -> Result<Account> {
    let (auth, client) = saved(&app)?;
    let me = client.put_json(&auth.token, "/api/me/looks", &looks).await.map_err(api_error).inspect_err(|e| {
        forget_if_unauthorized(&app, &auth, e);
    })?;
    let app2 = app.clone();
    off_main(move || save_name(&app2, &auth, &me)).await?
}

/// Sets the profile picture: a square JPEG the window cropped.
#[tauri::command]
pub async fn set_avatar(app: AppHandle, jpeg: Vec<u8>) -> Result<Account> {
    if jpeg.is_empty() || jpeg.len() > MAX_PICTURE_BYTES {
        return Err(AppError::InvalidInput("picture".into()));
    }
    let (auth, client) = saved(&app)?;
    let me = client.put_bytes(&auth.token, "/api/me/avatar", "image/jpeg", jpeg).await.map_err(api_error).inspect_err(|e| {
        forget_if_unauthorized(&app, &auth, e);
    })?;
    let app2 = app.clone();
    off_main(move || save_name(&app2, &auth, &me)).await?
}

/// Back to the initial.
#[tauri::command]
pub async fn remove_avatar(app: AppHandle) -> Result<Account> {
    let (auth, client) = saved(&app)?;
    let me = client.delete_json(&auth.token, "/api/me/avatar").await.map_err(api_error).inspect_err(|e| {
        forget_if_unauthorized(&app, &auth, e);
    })?;
    let app2 = app.clone();
    off_main(move || save_name(&app2, &auth, &me)).await?
}

/// The most a picture may weigh (the server's limit too).
const MAX_PICTURE_BYTES: usize = 10 * 1024 * 1024;

/// A picture file as a `data:` URL the window can draw (for cropping a new profile picture).
#[tauri::command]
pub async fn read_picture(path: std::path::PathBuf) -> Result<String> {
    off_main(move || picture_data_url(&path)).await?
}

fn picture_data_url(path: &std::path::Path) -> Result<String> {
    use base64::Engine;
    let ext = path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default();
    let mime = match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        _ => return Err(AppError::InvalidInput("Use a JPEG, PNG or WebP picture".into())),
    };
    let len = std::fs::metadata(path)?.len();
    if len == 0 || len > MAX_PICTURE_BYTES as u64 {
        return Err(AppError::Refused("Pictures can be up to 10 MB.".into()));
    }
    let bytes = std::fs::read(path)?;
    Ok(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
}

#[derive(Debug, Clone, Serialize)]
pub struct InfoLine {
    pub label: String,
    pub value: String,
}

fn collect_info(app: &AppHandle) -> Vec<(String, String)> {
    let settings = lock(&app.state::<AppState>().settings).clone();
    crate::diagnostics::system_info(&app.package_info().version.to_string(), &settings)
}

/// What a problem report says about this PC, to show before sending.
#[tauri::command]
pub async fn system_info(app: AppHandle) -> Result<Vec<InfoLine>> {
    off_main(move || collect_info(&app).into_iter().map(|(label, value)| InfoLine { label, value }).collect()).await
}

/// Sends a problem report or an idea to the server's admin. A report with `include_logs` carries
/// the system info and the end of the log.
#[tauri::command]
pub async fn send_feedback(app: AppHandle, kind: String, message: String, include_logs: bool) -> Result<()> {
    if kind != "bug" && kind != "idea" {
        return Err(AppError::InvalidInput("kind".into()));
    }
    let message = message.trim().to_string();
    if message.is_empty() || message.chars().count() > 10_000 {
        return Err(AppError::Refused("Write a few words, up to 10,000 characters.".into()));
    }
    let (auth, client) = saved(&app)?;
    crate::elog!("klipt: sending a {} report", if kind == "bug" { "problem" } else { "idea" });
    let app2 = app.clone();
    let (system, logs) = off_main(move || {
        if !include_logs {
            return (None, None);
        }
        let info = collect_info(&app2);
        let logs = format!("{}\n{}", crate::diagnostics::as_text(&info), crate::logging::tail(REPORT_LOG_BYTES));
        (Some(info), Some(logs))
    })
    .await?;
    let mut body = serde_json::json!({ "kind": kind, "message": message, "source": "desktop" });
    if let Some(system) = system {
        body["system"] =
            serde_json::Value::Object(system.into_iter().map(|(k, v)| (k, serde_json::Value::String(v))).collect());
    }
    if let Some(logs) = logs {
        body["logs"] = serde_json::Value::String(logs);
    }
    match client.post_json(&auth.token, "/api/feedback", &body).await {
        Ok(_) => Ok(()),
        Err(ApiError::Server { status: 404, .. }) => {
            Err(AppError::Refused("Your server is too old for reports. Ask your admin to update it.".into()))
        }
        Err(e) => {
            let e = api_error(e);
            forget_if_unauthorized(&app, &auth, &e);
            Err(e)
        }
    }
}

/// Opens the folder with the app's logs.
#[tauri::command]
pub async fn open_logs_folder(app: AppHandle) -> Result<()> {
    off_main(move || {
        let dir = crate::logging::dir().ok_or_else(|| AppError::Internal("logging isn't set up".into()))?;
        app.opener()
            .open_path(dir.to_string_lossy(), None::<&str>)
            .map_err(|e| AppError::Io(std::io::Error::other(e.to_string())))
    })
    .await?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picture_data_url_by_extension_and_size() {
        let dir = tempfile::tempdir().unwrap();
        let png = dir.path().join("me.PNG");
        std::fs::write(&png, [1u8, 2, 3]).unwrap();
        assert_eq!(picture_data_url(&png).unwrap(), "data:image/png;base64,AQID");
        let gif = dir.path().join("me.gif");
        std::fs::write(&gif, [1u8]).unwrap();
        assert!(matches!(picture_data_url(&gif), Err(AppError::InvalidInput(_))));
        let empty = dir.path().join("empty.jpg");
        std::fs::write(&empty, []).unwrap();
        assert!(matches!(picture_data_url(&empty), Err(AppError::Refused(_))));
    }
}
