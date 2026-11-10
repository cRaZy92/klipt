//! Linux backend: gpu-screen-recorder (gsr) in replay mode. See the decision record
//! `docs/superpowers/specs/2026-11-09-klipt-capture-decision.md`, "Linux backend details".
//!
//! - SIGUSR1 to the child PID saves the replay; gsr prints the saved file's path on stdout.
//! - SIGINT stops it; after 5 s the child is killed through its handle.
//! - KMS capture (`-w <monitor>`) that exits non-zero within 3 s is retried once with the portal.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use nix::sys::prctl::set_pdeathsig;
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;

use super::{send, AudioDevice, Display, EventSink, RecordConfig, RecorderBackend, RecorderEvent};
use crate::errors::{AppError, Result};

pub const PROGRAM: &str = "gpu-screen-recorder";
/// Name of the portal session token file in the app data dir.
pub const PORTAL_TOKEN_FILE: &str = "portal-token";
/// KMS capture that dies this quickly with an error is retried with the portal.
pub const KMS_FALLBACK_WINDOW: Duration = Duration::from_secs(3);
pub const STOP_TIMEOUT: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(100);

/// What gsr captures (`-w`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// A monitor name from `--list-monitors`, captured through KMS.
    Monitor(String),
    /// The desktop portal (asks the user once; the token file remembers the choice).
    Portal,
}

/// The gsr command line (without the program name), in the decision record's order.
pub fn build_args(cfg: &RecordConfig, target: &Target, portal_token: &Path) -> Vec<String> {
    let mut args: Vec<String> = vec!["-w".into()];
    match target {
        Target::Monitor(name) => args.push(name.clone()),
        Target::Portal => args.extend([
            "portal".into(),
            "-restore-portal-session".into(),
            "yes".into(),
            "-portal-session-token-filepath".into(),
            portal_token.to_string_lossy().into_owned(),
        ]),
    }
    args.extend(["-f".into(), cfg.fps.to_string()]);
    if let Some((w, h)) = cfg.size {
        args.extend(["-s".into(), format!("{w}x{h}")]);
    }
    args.extend(["-k", "h264", "-bm", "cbr", "-q"].map(String::from));
    args.push(cfg.bitrate_kbps.to_string());
    args.extend(["-ac", "aac", "-a", "default_output"].map(String::from));
    if cfg.mic {
        args.push("-a".into());
        args.push(cfg.mic_device_id.clone().unwrap_or_else(|| "default_input".into()));
    }
    args.extend(["-c", "mp4", "-r"].map(String::from));
    args.push(cfg.buffer_seconds.to_string());
    args.extend(["-replay-storage", "ram", "-o"].map(String::from));
    args.push(cfg.output_dir.to_string_lossy().into_owned());
    args
}

/// Parses `gpu-screen-recorder --list-monitors` output (`NAME|WxH` per line).
pub fn parse_monitors(text: &str) -> Vec<Display> {
    text.lines()
        .filter_map(|line| {
            let (name, size) = line.trim().split_once('|')?;
            let (w, h) = size.trim().split_once('x')?;
            let name = name.trim();
            if name.is_empty() {
                return None;
            }
            Some(Display {
                id: name.to_string(),
                name: name.to_string(),
                width: w.parse().ok()?,
                height: h.parse().ok()?,
                // Wayland doesn't say which monitor is primary.
                primary: false,
            })
        })
        .collect()
}

/// Parses `gpu-screen-recorder --list-audio-devices` output (`ID|Description` per line), keeping
/// only microphones: not `default_output`/`default_input` (the UI offers "Default microphone"
/// itself) and not `.monitor` sources, which record what an output plays.
pub fn parse_audio_devices(text: &str) -> Vec<AudioDevice> {
    text.lines()
        .filter_map(|line| {
            let (id, name) = line.trim().split_once('|')?;
            let (id, name) = (id.trim(), name.trim());
            if id.is_empty() || id == "default_output" || id == "default_input" || id.ends_with(".monitor") {
                return None;
            }
            let name = if name.is_empty() { id } else { name };
            Some(AudioDevice { id: id.to_string(), name: name.to_string() })
        })
        .collect()
}

/// The monitor KDE treats as primary, from `kscreen-doctor -j`: the enabled output with
/// priority 1. Wayland itself has no primary flag.
pub fn parse_kscreen_primary(json: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    value["outputs"]
        .as_array()?
        .iter()
        .find(|o| o["enabled"].as_bool() == Some(true) && o["priority"].as_u64() == Some(1))
        .and_then(|o| o["name"].as_str())
        .map(String::from)
}

/// Marks `primary` (a monitor name) as the primary display.
pub fn mark_primary(displays: &mut [Display], primary: Option<&str>) {
    for d in displays {
        d.primary = primary == Some(d.id.as_str());
    }
}

/// The monitor to record when the user chose "Automatic": the primary one, else the first.
pub fn automatic_monitor(displays: &[Display]) -> Option<&str> {
    displays.iter().find(|d| d.primary).or(displays.first()).map(|d| d.id.as_str())
}

/// How long `kscreen-doctor` may take before it's given up on.
const KSCREEN_TIMEOUT: Duration = Duration::from_secs(3);

/// Asks KDE which monitor is primary. None off KDE, or if it doesn't answer in time.
fn kscreen_primary() -> Option<String> {
    let mut child = Command::new("kscreen-doctor")
        .arg("-j")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    // Read on a thread so a large answer can't fill the pipe while this waits.
    let reader = thread::spawn(move || {
        let mut text = String::new();
        std::io::Read::read_to_string(&mut stdout, &mut text).ok().map(|_| text)
    });
    let deadline = Instant::now() + KSCREEN_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => thread::sleep(POLL / 2),
            _ => {
                eprintln!("klipt: kscreen-doctor didn't answer; using the first monitor");
                // Our own child, by its handle.
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    parse_kscreen_primary(&reader.join().ok()??)
}

/// A stdout line from gsr that names an existing `.mp4` file is a finished save.
pub fn parse_saved_line(line: &str) -> Option<PathBuf> {
    let path = Path::new(line.trim());
    let is_mp4 = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("mp4"));
    (path.is_absolute() && is_mp4 && path.is_file()).then(|| path.to_path_buf())
}

/// State shared between the backend and the thread that waits for the child.
/// The slot holds the child until it has been reaped, so its PID is never stale while in there.
struct Shared {
    child: Mutex<Option<Child>>,
    stop_requested: AtomicBool,
}

struct Run {
    shared: Arc<Shared>,
    waiter: JoinHandle<()>,
}

pub struct GsrBackend {
    program: PathBuf,
    portal_token: PathBuf,
    sink: EventSink,
    run: Option<Run>,
}

impl GsrBackend {
    /// `data_dir` is the app data dir; the portal token is kept there.
    pub fn new(data_dir: &Path) -> GsrBackend {
        let program = std::env::var_os("KLIPT_GSR_PATH").map(PathBuf::from).unwrap_or_else(|| PROGRAM.into());
        GsrBackend { program, portal_token: data_dir.join(PORTAL_TOKEN_FILE), sink: EventSink::default(), run: None }
    }

    #[cfg(test)]
    fn with_program(program: PathBuf, data_dir: &Path) -> GsrBackend {
        let mut backend = GsrBackend::new(data_dir);
        backend.program = program;
        backend
    }

    fn is_running(&self) -> bool {
        self.run.as_ref().is_some_and(|r| lock(&r.shared.child).is_some())
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

fn spawn_error(program: &Path, e: std::io::Error) -> AppError {
    if e.kind() == std::io::ErrorKind::NotFound {
        AppError::RecorderMissing(program.display().to_string())
    } else {
        AppError::RecorderFailed(format!("can't start {}: {e}", program.display()))
    }
}

/// Spawns gsr with piped output: stdout lines that name a saved file become `Saved`; stderr goes
/// to the app log (and must keep being read so gsr never blocks on a full pipe).
fn spawn_child(program: &Path, args: &[String], tx: &Option<Sender<RecorderEvent>>) -> std::io::Result<Child> {
    eprintln!("klipt: starting {} {}", program.display(), args.join(" "));
    let mut cmd = Command::new(program);
    cmd.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    // If Klipt dies without stopping it, gsr must not keep recording on its own. (The signal
    // fires when the spawning thread ends; gsr is spawned from long-lived recorder threads.)
    // SAFETY: prctl is async-signal-safe and touches no memory of the parent.
    unsafe {
        cmd.pre_exec(|| set_pdeathsig(Signal::SIGINT).map_err(std::io::Error::from));
    }
    let mut child = cmd.spawn()?;
    if let Some(stdout) = child.stdout.take() {
        let tx = tx.clone();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(|l| l.ok()) {
                match parse_saved_line(&line) {
                    Some(path) => send(&tx, RecorderEvent::Saved { path, duration_ms: None }),
                    None => eprintln!("gsr: {line}"),
                }
            }
        });
    }
    if let Some(stderr) = child.stderr.take() {
        thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(|l| l.ok()) {
                // gsr reports its capture rate every second; that's noise, not news.
                if !line.contains("gsr info: update fps") {
                    eprintln!("gsr: {line}");
                }
            }
        });
    }
    Ok(child)
}

/// Waits for the child to exit, retrying a quickly failing KMS capture once with the portal,
/// then reports `Exited`.
fn wait_loop(
    shared: Arc<Shared>,
    mut target: Target,
    retry_args: Vec<String>,
    program: PathBuf,
    tx: Option<Sender<RecorderEvent>>,
) {
    let mut spawned_at = Instant::now();
    loop {
        thread::sleep(POLL);
        let mut slot = lock(&shared.child);
        let Some(child) = slot.as_mut() else { return };
        let code = match child.try_wait() {
            Ok(None) => continue,
            Ok(Some(status)) => status.code(),
            Err(e) => {
                eprintln!("klipt: can't check on gpu-screen-recorder: {e}");
                None
            }
        };
        let quick_failure = spawned_at.elapsed() < KMS_FALLBACK_WINDOW && code != Some(0);
        if matches!(target, Target::Monitor(_)) && quick_failure && !shared.stop_requested.load(Ordering::SeqCst) {
            eprintln!("klipt: gpu-screen-recorder KMS capture exited with {code:?}; retrying with the desktop portal");
            match spawn_child(&program, &retry_args, &tx) {
                Ok(new_child) => {
                    *slot = Some(new_child);
                    target = Target::Portal;
                    spawned_at = Instant::now();
                    continue;
                }
                Err(e) => eprintln!("klipt: can't start gpu-screen-recorder with the portal: {e}"),
            }
        }
        *slot = None;
        drop(slot);
        send(&tx, RecorderEvent::Exited { code });
        return;
    }
}

impl RecorderBackend for GsrBackend {
    fn start(&mut self, cfg: &RecordConfig) -> Result<()> {
        if self.is_running() {
            return Err(AppError::RecorderFailed("gpu-screen-recorder is already running".into()));
        }
        if let Some(old) = self.run.take() {
            let _ = old.waiter.join();
        }
        std::fs::create_dir_all(&cfg.output_dir)?;
        let target = match &cfg.display_id {
            Some(id) => Target::Monitor(id.clone()),
            None => match self.list_displays() {
                Ok(displays) => automatic_monitor(&displays).map_or(Target::Portal, |id| Target::Monitor(id.into())),
                Err(e) => {
                    eprintln!("klipt: can't list monitors ({e}); using the desktop portal");
                    Target::Portal
                }
            },
        };
        let tx = self.sink.current();
        let child = spawn_child(&self.program, &build_args(cfg, &target, &self.portal_token), &tx)
            .map_err(|e| spawn_error(&self.program, e))?;
        let shared = Arc::new(Shared { child: Mutex::new(Some(child)), stop_requested: AtomicBool::new(false) });
        let retry_args = build_args(cfg, &Target::Portal, &self.portal_token);
        let waiter = {
            let (shared, program, tx) = (shared.clone(), self.program.clone(), tx.clone());
            thread::spawn(move || wait_loop(shared, target, retry_args, program, tx))
        };
        self.run = Some(Run { shared, waiter });
        send(&tx, RecorderEvent::Started { encoder: "h264".into() });
        Ok(())
    }

    fn save(&mut self) -> Result<()> {
        let run = self.run.as_ref().ok_or(AppError::NotRecording)?;
        let slot = lock(&run.shared.child);
        let child = slot.as_ref().ok_or(AppError::NotRecording)?;
        kill(Pid::from_raw(child.id() as i32), Signal::SIGUSR1)
            .map_err(|e| AppError::RecorderFailed(format!("can't signal gpu-screen-recorder: {e}")))
    }

    fn stop(&mut self) -> Result<()> {
        let Some(run) = self.run.take() else { return Ok(()) };
        run.shared.stop_requested.store(true, Ordering::SeqCst);
        if let Some(child) = lock(&run.shared.child).as_mut() {
            if let Ok(None) = child.try_wait() {
                if let Err(e) = kill(Pid::from_raw(child.id() as i32), Signal::SIGINT) {
                    eprintln!("klipt: can't stop gpu-screen-recorder: {e}");
                }
            }
        }
        let deadline = Instant::now() + STOP_TIMEOUT;
        while lock(&run.shared.child).is_some() && Instant::now() < deadline {
            thread::sleep(POLL / 2);
        }
        if let Some(child) = lock(&run.shared.child).as_mut() {
            eprintln!("klipt: gpu-screen-recorder didn't stop within {STOP_TIMEOUT:?}; killing it");
            let _ = child.kill();
        }
        let _ = run.waiter.join();
        Ok(())
    }

    fn events(&self) -> Receiver<RecorderEvent> {
        self.sink.subscribe()
    }

    fn list_displays(&mut self) -> Result<Vec<Display>> {
        let output = Command::new(&self.program)
            .arg("--list-monitors")
            .stdin(Stdio::null())
            .output()
            .map_err(|e| spawn_error(&self.program, e))?;
        let mut displays = parse_monitors(&String::from_utf8_lossy(&output.stdout));
        if displays.len() > 1 {
            mark_primary(&mut displays, kscreen_primary().as_deref());
        }
        Ok(displays)
    }

    fn list_audio_devices(&mut self) -> Result<Vec<AudioDevice>> {
        let output = Command::new(&self.program)
            .arg("--list-audio-devices")
            .stdin(Stdio::null())
            .output()
            .map_err(|e| spawn_error(&self.program, e))?;
        Ok(parse_audio_devices(&String::from_utf8_lossy(&output.stdout)))
    }
}

impl Drop for GsrBackend {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> RecordConfig {
        RecordConfig {
            display_id: Some("DP-1".into()),
            fps: 60,
            size: None,
            bitrate_kbps: 20_000,
            buffer_seconds: 30,
            mic: true,
            mic_device_id: None,
            output_dir: "/home/u/Videos/Klipt".into(),
        }
    }

    fn strings(s: &str) -> Vec<String> {
        s.split(' ').map(String::from).collect()
    }

    #[test]
    fn kms_with_mic_matches_the_decision_record() {
        let args = build_args(&cfg(), &Target::Monitor("DP-1".into()), Path::new("/data/portal-token"));
        assert_eq!(
            args,
            strings(
                "-w DP-1 -f 60 -k h264 -bm cbr -q 20000 -ac aac -a default_output -a default_input \
                 -c mp4 -r 30 -replay-storage ram -o /home/u/Videos/Klipt"
            )
        );
    }

    #[test]
    fn portal_without_mic_and_a_size_box() {
        let c = RecordConfig { mic: false, size: Some((1280, 720)), fps: 30, buffer_seconds: 120, ..cfg() };
        let args = build_args(&c, &Target::Portal, Path::new("/data/portal-token"));
        assert_eq!(
            args,
            strings(
                "-w portal -restore-portal-session yes -portal-session-token-filepath /data/portal-token \
                 -f 30 -s 1280x720 -k h264 -bm cbr -q 20000 -ac aac -a default_output \
                 -c mp4 -r 120 -replay-storage ram -o /home/u/Videos/Klipt"
            )
        );
    }

    #[test]
    fn custom_bitrate_and_a_chosen_mic() {
        let c = RecordConfig { bitrate_kbps: 35_000, mic_device_id: Some("alsa_input.usb-mic".into()), ..cfg() };
        let args = build_args(&c, &Target::Monitor("HDMI-A-1".into()), Path::new("/t"));
        assert_eq!(
            args,
            strings(
                "-w HDMI-A-1 -f 60 -k h264 -bm cbr -q 35000 -ac aac -a default_output -a alsa_input.usb-mic \
                 -c mp4 -r 30 -replay-storage ram -o /home/u/Videos/Klipt"
            )
        );
    }

    #[test]
    fn parses_list_monitors() {
        let displays = parse_monitors("HDMI-A-1|1920x1080\nDP-1|2560x1440\n\ngarbage line\nX|axb\n");
        assert_eq!(
            displays,
            vec![
                Display { id: "HDMI-A-1".into(), name: "HDMI-A-1".into(), width: 1920, height: 1080, primary: false },
                Display { id: "DP-1".into(), name: "DP-1".into(), width: 2560, height: 1440, primary: false },
            ]
        );
    }

    #[test]
    fn parses_list_audio_devices_keeping_only_microphones() {
        let text = "default_output|Default output\n\
default_input|Default input\n\
alsa_output.usb-HyperX-00.analog-stereo.monitor|Monitor of HyperX Analog Stereo\n\
alsa_input.usb-HyperX-00.mono-fallback|HyperX Cloud Stinger Core Wireless DTS Mono\n\
alsa_output.pci-0000_16_00.6.iec958-stereo.monitor|Monitor of Ryzen HD Audio Controller Digital Stereo (IEC958)\n\
alsa_input.pci-0000_16_00.6.analog-stereo|Ryzen HD Audio Controller Analog Stereo\n\
\n\
garbage\n\
|No id\n";
        assert_eq!(
            parse_audio_devices(text),
            vec![
                AudioDevice {
                    id: "alsa_input.usb-HyperX-00.mono-fallback".into(),
                    name: "HyperX Cloud Stinger Core Wireless DTS Mono".into()
                },
                AudioDevice {
                    id: "alsa_input.pci-0000_16_00.6.analog-stereo".into(),
                    name: "Ryzen HD Audio Controller Analog Stereo".into()
                },
            ]
        );
        assert_eq!(parse_audio_devices("weird|"), vec![AudioDevice { id: "weird".into(), name: "weird".into() }]);
    }

    #[test]
    fn kscreen_primary_is_the_enabled_output_with_priority_one() {
        let json = r#"{"outputs":[
            {"name":"HDMI-A-1","priority":2,"enabled":true,"connected":true},
            {"name":"DP-1","priority":1,"enabled":true,"connected":true},
            {"name":"DP-2","priority":0,"enabled":false,"connected":false}
        ]}"#;
        assert_eq!(parse_kscreen_primary(json).as_deref(), Some("DP-1"));
        let disabled = r#"{"outputs":[{"name":"DP-1","priority":1,"enabled":false,"connected":true}]}"#;
        assert_eq!(parse_kscreen_primary(disabled), None);
        assert_eq!(parse_kscreen_primary("not json"), None);
        assert_eq!(parse_kscreen_primary(r#"{"outputs":[]}"#), None);
    }

    #[test]
    fn the_primary_monitor_is_marked_and_preferred() {
        let mut displays = parse_monitors("HDMI-A-1|1920x1080\nDP-1|1920x1080\n");
        mark_primary(&mut displays, Some("DP-1"));
        assert_eq!(displays.iter().filter(|d| d.primary).map(|d| d.id.as_str()).collect::<Vec<_>>(), ["DP-1"]);
        assert_eq!(automatic_monitor(&displays), Some("DP-1"));
        // Unknown primary: the first line, as before.
        let mut displays = parse_monitors("HDMI-A-1|1920x1080\nDP-1|1920x1080\n");
        mark_primary(&mut displays, Some("DP-9"));
        assert_eq!(automatic_monitor(&displays), Some("HDMI-A-1"));
        mark_primary(&mut displays, None);
        assert_eq!(automatic_monitor(&displays), Some("HDMI-A-1"));
        assert_eq!(automatic_monitor(&[]), None);
    }

    #[test]
    fn saved_line_must_be_an_existing_mp4_path() {
        let dir = tempfile::tempdir().unwrap();
        let clip = dir.path().join("Replay_2026-11-10_12-00-00.mp4");
        std::fs::write(&clip, b"x").unwrap();
        let other = dir.path().join("notes.txt");
        std::fs::write(&other, b"x").unwrap();

        assert_eq!(parse_saved_line(&format!("{}\n", clip.display())), Some(clip.clone()));
        assert_eq!(parse_saved_line("gsr info: replay saved"), None);
        assert_eq!(parse_saved_line(""), None);
        assert_eq!(parse_saved_line(&other.display().to_string()), None);
        assert_eq!(parse_saved_line(&dir.path().join("missing.mp4").display().to_string()), None);
        assert_eq!(parse_saved_line("relative/clip.mp4"), None);
    }

    /// A stand-in for gsr: saves on SIGUSR1 (prints the path), exits 0 on SIGINT. With
    /// `fail_kms`, anything but `-w portal` exits 1 at once (like KMS capture without permission).
    fn fake_gsr(dir: &Path, fail_kms: bool) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let script = dir.join("fake-gsr.sh");
        let kms = if fail_kms { r#"[ "$2" = portal ] || exit 1"# } else { "" };
        std::fs::write(
            &script,
            format!(
                r#"#!/bin/sh
{kms}
for a; do out=$a; done
echo "$@" > "$out/args"
trap 'f="$out/clip.mp4"; : > "$f"; echo "$f"' USR1
trap 'exit 0' INT
while :; do sleep 0.05; done
"#
            ),
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        script
    }

    fn next(rx: &Receiver<RecorderEvent>) -> RecorderEvent {
        rx.recv_timeout(Duration::from_secs(10)).expect("an event from the fake gsr")
    }

    #[test]
    fn real_child_saves_on_sigusr1_and_stops_on_sigint() {
        let dir = tempfile::tempdir().unwrap();
        let mut backend = GsrBackend::with_program(fake_gsr(dir.path(), false), dir.path());
        let rx = backend.events();
        let c = RecordConfig { output_dir: dir.path().join("out"), ..cfg() };
        backend.start(&c).unwrap();
        assert_eq!(next(&rx), RecorderEvent::Started { encoder: "h264".into() });
        thread::sleep(Duration::from_millis(300)); // let the script install its traps
        backend.save().unwrap();
        let clip = c.output_dir.join("clip.mp4");
        assert_eq!(next(&rx), RecorderEvent::Saved { path: clip, duration_ms: None });
        backend.stop().unwrap();
        assert_eq!(next(&rx), RecorderEvent::Exited { code: Some(0) });
        assert!(matches!(backend.save(), Err(AppError::NotRecording)));
    }

    #[test]
    fn quick_kms_failure_retries_once_with_the_portal() {
        let dir = tempfile::tempdir().unwrap();
        let mut backend = GsrBackend::with_program(fake_gsr(dir.path(), true), dir.path());
        let rx = backend.events();
        let c = RecordConfig { output_dir: dir.path().join("out"), ..cfg() };
        backend.start(&c).unwrap();
        assert_eq!(next(&rx), RecorderEvent::Started { encoder: "h264".into() });
        let args_file = c.output_dir.join("args");
        let deadline = Instant::now() + Duration::from_secs(10);
        while !std::fs::read_to_string(&args_file).unwrap_or_default().starts_with("-w portal") {
            assert!(Instant::now() < deadline, "no portal retry");
            thread::sleep(Duration::from_millis(20));
        }
        let args = std::fs::read_to_string(&args_file).unwrap();
        assert!(args.contains(&format!("-portal-session-token-filepath {}", dir.path().join("portal-token").display())));
        backend.stop().unwrap();
        assert_eq!(next(&rx), RecorderEvent::Exited { code: Some(0) });
    }
}
