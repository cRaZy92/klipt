//! "New clips from friends": checks the feed every few minutes, keeps the count of friends' clips
//! newer than the last one you saw (the Feed tab's badge), and can notify about new uploads.
//!
//! What you've seen is `<app data>/feed.json`, per account; opening the Feed marks it seen. A
//! fresh install starts with everything already seen, so it doesn't greet you with old clips.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};

use crate::state::AppState;
use crate::upload::{auth, ApiClient};

const FILE_NAME: &str = "feed.json";
const FIRST_CHECK_AFTER: Duration = Duration::from_secs(10);
const CHECK_EVERY: Duration = Duration::from_secs(2 * 60);
const TIMEOUT: Duration = Duration::from_secs(20);
/// Tells the UI the count changed: `{ count }`.
pub const FEED_NEW: &str = "feed-new";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Seen {
    /// `<server url>|<username>`: another account starts over.
    account: String,
    /// The newest clip time you've seen (the server's ISO 8601, which sorts as text).
    seen_at: String,
}

#[derive(Default)]
pub struct FeedWatch(Mutex<Inner>);

#[derive(Default)]
struct Inner {
    seen: Option<Seen>,
    /// The last feed page fetched, to recount when you mark it seen.
    items: Vec<Value>,
    me: String,
    count: u32,
    /// Clips up to this time were notified about (or were already there at startup).
    notified_until: Option<String>,
}

fn created_at(clip: &Value) -> &str {
    clip.get("createdAt").and_then(Value::as_str).unwrap_or("")
}

fn owner(clip: &Value) -> &str {
    clip.pointer("/owner/username").and_then(Value::as_str).unwrap_or("")
}

/// Friends' clips in `items` newer than `since` (never your own), newest first as given.
pub fn new_clips<'a>(items: &'a [Value], me: &str, since: &str) -> Vec<&'a Value> {
    items
        .iter()
        .filter(|c| created_at(c) > since && !owner(c).eq_ignore_ascii_case(me))
        .collect()
}

fn newest(items: &[Value]) -> Option<String> {
    items.iter().map(created_at).filter(|t| !t.is_empty()).max().map(String::from)
}

fn load(path: &Path) -> Option<Seen> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

fn save(path: &Path, seen: &Seen) {
    let result = serde_json::to_vec(seen).map_err(std::io::Error::other).and_then(|bytes| fs::write(path, bytes));
    if let Err(e) = result {
        eprintln!("klipt: can't remember which feed clips were seen: {e}");
    }
}

fn path(app: &AppHandle) -> PathBuf {
    app.state::<AppState>().data_dir.join(FILE_NAME)
}

impl FeedWatch {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn count(&self) -> u32 {
        self.lock().count
    }
}

/// Recounts and tells the UI when the number changed.
fn set_count(app: &AppHandle, inner: &mut Inner) {
    let count = match &inner.seen {
        Some(seen) => new_clips(&inner.items, &inner.me, &seen.seen_at).len() as u32,
        None => 0,
    };
    if count != inner.count {
        inner.count = count;
        let _ = app.emit(FEED_NEW, serde_json::json!({ "count": count }));
    }
}

/// One check of the feed.
async fn check(app: &AppHandle) {
    let watch = app.state::<FeedWatch>();
    let data_dir = app.state::<AppState>().data_dir.clone();
    let Some(saved) = auth::load(&data_dir) else {
        let mut inner = watch.lock();
        inner.items.clear();
        set_count(app, &mut inner);
        return;
    };
    let Ok(client) = ApiClient::with_timeout(&saved.server_url, TIMEOUT) else { return };
    let query = [("scope", "feed".to_string()), ("limit", "60".to_string())];
    let page = match client.get_json(&saved.token, "/api/clips", &query).await {
        Ok(page) => page,
        Err(e) => {
            eprintln!("klipt: can't check the feed for new clips: {e}");
            return;
        }
    };
    let items = page.get("items").and_then(Value::as_array).cloned().unwrap_or_default();
    let account = format!("{}|{}", saved.server_url, saved.username);
    let file = path(app);

    let fresh: Vec<Value> = {
        let mut inner = watch.lock();
        if inner.seen.is_none() {
            inner.seen = load(&file);
        }
        if inner.seen.as_ref().is_none_or(|s| s.account != account) {
            // First check for this account: everything so far counts as seen.
            let seen = Seen { account, seen_at: newest(&items).unwrap_or_default() };
            save(&file, &seen);
            inner.notified_until = Some(seen.seen_at.clone());
            inner.seen = Some(seen);
        }
        let since = inner.notified_until.clone().or_else(|| inner.seen.as_ref().map(|s| s.seen_at.clone())).unwrap_or_default();
        let fresh = new_clips(&items, &saved.username, &since).into_iter().cloned().collect();
        inner.notified_until = newest(&items).max(Some(since));
        inner.me = saved.username.clone();
        inner.items = items;
        set_count(app, &mut inner);
        fresh
    };

    let notify = crate::commands::lock(&app.state::<AppState>().settings).friend_notifications;
    if notify && !fresh.is_empty() {
        let (title, body) = match fresh.as_slice() {
            [clip] => (
                format!("{} shared a clip", owner(clip)),
                clip.get("title").and_then(Value::as_str).unwrap_or("").to_string(),
            ),
            many => (format!("{} new clips from friends", many.len()), "Open Klipt's Feed to watch them.".to_string()),
        };
        crate::feedback::notify(app, &title, Some(&body));
    }
}

/// Starts checking the feed in the background.
pub fn start(app: &AppHandle) {
    app.manage(FeedWatch::default());
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_CHECK_AFTER).await;
        loop {
            check(&app).await;
            tokio::time::sleep(CHECK_EVERY).await;
        }
    });
}

/// How many friends' clips are new since you last looked.
#[tauri::command]
pub fn feed_new_count(app: AppHandle) -> u32 {
    app.try_state::<FeedWatch>().map_or(0, |w| w.count())
}

/// You've looked at the feed up to the clip made at `newest` (its `createdAt`).
#[tauri::command]
pub fn mark_feed_seen(app: AppHandle, newest: String) {
    let Some(watch) = app.try_state::<FeedWatch>() else { return };
    let mut inner = watch.lock();
    let Some(seen) = inner.seen.as_mut() else { return };
    if newest.as_str() <= seen.seen_at.as_str() {
        return;
    }
    seen.seen_at = newest;
    let seen = seen.clone();
    save(&path(&app), &seen);
    set_count(&app, &mut inner);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn clip(at: &str, who: &str) -> Value {
        json!({ "createdAt": at, "owner": { "username": who } })
    }

    #[test]
    fn counts_friends_clips_newer_than_seen() {
        let items = vec![
            clip("2026-11-11T12:00:00.000Z", "bob"),
            clip("2026-11-11T11:00:00.000Z", "Adam"),
            clip("2026-11-11T10:00:00.000Z", "carol"),
            clip("2026-11-11T09:00:00.000Z", "bob"),
        ];
        let fresh = new_clips(&items, "adam", "2026-11-11T09:30:00.000Z");
        assert_eq!(fresh.len(), 2, "own clip and older clip left out");
        assert_eq!(new_clips(&items, "adam", "").len(), 3);
        assert_eq!(newest(&items).as_deref(), Some("2026-11-11T12:00:00.000Z"));
        assert_eq!(newest(&[]), None);
    }

    #[test]
    fn seen_survives_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(FILE_NAME);
        assert_eq!(load(&file), None);
        let seen = Seen { account: "https://k|adam".into(), seen_at: "2026-11-11T12:00:00.000Z".into() };
        save(&file, &seen);
        assert_eq!(load(&file), Some(seen));
    }
}
