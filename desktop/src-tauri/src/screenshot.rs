//! Screenshots: the screenshot shortcut (or the tray) saves a PNG of the recorded monitor into
//! the clips folder and adds it to the library next to the clips.
//!
//! Linux asks gpu-screen-recorder for a single frame (same KMS capture as recording). Windows
//! grabs the monitor's area of the desktop with the bundled ffmpeg (gdigrab); a game in
//! exclusive fullscreen can come out black there, borderless and windowed games are fine.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Emitter, Manager};

use crate::errors::{AppError, Result};
use crate::library::{ClipKind, ClipView, NewClip};
use crate::settings::Settings;
use crate::state::AppState;

/// A capture that takes longer than this is stuck.
#[cfg_attr(not(any(target_os = "linux", windows)), allow(dead_code))]
const CAPTURE_TIMEOUT: Duration = Duration::from_secs(10);

/// `Screenshot_2026-11-11_18-30-03.png`, in local time like gsr's `Replay_…` files.
pub fn file_name(now: chrono::DateTime<chrono::Local>) -> String {
    format!("Screenshot_{}.png", now.format("%Y-%m-%d_%H-%M-%S"))
}

/// `name` in `dir`, or `name` with `_2`, `_3`… when that file exists (two shots in one second).
pub fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let first = dir.join(name);
    if !first.exists() {
        return first;
    }
    let (stem, ext) = name.rsplit_once('.').unwrap_or((name, "png"));
    (2..).map(|n| dir.join(format!("{stem}_{n}.{ext}"))).find(|p| !p.exists()).expect("some free name")
}

/// Takes a screenshot on a background thread; the UI hears about it as `clip-saved`.
pub fn take_in_background(app: &AppHandle) {
    let app = app.clone();
    let spawned = std::thread::Builder::new().name("klipt-screenshot".into()).spawn(move || {
        let (sound, notification) = {
            let state = app.state::<AppState>();
            let settings = crate::commands::lock(&state.settings);
            (settings.save_sound, settings.save_notification)
        };
        match take(&app) {
            Ok(view) => {
                if sound {
                    crate::sound::play_saved();
                }
                if notification {
                    crate::feedback::notify(&app, "Screenshot saved", view.clip.game_name.as_deref());
                }
                let _ = app.emit(crate::recorder::events::CLIP_SAVED, view);
            }
            Err(e) => {
                eprintln!("klipt: screenshot failed: {e}");
                crate::feedback::notify(&app, "Screenshot not saved", Some(&e.user_message()));
            }
        }
    });
    if let Err(e) = spawned {
        eprintln!("klipt: can't start the screenshot thread: {e}");
    }
}

/// Captures the screen into the clips folder and adds the picture to the library.
pub fn take(app: &AppHandle) -> Result<ClipView> {
    let state = app.state::<AppState>();
    let settings = crate::commands::lock(&state.settings).clone();
    std::fs::create_dir_all(&settings.output_dir)?;
    let path = unique_path(&settings.output_dir, &file_name(chrono::Local::now()));
    capture(app, &settings, &path)?;
    if !path.is_file() {
        return Err(AppError::RecorderFailed("the screenshot wasn't written".into()));
    }

    let game = crate::games::detect_now(&state.data_dir);
    let tools = state.tools.clone();
    let probe = tools.as_ref().and_then(|t| crate::ffmpeg::probe(t, &path).ok());
    let new = NewClip {
        path: path.clone(),
        created_at: SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0),
        duration_ms: 0,
        width: probe.as_ref().map_or(0, |p| p.width),
        height: probe.as_ref().map_or(0, |p| p.height),
        has_mic: false,
        title: game.game_name.clone().unwrap_or_else(|| "Screenshot".into()),
        game_name: game.game_name,
        steam_app_id: game.steam_app_id,
        exe: game.exe,
        has_voice: false,
        kind: ClipKind::Screenshot,
    };
    let (clip, thumb) = {
        let library = crate::commands::lock(&state.library);
        let clip = library.insert(new)?;
        let thumb = library.thumb_path(&clip.id);
        (clip, thumb)
    };
    if let Some(tools) = tools {
        if let Err(e) = crate::ffmpeg::thumbnail(&tools, &clip.path, &thumb, 0) {
            eprintln!("klipt: can't make a thumbnail for {}: {e}", clip.path.display());
        }
    }
    Ok(ClipView { file_missing: false, thumb_path: thumb.is_file().then_some(thumb), clip })
}

/// Linux: one frame of the recorded monitor from gpu-screen-recorder.
#[cfg(target_os = "linux")]
fn capture(_app: &AppHandle, settings: &Settings, path: &Path) -> Result<()> {
    use crate::recorder::gsr;
    use std::process::Command;
    let program = std::env::var_os("KLIPT_GSR_PATH").map(PathBuf::from).unwrap_or_else(|| gsr::PROGRAM.into());
    let monitor = match &settings.display_id {
        Some(id) => id.clone(),
        None => {
            let listed = gsr::stdout_within(Command::new(&program).arg("--list-monitors"), CAPTURE_TIMEOUT)
                .map_err(|e| AppError::RecorderFailed(format!("can't list monitors: {e}")))?;
            let mut displays = gsr::parse_monitors(&listed);
            gsr::mark_primary(&mut displays, gsr::kscreen_primary().as_deref());
            gsr::automatic_monitor(&displays)
                .map(String::from)
                .ok_or_else(|| AppError::RecorderFailed("no monitor to capture".into()))?
        }
    };
    gsr::stdout_within(Command::new(&program).args(["-w", &monitor, "-o"]).arg(path), CAPTURE_TIMEOUT)
        .map_err(|e| AppError::RecorderFailed(format!("gpu-screen-recorder screenshot failed: {e}")))?;
    Ok(())
}

/// Windows: the chosen (or primary) monitor's area of the desktop, through ffmpeg's gdigrab.
#[cfg(windows)]
fn capture(app: &AppHandle, settings: &Settings, path: &Path) -> Result<()> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let tools = app.state::<AppState>().tools.clone().ok_or(AppError::ToolsMissing)?;
    let displays = app.state::<crate::recorder::SupervisorHandle>().list_displays(CAPTURE_TIMEOUT).unwrap_or_default();
    let display = settings
        .display_id
        .as_ref()
        .and_then(|id| displays.iter().find(|d| &d.id == id))
        .or_else(|| displays.iter().find(|d| d.primary))
        .or(displays.first());
    let mut cmd = std::process::Command::new(&tools.ffmpeg);
    cmd.args(["-nostdin", "-hide_banner", "-loglevel", "error", "-y", "-f", "gdigrab", "-framerate", "1"]);
    if let Some(d) = display.filter(|d| d.width > 0 && d.height > 0) {
        cmd.args(["-offset_x", &d.x.to_string(), "-offset_y", &d.y.to_string()]);
        cmd.args(["-video_size", &format!("{}x{}", d.width, d.height)]);
    }
    cmd.args(["-i", "desktop", "-frames:v", "1"]).arg(path).creation_flags(CREATE_NO_WINDOW);
    let output = cmd.output().map_err(|e| AppError::ToolFailed { tool: "ffmpeg", detail: e.to_string() })?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(AppError::ToolFailed { tool: "ffmpeg", detail });
    }
    Ok(())
}

#[cfg(not(any(target_os = "linux", windows)))]
fn capture(_app: &AppHandle, _settings: &Settings, _path: &Path) -> Result<()> {
    Err(AppError::RecorderFailed("screenshots aren't supported on this platform".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn names_follow_the_local_time() {
        let at = chrono::Local.with_ymd_and_hms(2026, 11, 11, 18, 30, 3).unwrap();
        assert_eq!(file_name(at), "Screenshot_2026-11-11_18-30-03.png");
    }

    #[test]
    fn a_taken_name_gets_a_number() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(unique_path(dir.path(), "S.png"), dir.path().join("S.png"));
        std::fs::write(dir.path().join("S.png"), b"x").unwrap();
        assert_eq!(unique_path(dir.path(), "S.png"), dir.path().join("S_2.png"));
        std::fs::write(dir.path().join("S_2.png"), b"x").unwrap();
        assert_eq!(unique_path(dir.path(), "S.png"), dir.path().join("S_3.png"));
    }
}
