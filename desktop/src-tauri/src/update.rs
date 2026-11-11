//! Auto-update (Windows installs only; the Linux package updates through pacman).
//!
//! Checks GitHub Releases' `latest.json` at start and every 6 h. When a newer signed build is
//! there, the UI gets `update-available` and offers "Restart to install" (`install_update`).

use tauri::AppHandle;

#[allow(unused_imports)] // AppError is only used off Windows.
use crate::errors::{AppError, Result};

#[cfg(windows)]
mod imp {
    use std::sync::Mutex;
    use std::time::Duration;

    use tauri::{AppHandle, Emitter, Manager};
    use tauri_plugin_updater::{Update, UpdaterExt};

    use crate::errors::{AppError, Result};

    const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);

    /// The update found by the last check, waiting for the user.
    #[derive(Default)]
    pub struct Pending(Mutex<Option<Update>>);

    #[derive(Clone, serde::Serialize)]
    struct Available {
        version: String,
    }

    pub fn start(app: &AppHandle) {
        app.manage(Pending::default());
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                match check(&app).await {
                    Ok(Some(version)) => {
                        let _ = app.emit("update-available", Available { version });
                    }
                    Ok(None) => {}
                    Err(e) => eprintln!("klipt: update check failed: {e}"),
                }
                tokio::time::sleep(CHECK_EVERY).await;
            }
        });
    }

    async fn check(app: &AppHandle) -> std::result::Result<Option<String>, tauri_plugin_updater::Error> {
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

/// Starts the periodic update check (no-op off Windows).
pub fn start(app: &AppHandle) {
    #[cfg(windows)]
    imp::start(app);
    #[cfg(not(windows))]
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
        Err(AppError::InvalidInput("updates come from the system package manager here".into()))
    }
}
