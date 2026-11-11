//! Windows backend (and a Linux debugging aid): `klipt-recorder`, our libobs helper, spoken to
//! over the JSON-lines protocol in `desktop/recorder-obs/src/protocol.rs`.
//!
//! One helper process per recording session. Its stderr (libobs logs) is always drained into the
//! app log, because a full pipe would block libobs threads.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::json;

use super::{send, AudioDevice, Display, EventSink, RecordConfig, RecorderBackend, RecorderEvent};
use crate::errors::{AppError, Result};

/// libobs startup can be slow on a cold disk.
pub const READY_TIMEOUT: Duration = Duration::from_secs(30);
pub const REPLY_TIMEOUT: Duration = Duration::from_secs(10);
/// After `stop`, wait this long for `stopped` before killing the helper.
pub const STOP_TIMEOUT: Duration = Duration::from_secs(15);
/// After `stopped`, wait this long for the process to exit before killing it.
const EXIT_TIMEOUT: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(100);
/// The output box for Native when the display's size can't be found.
const FALLBACK_SIZE: (u32, u32) = (1920, 1080);

/// Where `klipt-recorder` is: `KLIPT_RECORDER_PATH`, else next to the app's exe
/// (`obs/bin/64bit/klipt-recorder.exe` on Windows, since it must sit in OBS's `bin/64bit`).
pub fn helper_path() -> PathBuf {
    if let Some(path) = std::env::var_os("KLIPT_RECORDER_PATH") {
        return path.into();
    }
    let exe_dir = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)).unwrap_or_default();
    if cfg!(windows) {
        exe_dir.join("obs").join("bin").join("64bit").join("klipt-recorder.exe")
    } else {
        exe_dir.join("klipt-recorder")
    }
}

/// An event line from the helper (the subset of fields the app uses).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "event", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum HelperEvent {
    Ready {
        #[serde(default)]
        encoders: Vec<String>,
    },
    Displays {
        items: Vec<Display>,
    },
    AudioDevices {
        #[serde(default)]
        inputs: Vec<AudioDevice>,
    },
    Started {
        encoder: String,
    },
    Saved {
        path: String,
        duration_ms: u64,
    },
    Stopped,
    Error {
        code: String,
        message: String,
    },
    Stats {},
}

pub fn parse_event_line(line: &str) -> Option<HelperEvent> {
    serde_json::from_str(line.trim().trim_start_matches('\u{feff}')).ok()
}

/// The `start` command line. `size` is the output box.
pub fn start_command(cfg: &RecordConfig, size: (u32, u32)) -> String {
    json!({
        "cmd": "start",
        "displayId": cfg.display_id,
        "fps": cfg.fps,
        "width": size.0,
        "height": size.1,
        "bitrateKbps": cfg.bitrate_kbps,
        "bufferSeconds": cfg.buffer_seconds,
        "mic": cfg.mic,
        "micDeviceId": cfg.mic_device_id,
        "outputDir": cfg.output_dir.to_string_lossy(),
        "encoder": "auto",
    })
    .to_string()
}

/// The output box: the chosen resolution, or for Native the display's own size.
pub fn output_size(cfg: &RecordConfig, displays: &[Display]) -> (u32, u32) {
    cfg.size.unwrap_or_else(|| {
        let display = match &cfg.display_id {
            Some(id) => displays.iter().find(|d| &d.id == id),
            None => displays.iter().find(|d| d.primary).or(displays.first()),
        };
        display.map_or(FALLBACK_SIZE, |d| (d.width, d.height))
    })
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// One running helper process.
struct Helper {
    /// Holds the child until it has been reaped, so killing it never hits a reused PID.
    child: Arc<Mutex<Option<Child>>>,
    stdin: ChildStdin,
    /// `ready`, `displays`, `audioDevices` and `stopped`: answers the backend waits for.
    replies: Receiver<HelperEvent>,
    waiter: JoinHandle<()>,
}

impl Helper {
    /// `tx` receives the session events (`started`, `saved`, errors, `Exited`); None for a
    /// short-lived helper that only lists displays.
    fn spawn(program: &Path, tx: Option<Sender<RecorderEvent>>) -> Result<Helper> {
        let mut cmd = Command::new(program);
        cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        if let Some(dir) = program.parent().filter(|d| !d.as_os_str().is_empty()) {
            cmd.current_dir(dir);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = cmd.spawn().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                AppError::RecorderMissing(program.display().to_string())
            } else {
                AppError::RecorderFailed(format!("can't start {}: {e}", program.display()))
            }
        })?;
        let (stdin, stdout, stderr) = match (child.stdin.take(), child.stdout.take(), child.stderr.take()) {
            (Some(i), Some(o), Some(e)) => (i, o, e),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(AppError::RecorderFailed("klipt-recorder has no stdio pipes".into()));
            }
        };

        thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(|l| l.ok()) {
                eprintln!("klipt-recorder: {line}");
            }
        });

        let (reply_tx, replies) = mpsc::channel();
        let events = tx.clone();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(|l| l.ok()) {
                let Some(event) = parse_event_line(&line) else {
                    eprintln!("klipt: unexpected line from klipt-recorder: {line}");
                    continue;
                };
                match event {
                    HelperEvent::Started { encoder } => send(&events, RecorderEvent::Started { encoder }),
                    HelperEvent::Saved { path, duration_ms } => {
                        send(&events, RecorderEvent::Saved { path: path.into(), duration_ms: Some(duration_ms) })
                    }
                    HelperEvent::Error { code, .. } if code == "busy" => send(&events, RecorderEvent::Busy),
                    HelperEvent::Error { code, message } => {
                        eprintln!("klipt: klipt-recorder error {code}: {message}");
                        send(&events, RecorderEvent::Error { code, message })
                    }
                    HelperEvent::Stats {} => {}
                    reply @ (HelperEvent::Ready { .. }
                    | HelperEvent::Displays { .. }
                    | HelperEvent::AudioDevices { .. }
                    | HelperEvent::Stopped) => {
                        let _ = reply_tx.send(reply);
                    }
                }
            }
        });

        let child = Arc::new(Mutex::new(Some(child)));
        let waiter = {
            let child = child.clone();
            thread::spawn(move || loop {
                thread::sleep(POLL);
                let mut slot = lock(&child);
                let Some(c) = slot.as_mut() else { return };
                let code = match c.try_wait() {
                    Ok(None) => continue,
                    Ok(Some(status)) => status.code(),
                    Err(e) => {
                        eprintln!("klipt: can't check on klipt-recorder: {e}");
                        None
                    }
                };
                *slot = None;
                drop(slot);
                send(&tx, RecorderEvent::Exited { code });
                return;
            })
        };
        let helper = Helper { child, stdin, replies, waiter };
        match helper.wait_for(READY_TIMEOUT, |e| matches!(e, HelperEvent::Ready { .. })) {
            Some(_) => Ok(helper),
            None => {
                helper.shut_down();
                Err(AppError::RecorderFailed("klipt-recorder didn't become ready".into()))
            }
        }
    }

    fn command(&mut self, line: &str) -> Result<()> {
        writeln!(self.stdin, "{line}")
            .and_then(|_| self.stdin.flush())
            .map_err(|e| AppError::RecorderFailed(format!("can't talk to klipt-recorder: {e}")))
    }

    /// The next reply matching `want`, skipping others; None on timeout or if the helper exited.
    fn wait_for(&self, timeout: Duration, want: impl Fn(&HelperEvent) -> bool) -> Option<HelperEvent> {
        let deadline = Instant::now() + timeout;
        loop {
            match self.replies.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                Ok(e) if want(&e) => return Some(e),
                Ok(_) => continue,
                Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => return None,
            }
        }
    }

    fn list_displays(&mut self) -> Result<Vec<Display>> {
        self.command(r#"{"cmd":"listDisplays"}"#)?;
        match self.wait_for(REPLY_TIMEOUT, |e| matches!(e, HelperEvent::Displays { .. })) {
            Some(HelperEvent::Displays { items }) => Ok(items),
            _ => Err(AppError::RecorderFailed("klipt-recorder didn't list displays".into())),
        }
    }

    fn list_audio_devices(&mut self) -> Result<Vec<AudioDevice>> {
        self.command(r#"{"cmd":"listAudioDevices"}"#)?;
        match self.wait_for(REPLY_TIMEOUT, |e| matches!(e, HelperEvent::AudioDevices { .. })) {
            // libobs lists the system default as "default"; the UI offers "Default microphone".
            Some(HelperEvent::AudioDevices { inputs }) => Ok(inputs.into_iter().filter(|d| d.id != "default").collect()),
            _ => Err(AppError::RecorderFailed("klipt-recorder didn't list audio devices".into())),
        }
    }

    fn is_alive(&self) -> bool {
        lock(&self.child).is_some()
    }

    /// `stop`, wait for `stopped` and the exit; kill the child (by its handle) if it hangs.
    fn shut_down(mut self) {
        if self.is_alive() {
            match self.command(r#"{"cmd":"stop"}"#) {
                Ok(()) => {
                    if self.wait_for(STOP_TIMEOUT, |e| matches!(e, HelperEvent::Stopped)).is_none() {
                        eprintln!("klipt: klipt-recorder didn't confirm the stop within {STOP_TIMEOUT:?}");
                    }
                }
                Err(e) => eprintln!("klipt: {e}"),
            }
            let deadline = Instant::now() + EXIT_TIMEOUT;
            while self.is_alive() && Instant::now() < deadline {
                thread::sleep(POLL / 2);
            }
            if let Some(child) = lock(&self.child).as_mut() {
                eprintln!("klipt: killing klipt-recorder (pid {})", child.id());
                let _ = child.kill();
            }
        }
        let _ = self.waiter.join();
    }
}

pub struct ObsBackend {
    sink: EventSink,
    helper: Option<Helper>,
    program: PathBuf,
}

impl ObsBackend {
    pub fn new() -> ObsBackend {
        ObsBackend { sink: EventSink::default(), helper: None, program: helper_path() }
    }
}

impl Default for ObsBackend {
    fn default() -> Self {
        ObsBackend::new()
    }
}

impl RecorderBackend for ObsBackend {
    fn start(&mut self, cfg: &RecordConfig) -> Result<()> {
        if self.helper.as_ref().is_some_and(Helper::is_alive) {
            return Err(AppError::RecorderFailed("klipt-recorder is already running".into()));
        }
        if let Some(old) = self.helper.take() {
            old.shut_down();
        }
        std::fs::create_dir_all(&cfg.output_dir)?;
        let mut helper = Helper::spawn(&self.program, self.sink.current())?;
        let size = match cfg.size {
            Some(size) => size,
            None => {
                let displays = helper.list_displays().unwrap_or_else(|e| {
                    eprintln!("klipt: {e}");
                    vec![]
                });
                output_size(cfg, &displays)
            }
        };
        if let Err(e) = helper.command(&start_command(cfg, size)) {
            // Don't keep a half-started helper around: the next start would find it "running".
            helper.shut_down();
            return Err(e);
        }
        self.helper = Some(helper);
        Ok(())
    }

    fn save(&mut self) -> Result<()> {
        match &mut self.helper {
            Some(helper) if helper.is_alive() => helper.command(r#"{"cmd":"save"}"#),
            _ => Err(AppError::NotRecording),
        }
    }

    fn stop(&mut self) -> Result<()> {
        if let Some(helper) = self.helper.take() {
            helper.shut_down();
        }
        Ok(())
    }

    fn events(&self) -> Receiver<RecorderEvent> {
        self.sink.subscribe()
    }

    fn list_displays(&mut self) -> Result<Vec<Display>> {
        if let Some(helper) = self.helper.as_mut().filter(|h| h.is_alive()) {
            return helper.list_displays();
        }
        let mut helper = Helper::spawn(&self.program, None)?;
        let displays = helper.list_displays();
        helper.shut_down();
        displays
    }

    fn list_audio_devices(&mut self) -> Result<Vec<AudioDevice>> {
        if let Some(helper) = self.helper.as_mut().filter(|h| h.is_alive()) {
            return helper.list_audio_devices();
        }
        let mut helper = Helper::spawn(&self.program, None)?;
        let devices = helper.list_audio_devices();
        helper.shut_down();
        devices
    }
}

impl Drop for ObsBackend {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> RecordConfig {
        RecordConfig {
            display_id: None,
            fps: 60,
            size: Some((1920, 1080)),
            bitrate_kbps: 20_000,
            buffer_seconds: 30,
            mic: false,
            mic_device_id: None,
            output_dir: "C:\\Users\\a\\Videos\\Klipt".into(),
            voice_apps: Vec::new(),
        }
    }

    #[test]
    fn start_command_uses_the_protocol_field_names() {
        let line = start_command(&RecordConfig { mic: true, mic_device_id: Some("m1".into()), ..cfg() }, (1280, 720));
        let v: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(
            v,
            json!({"cmd":"start","displayId":null,"fps":60,"width":1280,"height":720,"bitrateKbps":20000,
                   "bufferSeconds":30,"mic":true,"micDeviceId":"m1","outputDir":"C:\\Users\\a\\Videos\\Klipt",
                   "encoder":"auto"})
        );
        assert!(!line.contains('\n'));
    }

    #[test]
    fn parses_helper_events() {
        assert_eq!(
            parse_event_line(r#"{"event":"ready","version":"0.1.0","platform":"windows","encoders":["obs_x264"]}"#),
            Some(HelperEvent::Ready { encoders: vec!["obs_x264".into()] })
        );
        assert_eq!(
            parse_event_line(r#"{"event":"saved","path":"C:\\c\\a.mp4","durationMs":30012}"#),
            Some(HelperEvent::Saved { path: "C:\\c\\a.mp4".into(), duration_ms: 30012 })
        );
        assert_eq!(
            parse_event_line(r#"{"event":"error","code":"busy","message":"Already saving"}"#),
            Some(HelperEvent::Error { code: "busy".into(), message: "Already saving".into() })
        );
        assert_eq!(parse_event_line(r#"{"event":"stopped"}"#), Some(HelperEvent::Stopped));
        assert_eq!(
            parse_event_line(
                r#"{"event":"stats","cpuSeconds":1.0,"wallSeconds":2.0,"avgCpuPercentOfOneCore":50.0,"maxRssMb":3}"#
            ),
            Some(HelperEvent::Stats {})
        );
        assert_eq!(
            parse_event_line(
                r#"{"event":"displays","items":[{"id":"d1","name":"DELL","width":2560,"height":1440,"primary":true}]}"#
            ),
            Some(HelperEvent::Displays {
                items: vec![Display { id: "d1".into(), name: "DELL".into(), width: 2560, height: 1440, primary: true, x: 0, y: 0 }]
            })
        );
        assert_eq!(
            parse_event_line(
                r#"{"event":"audioDevices","outputs":[{"id":"default","name":"Default"}],"inputs":[{"id":"{0.0.1}.{m}","name":"Mic"}]}"#
            ),
            Some(HelperEvent::AudioDevices { inputs: vec![AudioDevice { id: "{0.0.1}.{m}".into(), name: "Mic".into() }] })
        );
        assert_eq!(parse_event_line("libobs: not json"), None);
    }

    #[test]
    fn native_size_comes_from_the_display() {
        let displays = vec![
            Display { id: "a".into(), name: "A".into(), width: 1920, height: 1080, primary: false, x: 0, y: 0 },
            Display { id: "b".into(), name: "B".into(), width: 3440, height: 1440, primary: true, x: 0, y: 0 },
        ];
        let native = RecordConfig { size: None, ..cfg() };
        assert_eq!(output_size(&native, &displays), (3440, 1440));
        assert_eq!(output_size(&RecordConfig { display_id: Some("a".into()), ..native.clone() }, &displays), (1920, 1080));
        assert_eq!(output_size(&native, &[]), FALLBACK_SIZE);
        assert_eq!(output_size(&cfg(), &displays), (1920, 1080));
    }

    #[cfg(unix)]
    #[test]
    fn a_failed_start_command_shuts_the_helper_down() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        // A helper that closes its stdin before saying it's ready: writing `start` fails.
        let script = dir.path().join("fake-recorder.sh");
        std::fs::write(
            &script,
            "#!/bin/sh\nexec 0<&-\necho '{\"event\":\"ready\",\"version\":\"0\",\"platform\":\"linux\",\"encoders\":[]}'\nsleep 1\n",
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut backend = ObsBackend::new();
        backend.program = script;
        let c = RecordConfig { output_dir: dir.path().join("out"), ..cfg() };
        assert!(matches!(backend.start(&c), Err(AppError::RecorderFailed(_))));
        assert!(backend.helper.is_none(), "the half-started helper must not be kept");
        // The next start is a fresh attempt, not "already running".
        match backend.start(&c) {
            Err(AppError::RecorderFailed(m)) => assert!(!m.contains("already running"), "{m}"),
            other => panic!("{other:?}"),
        }
    }
}
