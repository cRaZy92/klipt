//! Your account on the server from the app: display name, password, telling the server which
//! app version runs on what PC.

use std::time::Duration;

use tauri::{AppHandle, Manager};

use crate::commands::{clear_auth_if_current, off_main, Account};
use crate::errors::{AppError, Result};
use crate::state::AppState;
use crate::upload::api::{ApiClient, ApiError};
use crate::upload::auth::{self, Auth};

const TIMEOUT: Duration = Duration::from_secs(30);

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

/// Keeps the display name the server answered with in the saved login.
fn save_name(app: &AppHandle, auth: &Auth, custom_name: Option<String>) -> Result<Account> {
    let data_dir = app.state::<AppState>().data_dir.clone();
    // Only if this login is still the saved one (a logout or another login may have happened).
    let mut current = auth::load(&data_dir).filter(|a| a.token == auth.token).ok_or(AppError::Unauthorized)?;
    if current.display_name != custom_name {
        current.display_name = custom_name;
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
    off_main(move || save_name(&app2, &auth, custom_name_of(&me))).await?.map(Some)
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
    off_main(move || save_name(&app2, &auth, custom_name_of(&me))).await?
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

