//! Friends and referral links, through the server's API.

use std::time::Duration;

use serde_json::Value;
use tauri::AppHandle;

use crate::errors::{AppError, Result};
use crate::server_clips::{failed, login};

const TIMEOUT: Duration = Duration::from_secs(20);

/// Usernames are 3–24 letters, digits or underscores; anything else never reaches a URL path.
pub(crate) fn checked_username(name: &str) -> Result<&str> {
    let name = name.trim();
    let ok = (3..=24).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_');
    ok.then_some(name).ok_or_else(|| AppError::InvalidInput("Usernames are 3–24 letters, numbers or underscores.".into()))
}

/// Referral codes are URL-safe; anything else never reaches a URL path.
pub(crate) fn checked_code(code: &str) -> Result<&str> {
    let ok = (1..=100).contains(&code.len()) && code.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    ok.then_some(code).ok_or_else(|| AppError::InvalidInput("not a referral code".into()))
}

/// Your friends and the requests either way (the server's `FriendList`).
#[tauri::command]
pub async fn list_friends(app: AppHandle) -> Result<Value> {
    let (saved, client, data_dir) = login(&app, TIMEOUT)?;
    client.get_json(&saved.token, "/api/friends", &[]).await.map_err(|e| failed(&data_dir, &saved, e))
}

/// Asks `username` to be friends, or accepts their request. Returns `{ state }`.
#[tauri::command]
pub async fn add_friend(app: AppHandle, username: String) -> Result<Value> {
    let name = checked_username(&username)?.to_string();
    let (saved, client, data_dir) = login(&app, TIMEOUT)?;
    client
        .post_json(&saved.token, &format!("/api/friends/{name}"), &serde_json::json!({}))
        .await
        .map_err(|e| failed(&data_dir, &saved, e))
}

/// Declines, cancels or unfriends.
#[tauri::command]
pub async fn remove_friend(app: AppHandle, username: String) -> Result<()> {
    let name = checked_username(&username)?.to_string();
    let (saved, client, data_dir) = login(&app, TIMEOUT)?;
    client.delete(&saved.token, &format!("/api/friends/{name}")).await.map_err(|e| failed(&data_dir, &saved, e))
}

/// A new referral link (`{ code, url, expiresAt }`).
#[tauri::command]
pub async fn make_referral(app: AppHandle) -> Result<Value> {
    let (saved, client, data_dir) = login(&app, TIMEOUT)?;
    client.post_json(&saved.token, "/api/referrals", &serde_json::json!({})).await.map_err(|e| failed(&data_dir, &saved, e))
}

/// Your referral links that still work.
#[tauri::command]
pub async fn list_referrals(app: AppHandle) -> Result<Value> {
    let (saved, client, data_dir) = login(&app, TIMEOUT)?;
    client.get_json(&saved.token, "/api/referrals", &[]).await.map_err(|e| failed(&data_dir, &saved, e))
}

#[tauri::command]
pub async fn revoke_referral(app: AppHandle, code: String) -> Result<()> {
    let code = checked_code(&code)?.to_string();
    let (saved, client, data_dir) = login(&app, TIMEOUT)?;
    client.delete(&saved.token, &format!("/api/referrals/{code}")).await.map_err(|e| failed(&data_dir, &saved, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_username_refuses_paths_and_short_names() {
        for bad in ["a", "a/b", "../x", "x y", "", "abcdefghijklmnopqrstuvwxy"] {
            assert!(checked_username(bad).is_err(), "{bad}");
        }
        assert_eq!(checked_username(" Bob_1 ").unwrap(), "Bob_1");
    }

    #[test]
    fn checked_code_refuses_paths() {
        assert!(checked_code("../x").is_err());
        assert!(checked_code("").is_err());
        assert_eq!(checked_code("Ab_-9").unwrap(), "Ab_-9");
    }
}
