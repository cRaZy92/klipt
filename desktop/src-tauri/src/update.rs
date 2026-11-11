//! Update checks.
//!
//! Windows: checks GitHub Releases' `latest.json` at start and every 6 h. When a newer signed build
//! is there, the UI gets `update-available` and offers "Restart to install" (`install_update`).
//!
//! Linux: the package is built from source with `makepkg`, so Klipt can't install updates itself.
//! A package build stamps its commit (`KLIPT_COMMIT`, set by the PKGBUILD); at start and every 6 h
//! Klipt asks GitHub whether the latest release has commits that build lacks, and if so the UI gets
//! `update-available` with the command that rebuilds and installs the newest version. Dev builds
//! have no stamp and never check.

use std::time::Duration;

use tauri::AppHandle;

#[allow(unused_imports)] // AppError is only used off Windows.
use crate::errors::{AppError, Result};

#[allow(dead_code)] // Unused on macOS.
const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);

/// Payload of `update-available`.
#[derive(Clone, serde::Serialize)]
#[allow(dead_code)]
struct Available {
    version: String,
    /// Linux: the shell command that installs the update. Absent where Klipt installs it itself.
    command: Option<String>,
}

#[cfg(windows)]
mod imp {
    use std::sync::Mutex;

    use tauri::{AppHandle, Emitter, Manager};
    use tauri_plugin_updater::{Update, UpdaterExt};

    use super::{Available, CHECK_EVERY};
    use crate::errors::{AppError, Result};

    /// The update found by the last check, waiting for the user.
    #[derive(Default)]
    pub struct Pending(Mutex<Option<Update>>);

    pub fn start(app: &AppHandle) {
        app.manage(Pending::default());
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                match check(&app).await {
                    Ok(Some(version)) => {
                        let _ = app.emit("update-available", Available { version, command: None });
                    }
                    Ok(None) => {}
                    Err(e) => eprintln!("klipt: update check failed: {e}"),
                }
                tokio::time::sleep(CHECK_EVERY).await;
            }
        });
    }

    pub(super) async fn check(app: &AppHandle) -> std::result::Result<Option<String>, tauri_plugin_updater::Error> {
        let Some(update) = app.updater()?.check().await? else { return Ok(None) };
        let version = update.version.clone();
        *app.state::<Pending>().0.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(update);
        Ok(Some(version))
    }

    pub async fn install(app: &AppHandle) -> Result<()> {
        let update = app.state::<Pending>().0.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take();
        let Some(update) = update else { return Err(AppError::InvalidInput("no update is waiting".into())) };
        update
            .download_and_install(|_, _| {}, || {})
            .await
            .map_err(|e| AppError::Network(format!("couldn't install the update: {e}")))?;
        app.restart();
    }
}

/// GitHub's API for the repo releases come from.
#[allow(dead_code)]
const REPO_API: &str = "https://api.github.com/repos/cRaZy92/klipt";

/// Rebuilds and installs the newest Klipt (Linux package) by running the repo's newest
/// `packaging/aur/update.sh`: it asks for the password once at the start, clones, builds and
/// installs without further questions.
#[allow(dead_code)]
pub const LINUX_UPDATE_COMMAND: &str =
    "sh -c \"$(curl -fsSL https://raw.githubusercontent.com/cRaZy92/klipt/main/packaging/aur/update.sh)\"";

#[cfg(target_os = "linux")]
mod imp {
    use tauri::{AppHandle, Emitter};

    use super::{newer_release, Available, CHECK_EVERY, LINUX_UPDATE_COMMAND, REPO_API};

    pub fn start(app: &AppHandle) {
        let Some(commit) = option_env!("KLIPT_COMMIT").filter(|c| !c.is_empty()) else { return };
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                match newer_release(REPO_API, commit).await {
                    Ok(Some(version)) => {
                        let command = Some(LINUX_UPDATE_COMMAND.to_string());
                        let _ = app.emit("update-available", Available { version, command });
                    }
                    Ok(None) => {}
                    Err(e) => eprintln!("klipt: update check failed: {e}"),
                }
                tokio::time::sleep(CHECK_EVERY).await;
            }
        });
    }
}

#[derive(serde::Deserialize)]
struct Release {
    tag_name: String,
}

#[derive(serde::Deserialize)]
struct Comparison {
    /// Commits the release has that `commit` doesn't.
    ahead_by: u64,
}

/// The latest release's version if it has commits that `commit` lacks (Linux package check).
/// `api` is the repo's GitHub API base, e.g. `https://api.github.com/repos/owner/name`.
#[allow(dead_code)]
pub(crate) async fn newer_release(api: &str, commit: &str) -> std::result::Result<Option<String>, String> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let http = reqwest::Client::builder()
        .user_agent("klipt-desktop")
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;
    let get = |url: String| {
        let http = http.clone();
        async move {
            let res = http
                .get(&url)
                .header("Accept", "application/vnd.github+json")
                .send()
                .await
                .map_err(|e| e.to_string())?;
            if !res.status().is_success() {
                return Err(format!("{url}: HTTP {}", res.status()));
            }
            Ok(res)
        }
    };
    let release: Release = get(format!("{api}/releases/latest")).await?.json().await.map_err(|e| e.to_string())?;
    let tag = release.tag_name;
    let cmp: Comparison =
        get(format!("{api}/compare/{commit}...{tag}")).await?.json().await.map_err(|e| e.to_string())?;
    Ok((cmp.ahead_by > 0).then(|| tag.trim_start_matches('v').to_string()))
}

/// Which Klipt this is, for Settings → About.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    /// `MAJOR.MINOR.<commits on main>` for releases and Linux packages.
    version: String,
    /// The commit a Linux package was built from (short), if stamped.
    commit: Option<String>,
    /// A `pnpm tauri dev` build: its version means nothing.
    dev: bool,
    /// `linux` or `windows` (Rust's `std::env::consts::OS`).
    platform: &'static str,
}

#[tauri::command]
pub fn app_info(app: AppHandle) -> AppInfo {
    AppInfo {
        version: app.package_info().version.to_string(),
        commit: option_env!("KLIPT_COMMIT").filter(|c| !c.is_empty()).map(|c| c.chars().take(7).collect()),
        dev: cfg!(debug_assertions),
        platform: std::env::consts::OS,
    }
}

/// What a manual check ("Check for updates" in Settings) found.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum CheckResult {
    /// A newer version is out (the app's update banner shows it too).
    #[serde(rename_all = "camelCase")]
    Available { version: String, command: Option<String> },
    UpToDate,
    /// This build can't check; `reason` says why.
    Unsupported { reason: String },
}

/// Checks for an update now. A found update is also announced like the periodic check does, so
/// the banner comes back even after "Later".
#[tauri::command]
pub async fn check_for_update(app: AppHandle) -> Result<CheckResult> {
    #[cfg(windows)]
    {
        use tauri::Emitter;
        return match imp::check(&app).await {
            Ok(Some(version)) => {
                let _ = app.emit("update-available", Available { version: version.clone(), command: None });
                Ok(CheckResult::Available { version, command: None })
            }
            Ok(None) => Ok(CheckResult::UpToDate),
            Err(e) => Err(AppError::Network(format!("couldn't check for updates: {e}"))),
        };
    }
    #[cfg(target_os = "linux")]
    {
        use tauri::Emitter;
        let Some(commit) = option_env!("KLIPT_COMMIT").filter(|c| !c.is_empty()) else {
            let reason = if cfg!(debug_assertions) {
                "Development builds don't check for updates."
            } else {
                "This build wasn't installed from the Klipt package, so it can't tell which version it is."
            };
            return Ok(CheckResult::Unsupported { reason: reason.into() });
        };
        return match newer_release(REPO_API, commit).await {
            Ok(Some(version)) => {
                let command = Some(LINUX_UPDATE_COMMAND.to_string());
                let _ = app.emit("update-available", Available { version: version.clone(), command: command.clone() });
                Ok(CheckResult::Available { version, command })
            }
            Ok(None) => Ok(CheckResult::UpToDate),
            Err(e) => Err(AppError::Network(format!("couldn't check for updates: {e}"))),
        };
    }
    #[allow(unreachable_code)]
    {
        let _ = app;
        Ok(CheckResult::Unsupported { reason: "Updates aren't available on this system.".into() })
    }
}

/// Starts the periodic update check (no-op on dev builds and other platforms).
pub fn start(app: &AppHandle) {
    #[cfg(any(windows, target_os = "linux"))]
    imp::start(app);
    #[cfg(not(any(windows, target_os = "linux")))]
    let _ = app;
}

/// Downloads and installs the waiting update, then restarts Klipt.
#[tauri::command]
pub async fn install_update(app: AppHandle) -> Result<()> {
    #[cfg(windows)]
    return imp::install(&app).await;
    #[cfg(not(windows))]
    {
        let _ = app;
        Err(AppError::InvalidInput("run the update command shown in the app instead".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::CheckResult;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::newer_release;

    async fn github(ahead_by: u64) -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/releases/latest"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "tag_name": "v0.1.11" })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/compare/abc123...v0.1.11"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({ "status": "x", "ahead_by": ahead_by })),
            )
            .mount(&server)
            .await;
        server
    }

    #[tokio::test]
    async fn reports_a_release_the_build_lacks() {
        let server = github(3).await;
        assert_eq!(newer_release(&server.uri(), "abc123").await, Ok(Some("0.1.11".into())));
    }

    #[tokio::test]
    async fn quiet_when_the_build_has_the_release() {
        let server = github(0).await;
        assert_eq!(newer_release(&server.uri(), "abc123").await, Ok(None));
    }

    #[test]
    fn check_results_serialize_for_the_ui() {
        let available = CheckResult::Available { version: "0.2.0".into(), command: Some("sh x".into()) };
        assert_eq!(
            serde_json::to_value(available).unwrap(),
            serde_json::json!({ "status": "available", "version": "0.2.0", "command": "sh x" })
        );
        assert_eq!(serde_json::to_value(CheckResult::UpToDate).unwrap(), serde_json::json!({ "status": "upToDate" }));
        assert_eq!(
            serde_json::to_value(CheckResult::Unsupported { reason: "r".into() }).unwrap(),
            serde_json::json!({ "status": "unsupported", "reason": "r" })
        );
    }

    #[tokio::test]
    async fn unknown_commit_is_an_error() {
        let server = github(0).await;
        assert!(newer_release(&server.uri(), "zzz").await.is_err());
    }
}
