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

/// Keeps the display name and admin flag the server answered with (`me`) in the saved login.
fn save_name(app: &AppHandle, auth: &Auth, me: &serde_json::Value) -> Result<Account> {
    let data_dir = app.state::<AppState>().data_dir.clone();
    // Only if this login is still the saved one (a logout or another login may have happened).
    let mut current = auth::load(&data_dir).filter(|a| a.token == auth.token).ok_or(AppError::Unauthorized)?;
    let custom_name = custom_name_of(me);
    let is_admin = me.get("isAdmin").and_then(|v| v.as_bool()).unwrap_or(current.is_admin);
    if current.display_name != custom_name || current.is_admin != is_admin {
        current.display_name = custom_name;
        current.is_admin = is_admin;
        auth::save(&data_dir, &current)?;
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
