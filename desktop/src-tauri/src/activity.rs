//! Achievements from the app's side: what the library says the user did (clips saved, hours
//! recorded, editor tools used…) goes to the server, which decides what that earns. The first
//! report from an account is a backfill: its awards arrive quietly, as one summary.
//!
//! Reports run a few seconds after something changed ([`changed`]) and at every account refresh
//! (startup, login). Logged out, nothing is sent: the library still has it all for the next login.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::errors::{AppError, Result};
use crate::library::LocalActivity;
use crate::state::AppState;
use crate::upload::api::{ApiClient, ApiError};
use crate::upload::auth;

const TIMEOUT: Duration = Duration::from_secs(30);
/// Saving a clip, then trimming it, then uploading: one report for the lot.
const DEBOUNCE: Duration = Duration::from_secs(5);
const FILE_NAME: &str = "activity.json";
/// The event the window shows as a toast.
pub const ACHIEVEMENT_EVENT: &str = "achievement";

/// The `POST /api/me/activity` body.
pub fn report_body(a: &LocalActivity, thumbnail_image: bool, backfill: bool) -> serde_json::Value {
    let mut firsts: Vec<&str> = a.firsts.clone();
    if thumbnail_image {
        firsts.push("thumbnailImage");
    }
    serde_json::json!({
        "counters": {
            "clipsSaved": a.clips_saved,
            "screenshots": a.screenshots,
            "recordedMs": a.recorded_ms,
            "recordings": a.recordings,
            "imports": a.imports,
            "games": a.games,
        },
        "firsts": firsts,
        "backfill": backfill,
    })
}

/// Whether a picture was ever used as a thumbnail here (`<thumbs>/<id>.image.jpg`).
fn any_thumbnail_image(thumbs: &Path) -> bool {
    fs::read_dir(thumbs)
        .map(|dir| dir.flatten().any(|e| e.file_name().to_string_lossy().ends_with(".image.jpg")))
        .unwrap_or(false)
}

/// The accounts that already sent their first (backfill) report: `<app data>/activity.json`.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Backfilled {
    #[serde(default)]
    backfilled: Vec<String>,
}

fn account_key(server_url: &str, username: &str) -> String {
    format!("{}|{}", server_url.trim_end_matches('/'), username.to_lowercase())
}

impl Backfilled {
    pub fn load(dir: &Path) -> Backfilled {
        fs::read(dir.join(FILE_NAME)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    pub fn contains(&self, server_url: &str, username: &str) -> bool {
        self.backfilled.contains(&account_key(server_url, username))
    }

    /// Records the account and saves (atomically: a torn file would only mean one more quiet report).
    pub fn insert(&mut self, dir: &Path, server_url: &str, username: &str) -> Result<()> {
        let key = account_key(server_url, username);
        if self.backfilled.contains(&key) {
            return Ok(());
        }
        self.backfilled.push(key);
        let temp = dir.join(format!("{FILE_NAME}.tmp"));
        {
            let mut file = fs::File::create(&temp)?;
            file.write_all(&serde_json::to_vec_pretty(self)?)?;
            file.sync_all()?;
        }
        fs::rename(&temp, dir.join(FILE_NAME))?;
        Ok(())
    }
}

/// An earned achievement as the server lists it.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Earned {
    pub id: String,
    pub name: String,
    pub description: String,
    pub seen: bool,
    pub quiet: bool,
    #[serde(default)]
    pub reward_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AchievementNotice {
    pub title: String,
    pub body: String,
}

/// What to announce for the unseen achievements, and their ids (to mark seen once shown). The
/// quiet ones (earned for past activity) share one summary; each new one gets its own notice.
pub fn notices(earned: &[Earned]) -> (Vec<AchievementNotice>, Vec<String>) {
    let unseen: Vec<&Earned> = earned.iter().filter(|e| !e.seen).collect();
    let quiet = unseen.iter().filter(|e| e.quiet).count();
    let mut out = Vec::new();
    if quiet > 0 {
        let title = if quiet == 1 {
            "You've already earned an achievement".to_string()
        } else {
            format!("You've already earned {quiet} achievements")
        };
        out.push(AchievementNotice { title, body: "See them in your profile.".into() });
    }
    for e in unseen.iter().filter(|e| !e.quiet) {
        let body = match &e.reward_name {
            Some(reward) => format!("{} · unlocked {reward}", e.description),
            None => e.description.clone(),
        };
        out.push(AchievementNotice { title: format!("Achievement unlocked: {}", e.name), body });
    }
    (out, unseen.iter().map(|e| e.id.clone()).collect())
}

/// Bumped on every change, so a scheduled report only runs if nothing changed after it; and one
/// report at a time, so two can't announce the same achievement.
#[derive(Default)]
pub struct Reporter(AtomicU64, tokio::sync::Mutex<()>);

/// Something that counts changed (a clip saved, an edit, an import…): report in a few seconds.
pub fn changed(app: &AppHandle) {
    let Some(reporter) = app.try_state::<Reporter>() else { return };
    let generation = reporter.0.fetch_add(1, Ordering::SeqCst) + 1;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(DEBOUNCE).await;
        if app.state::<Reporter>().0.load(Ordering::SeqCst) != generation {
            return;
        }
        if let Err(e) = report_now(&app).await {
            crate::elog!("klipt: can't report activity to the server: {e}");
        }
    });
}

fn data_dir(app: &AppHandle) -> PathBuf {
    app.state::<AppState>().data_dir.clone()
}

/// Sends what the library says now, then announces anything new. Logged out, or an older server
/// without achievements: nothing to do.
pub async fn report_now(app: &AppHandle) -> Result<()> {
    let Some(reporter) = app.try_state::<Reporter>() else { return Ok(()) };
    let _one_at_a_time = reporter.1.lock().await;
    let dir = data_dir(app);
    let Some(saved) = auth::load(&dir) else { return Ok(()) };
    let app2 = app.clone();
    let (activity, thumb_image) = crate::commands::off_main(move || {
        let state = app2.state::<AppState>();
        let library = crate::commands::lock(&state.library);
        let thumbs = library.thumb_path("x").parent().map(Path::to_path_buf);
        Ok::<_, AppError>((library.activity()?, thumbs.is_some_and(|t| any_thumbnail_image(&t))))
    })
    .await??;
    let mut backfilled = Backfilled::load(&dir);
    let backfill = !backfilled.contains(&saved.server_url, &saved.username);
    let client = ApiClient::with_timeout(&saved.server_url, TIMEOUT)?;
    let answer = match client.post_json(&saved.token, "/api/me/activity", &report_body(&activity, thumb_image, backfill)).await {
        Ok(answer) => answer,
        Err(ApiError::Server { status: 404, .. }) => return Ok(()),
        Err(e) => return Err(crate::server_clips::failed(&dir, &saved, e)),
    };
    if backfill {
        backfilled.insert(&dir, &saved.server_url, &saved.username)?;
    }
    announce(app, &client, &saved.token, &answer).await;
    Ok(())
}

/// Shows the unseen achievements in `mine` (`MyAchievements`): toasts when the window is in
/// front, else system notifications. Then they're seen (no reminders).
pub async fn announce(app: &AppHandle, client: &ApiClient, token: &str, mine: &serde_json::Value) {
    let earned: Vec<Earned> = mine.get("earned").cloned().and_then(|e| serde_json::from_value(e).ok()).unwrap_or_default();
    let (list, ids) = notices(&earned);
    if list.is_empty() {
        return;
    }
    let focused =
        app.get_webview_window(crate::tray::MAIN_WINDOW).and_then(|w| w.is_focused().ok()).unwrap_or(false);
    for notice in &list {
        if focused {
            if let Err(e) = app.emit(ACHIEVEMENT_EVENT, notice) {
                crate::elog!("klipt: can't show an achievement: {e}");
            }
        } else {
            crate::feedback::notify(app, &notice.title, Some(&notice.body));
        }
    }
    if let Err(e) = client.post_json(token, "/api/me/achievements/seen", &serde_json::json!({ "ids": ids })).await {
        crate::elog!("klipt: can't mark achievements seen: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn earned(id: &str, seen: bool, quiet: bool, reward: Option<&str>) -> Earned {
        Earned {
            id: id.into(),
            name: match id {
                "night-owl" => "Night owl".into(),
                other => other.into(),
            },
            description: "Saved a clip between 3 and 5 in the morning".into(),
            seen,
            quiet,
            reward_name: reward.map(String::from),
        }
    }

    #[test]
    fn notices_one_summary_for_quiet() {
        let list = [
            earned("a", false, true, None),
            earned("b", false, true, None),
            earned("c", false, true, None),
            earned("old", true, false, None),
            earned("night-owl", false, false, Some("Midnight")),
        ];
        let (shown, ids) = notices(&list);
        let titles: Vec<&str> = shown.iter().map(|n| n.title.as_str()).collect();
        assert_eq!(titles, ["You've already earned 3 achievements", "Achievement unlocked: Night owl"]);
        assert!(shown[1].body.ends_with(" · unlocked Midnight"), "{}", shown[1].body);
        assert_eq!(ids, ["a", "b", "c", "night-owl"]);
    }

    #[test]
    fn notices_singular_and_nothing_when_all_seen() {
        let (shown, _) = notices(&[earned("a", false, true, None)]);
        assert_eq!(shown[0].title, "You've already earned an achievement");
        let (none, ids) = notices(&[earned("a", true, true, None)]);
        assert!(none.is_empty() && ids.is_empty());
    }

    #[test]
    fn report_body_shape() {
        let a = LocalActivity { clips_saved: 2, recorded_ms: 60_000, firsts: vec!["cut"], ..Default::default() };
        let body = report_body(&a, true, true);
        assert_eq!(body["counters"]["clipsSaved"], 2);
        assert_eq!(body["counters"]["recordedMs"], 60_000);
        assert_eq!(body["backfill"], true);
        assert_eq!(body["firsts"], serde_json::json!(["cut", "thumbnailImage"]));
        assert_eq!(report_body(&a, false, false)["firsts"], serde_json::json!(["cut"]));
    }

    #[test]
    fn backfilled_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let mut b = Backfilled::load(dir.path());
        assert!(!b.contains("https://klipt.example", "Alice"));
        b.insert(dir.path(), "https://klipt.example/", "Alice").unwrap();
        let again = Backfilled::load(dir.path());
        assert!(again.contains("https://klipt.example", "alice"));
        assert!(!again.contains("https://other.example", "alice"));
    }

    #[test]
    fn thumbnail_pictures_are_found() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!any_thumbnail_image(dir.path()));
        fs::write(dir.path().join("abc.jpg"), b"x").unwrap();
        assert!(!any_thumbnail_image(dir.path()));
        fs::write(dir.path().join("abc.image.jpg"), b"x").unwrap();
        assert!(any_thumbnail_image(dir.path()));
    }
}
