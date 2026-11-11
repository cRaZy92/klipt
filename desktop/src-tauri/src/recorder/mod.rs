//! Recording: a backend runs the replay-buffer helper process (gpu-screen-recorder on Linux,
//! `klipt-recorder` on Windows) and the supervisor keeps it running, debounces saves and turns
//! saved files into library entries.
//!
//! The app only ever signals or kills child processes it spawned itself, by their handle or PID.

#[cfg(target_os = "linux")]
pub mod gsr;
pub mod obs;
pub mod supervisor;

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};

use crate::errors::Result;
use crate::ffmpeg::{self, Tools};
use crate::library::{ClipView, Library, NewClip};
use crate::settings::Settings;
use crate::state::AppState;

pub use supervisor::{Deps, SavedClip, SupervisorEvent, SupervisorHandle, SystemClock};

/// What to record, resolved from [`Settings`].
#[derive(Debug, Clone, PartialEq)]
pub struct RecordConfig {
    /// None = the backend picks (gsr: the monitor KDE calls primary, else the first one;
    /// klipt-recorder: the primary display).
    pub display_id: Option<String>,
    pub fps: u32,
    /// The output is scaled to fit inside this box, keeping the aspect ratio. None = native size.
    pub size: Option<(u32, u32)>,
    pub bitrate_kbps: u32,
    pub buffer_seconds: u32,
    pub mic: bool,
    /// None = the default microphone.
    pub mic_device_id: Option<String>,
    pub output_dir: PathBuf,
    /// Voice chat apps recorded on their own track and left out of the game track (gsr only;
    /// empty = no voice track).
    pub voice_apps: Vec<String>,
}

impl RecordConfig {
    pub fn from_settings(s: &Settings) -> RecordConfig {
        RecordConfig {
            display_id: s.display_id.clone(),
            fps: s.fps,
            size: s.resolution.size(),
            bitrate_kbps: s.bitrate_kbps(),
            buffer_seconds: s.buffer_seconds,
            mic: s.mic_enabled,
            mic_device_id: s.mic_device_id.clone(),
            output_dir: s.output_dir.clone(),
            // Only gsr (Linux) can record "everything except these apps".
            voice_apps: if cfg!(target_os = "linux") { s.voice_apps.clone() } else { Vec::new() },
        }
    }

    /// Whether clips get a voice chat track (after the game and mic tracks).
    pub fn voice_track(&self) -> bool {
        !self.voice_apps.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Display {
    pub id: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub primary: bool,
    /// The top-left corner on the desktop (Windows; 0 when the helper doesn't say).
    #[serde(default)]
    pub x: i32,
    #[serde(default)]
    pub y: i32,
}

/// A microphone the user can pick.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecorderEvent {
    Started { encoder: String },
    /// `duration_ms` is None when the backend doesn't report it (gsr); ffprobe fills it in.
    Saved { path: PathBuf, duration_ms: Option<u64> },
    /// A save was requested while one was still running.
    Busy,
    /// `code` is a backend code such as `save_failed`; never shown to users as-is.
    Error { code: String, message: String },
    /// The helper process exited (None = killed by a signal).
    Exited { code: Option<i32> },
}

pub trait RecorderBackend {
    /// Spawns the helper and starts the replay buffer. Progress arrives as events.
    fn start(&mut self, cfg: &RecordConfig) -> Result<()>;
    /// Asks the helper to write the buffer to a file; `Saved` arrives when it's done.
    fn save(&mut self) -> Result<()>;
    /// Stops the helper and waits for it to exit (killing it after a timeout).
    fn stop(&mut self) -> Result<()>;
    /// A fresh event stream. Events of a helper started after this call go to the returned
    /// receiver; a helper started before keeps sending to the receiver it was started with.
    fn events(&self) -> Receiver<RecorderEvent>;
    fn list_displays(&mut self) -> Result<Vec<Display>>;
    /// Microphones (audio inputs) the helper can record from, without the "default" entry.
    fn list_audio_devices(&mut self) -> Result<Vec<AudioDevice>>;
}

/// The sender helper threads use for one run. `subscribe` swaps in a new channel; threads of an
/// earlier run keep their old sender, so their late events never reach the new subscriber.
#[derive(Clone, Default)]
pub(crate) struct EventSink(Arc<Mutex<Option<Sender<RecorderEvent>>>>);

impl EventSink {
    pub(crate) fn subscribe(&self) -> Receiver<RecorderEvent> {
        let (tx, rx) = mpsc::channel();
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = Some(tx);
        rx
    }

    pub(crate) fn current(&self) -> Option<Sender<RecorderEvent>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }
}

pub(crate) fn send(tx: &Option<Sender<RecorderEvent>>, event: RecorderEvent) {
    if let Some(tx) = tx {
        // The supervisor may have moved on to a newer run; then nobody listens and that's fine.
        let _ = tx.send(event);
    }
}

/// The backend for this platform. On Linux, `KLIPT_BACKEND=obs` picks `klipt-recorder` (debugging).
pub fn default_backend(data_dir: &Path) -> Box<dyn RecorderBackend + Send> {
    #[cfg(target_os = "linux")]
    if std::env::var("KLIPT_BACKEND").as_deref() != Ok("obs") {
        return Box::new(gsr::GsrBackend::new(data_dir));
    }
    let _ = data_dir;
    Box::new(obs::ObsBackend::new())
}

/// Thumbnail position: a third into the clip, but no later than 1 s.
pub fn thumbnail_at_ms(duration_ms: u64) -> u64 {
    (duration_ms / 3).min(1000)
}

/// The save pipeline: probe, insert into the library, thumbnail. A probe or thumbnail failure
/// doesn't lose the clip; it's inserted with what's known and the problem is logged.
pub fn process_saved_clip(library: &Mutex<Library>, tools: Option<&Tools>, saved: SavedClip) -> Result<ClipView> {
    let probe = match tools {
        Some(tools) => match ffmpeg::probe(tools, &saved.path) {
            Ok(p) => Some(p),
            Err(e) => {
                eprintln!("klipt: can't probe {}: {e}", saved.path.display());
                None
            }
        },
        None => None,
    };
    let duration_ms = probe.as_ref().map(|p| p.duration_ms).filter(|d| *d > 0).or(saved.duration_ms).unwrap_or(0);
    let title = saved.game.game_name.clone().unwrap_or_else(|| {
        saved.path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Clip".into())
    });
    // The tracks asked for, checked against what the file really has (game, mic, voice).
    let tracks = probe.as_ref().map(|p| p.audio_tracks);
    let has_mic = saved.has_mic && tracks.is_none_or(|n| n >= 2);
    let has_voice = saved.has_voice && tracks.is_none_or(|n| n >= 2 + u32::from(has_mic));
    let new = NewClip {
        path: saved.path.clone(),
        created_at: SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0),
        duration_ms,
        width: probe.as_ref().map_or(0, |p| p.width),
        height: probe.as_ref().map_or(0, |p| p.height),
        has_mic,
        has_voice,
        kind: crate::library::ClipKind::Clip,
        game_name: saved.game.game_name,
        steam_app_id: saved.game.steam_app_id,
        exe: saved.game.exe,
        title,
    };
    let (clip, thumb) = {
        let library = library.lock().unwrap_or_else(PoisonError::into_inner);
        let clip = library.insert(new)?;
        let thumb = library.thumb_path(&clip.id);
        (clip, thumb)
    };
    if let Some(tools) = tools {
        if let Err(e) = ffmpeg::thumbnail(tools, &clip.path, &thumb, thumbnail_at_ms(duration_ms)) {
            eprintln!("klipt: can't make a thumbnail for {}: {e}", clip.path.display());
        }
    }
    Ok(ClipView { file_missing: !clip.path.is_file(), thumb_path: thumb.is_file().then_some(thumb), clip })
}

/// Tauri event names the UI listens to.
pub mod events {
    /// Payload: the new `ClipView`.
    pub const CLIP_SAVED: &str = "clip-saved";
    /// No payload. The UI shows "Already saving…".
    pub const SAVE_BUSY: &str = "save-busy";
    /// Payload: `{ code, message }` with a friendly message.
    pub const RECORDER_ERROR: &str = "recorder-error";
    /// Payload: `{ encoder }`.
    pub const RECORDING_STARTED: &str = "recording-started";
}

/// Starts the supervisor for the app: the platform backend, game detection in `data_dir`, the
/// library save pipeline from [`AppState`], and Tauri events as the output.
pub fn spawn_for_app(app: &AppHandle, data_dir: PathBuf, settings: &Settings) -> SupervisorHandle {
    let (detect_dir, running_dir) = (data_dir.clone(), data_dir.clone());
    let (pipeline_app, sink_app) = (app.clone(), app.clone());
    let deps = Deps {
        clock: Box::new(SystemClock::new()),
        game_running: Box::new(move || crate::games::is_game_running(&running_dir)),
        detect_game: Box::new(move || crate::games::detect_now(&detect_dir)),
        process_clip: Box::new(move |saved| {
            let state = pipeline_app.state::<AppState>();
            process_saved_clip(&state.library, state.tools.as_ref(), saved)
        }),
        sink: Box::new(move |event| {
            // Sound, notifications and the tray icon: the window is usually hidden while gaming.
            crate::feedback::on_supervisor_event(&sink_app, &event);
            let sent = match event {
                SupervisorEvent::ClipSaved(view) => sink_app.emit(events::CLIP_SAVED, view),
                SupervisorEvent::SaveBusy => sink_app.emit(events::SAVE_BUSY, ()),
                SupervisorEvent::Error { code, message } => {
                    sink_app.emit(events::RECORDER_ERROR, serde_json::json!({ "code": code, "message": message }))
                }
                SupervisorEvent::Recording { encoder } => {
                    sink_app.emit(events::RECORDING_STARTED, serde_json::json!({ "encoder": encoder }))
                }
            };
            if let Err(e) = sent {
                eprintln!("klipt: can't send a recorder event to the UI: {e}");
            }
        }),
    };
    SupervisorHandle::spawn(
        default_backend(&data_dir),
        RecordConfig::from_settings(settings),
        settings.recording_mode,
        deps,
    )
}

/// Saves the replay buffer. Returns right away; the result arrives as `clip-saved`, `save-busy`
/// or `recorder-error`.
#[tauri::command]
pub fn save_clip(recorder: tauri::State<'_, SupervisorHandle>) {
    recorder.save();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::games::Detected;
    use crate::settings::{Quality, Resolution};

    #[test]
    fn record_config_follows_settings() {
        let s = Settings {
            resolution: Resolution::Native,
            quality: Quality::Custom,
            custom_bitrate_kbps: 30_000,
            mic_enabled: true,
            display_id: Some("DP-1".into()),
            ..Settings::default()
        };
        let cfg = RecordConfig::from_settings(&s);
        assert_eq!(cfg.size, None);
        assert_eq!(cfg.bitrate_kbps, 30_000);
        assert!(cfg.mic);
        assert_eq!(cfg.display_id.as_deref(), Some("DP-1"));
        assert_eq!(RecordConfig::from_settings(&Settings::default()).size, Some((1920, 1080)));
    }

    #[test]
    fn thumbnail_is_a_third_in_but_at_most_one_second() {
        assert_eq!(thumbnail_at_ms(900), 300);
        assert_eq!(thumbnail_at_ms(30_000), 1000);
        assert_eq!(thumbnail_at_ms(0), 0);
    }

    #[test]
    fn saved_clip_without_ffmpeg_is_still_inserted() {
        let dir = tempfile::tempdir().unwrap();
        let library = Mutex::new(Library::open(&dir.path().join("library.db")).unwrap());
        let path = dir.path().join("Replay_2026-11-10_12-00-00.mp4");
        std::fs::write(&path, b"not really a video").unwrap();
        let game = Detected { game_name: Some("Active Matter".into()), steam_app_id: Some(2887580), exe: None };
        let view = process_saved_clip(
            &library,
            None,
            SavedClip { path: path.clone(), duration_ms: Some(30_000), game, has_mic: true, has_voice: false },
        )
        .unwrap();
        assert_eq!(view.clip.path, path);
        assert_eq!(view.clip.duration_ms, 30_000);
        assert_eq!(view.clip.game_name.as_deref(), Some("Active Matter"));
        assert_eq!(view.clip.title, "Active Matter");
        assert!(view.clip.has_mic);
        assert!(!view.file_missing);
        assert_eq!(view.thumb_path, None);

        let untagged = process_saved_clip(
            &library,
            None,
            SavedClip { path: dir.path().join("b.mp4"), duration_ms: None, game: Detected::default(), has_mic: false, has_voice: false },
        )
        .unwrap();
        assert_eq!(untagged.clip.title, "b");
        assert!(untagged.file_missing);
    }
}
