//! klipt-recorder: Klipt's recording helper process, built on libobs.
//!
//! The app starts it with piped stdio and speaks the JSON-lines protocol in `protocol.rs`.
//! `--selftest` records without the app, for testing on real hardware.

/// Writes a line to stderr, ignoring errors. `eprintln!` panics when stderr is a closed pipe (the
/// app died), and a panic inside libobs's log callback would abort the process mid-save.
macro_rules! log {
    ($($arg:tt)*) => {{
        use std::io::Write as _;
        let _ = writeln!(std::io::stderr().lock(), $($arg)*);
    }};
}

mod displays;
mod hotkey_win;
mod obs;
mod protocol;
mod selftest;
mod session;

use protocol::{parse_command, AudioDevice, Command, ErrorCode, Event};
use session::{Session, Signal};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const USAGE: &str = "\
klipt-recorder — Klipt's libobs recording helper

USAGE:
  klipt-recorder [--obs-root DIR]              JSON-lines protocol on stdin/stdout (used by the app)
  klipt-recorder --list [--obs-root DIR]       print displays, audio devices and encoders, then exit
  klipt-recorder --selftest [OPTIONS]          record without the app and print events + stats

SELFTEST OPTIONS:
  --seconds N         replay buffer length in seconds (default 30)
  --display ID        display id from --list (default: the primary display)
  --mic               also record the default microphone as audio track 2
  --mic-device ID     microphone id from --list (implies --mic)
  --out DIR           clips folder (default: <Videos>/Klipt-test)
  --save-at N[,M...]  save automatically N seconds after recording starts
  --exit-after N      stop N seconds after recording starts and print stats
  --hotkey KEYS       Windows: save when KEYS is pressed, e.g. ALT+F8, or ALT+BACKQUOTE for the key under Esc
  --fps N             frames per second (default 60)
  --width N           maximum output width (default 1920)
  --height N          maximum output height (default 1080)
  --bitrate KBPS      video bitrate (default 20000)
  --encoder ID        libobs encoder id or 'auto' (default auto)
  While running, type 's' + Enter to save and 'q' + Enter to stop.

ENVIRONMENT:
  KLIPT_RECORDER_DEBUG=1   include libobs debug messages on stderr
";

/// Everything the main loop reacts to.
pub enum Input {
    Line(String),
    StdinClosed,
    Obs(Signal),
    Hotkey,
}

// ---------------------------------------------------------------------------------------------
// Event output

static EVENTS: OnceLock<Mutex<Box<dyn Write + Send>>> = OnceLock::new();

/// Sets up where events go. On Linux, child processes started by libobs (obs-ffmpeg-mux) inherit
/// fd 1 and may print to it, so events get a private duplicate of stdout and fd 1 is pointed at
/// stderr. (On Windows libobs starts its helpers without a stdout handle.)
fn init_event_output() {
    #[cfg(target_os = "linux")]
    let out: Box<dyn Write + Send> = {
        use std::os::fd::FromRawFd;
        extern "C" {
            fn fcntl(fd: i32, cmd: i32, ...) -> i32;
            fn dup2(old: i32, new: i32) -> i32;
        }
        const F_DUPFD_CLOEXEC: i32 = 1030;
        unsafe {
            let private = fcntl(1, F_DUPFD_CLOEXEC, 3);
            if private >= 0 && dup2(2, 1) >= 0 {
                Box::new(std::fs::File::from_raw_fd(private))
            } else {
                Box::new(std::io::stdout())
            }
        }
    };
    #[cfg(not(target_os = "linux"))]
    let out: Box<dyn Write + Send> = Box::new(std::io::stdout());
    let _ = EVENTS.set(Mutex::new(out));
}

/// Writes one event per line and flushes, so the app sees it immediately. Write errors (the app
/// went away) are ignored; stdin EOF shuts the process down.
pub fn emit(event: &Event) {
    let line = event.to_line();
    match EVENTS.get() {
        Some(out) => {
            let mut out = out.lock().unwrap_or_else(|e| e.into_inner());
            let _ = writeln!(out, "{line}");
            let _ = out.flush();
        }
        None => {
            let mut out = std::io::stdout().lock();
            let _ = writeln!(out, "{line}");
            let _ = out.flush();
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Process helpers

static LAUNCH_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Resolves a path relative to the directory the recorder was started in. The working directory
/// changes to OBS's bin folder during startup (libobs needs that on Windows).
pub fn resolve_path(p: &str) -> PathBuf {
    let base = LAUNCH_DIR.get().cloned().unwrap_or_default();
    base.join(Path::new(p))
}

/// Guarantees the process exits with `code` even if libobs cleanup hangs (releasing an output can
/// wait forever on libobs's stop event).
pub fn arm_exit_watchdog(code: i32) {
    static ARMED: OnceLock<()> = OnceLock::new();
    if ARMED.set(()).is_ok() {
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(10));
            log!("cleanup did not finish within 10 s; exiting");
            std::process::exit(code);
        });
    }
}

pub enum Flow {
    Continue,
    Exit(i32),
}

// ---------------------------------------------------------------------------------------------
// Controller

/// Applies commands and libobs signals to the (at most one) recording session.
pub struct Controller {
    pub tx: Sender<Input>,
    pub rx: Receiver<Input>,
    registered_encoders: Vec<String>,
    pub session: Option<Session>,
}

impl Controller {
    pub fn command(&mut self, cmd: Command) -> Flow {
        match cmd {
            Command::ListDisplays => emit(&Event::Displays { items: displays::list() }),
            Command::ListAudioDevices => emit(&audio_devices()),
            Command::Start(params) => {
                if self.session.is_some() {
                    emit(&Event::error(ErrorCode::AlreadyRecording, "Already recording"));
                    return Flow::Continue;
                }
                let tx = self.tx.clone();
                let notify = Box::new(move |s: Signal| {
                    let _ = tx.send(Input::Obs(s));
                });
                match Session::start(&params, &self.registered_encoders, notify) {
                    Ok(s) => {
                        emit(&Event::Started { encoder: s.encoder.clone(), width: s.width, height: s.height });
                        self.session = Some(s);
                    }
                    Err(e) => emit(&e.to_event()),
                }
            }
            Command::Save => self.save(),
            Command::Stop => {
                self.end_session();
                emit(&Event::Stopped);
                self.release_session(0);
                return Flow::Exit(0);
            }
            Command::Shutdown => {
                self.end_session();
                self.release_session(0);
                return Flow::Exit(0);
            }
        }
        Flow::Continue
    }

    pub fn save(&mut self) {
        match &mut self.session {
            None => emit(&Event::error(ErrorCode::NotRecording, "Recording hasn't started")),
            Some(s) => {
                if let Err(e) = s.save() {
                    emit(&e);
                }
            }
        }
    }

    pub fn signal(&mut self, signal: Signal) -> Flow {
        match signal {
            Signal::Saved => {
                if let Some(event) = self.session.as_mut().and_then(Session::on_saved) {
                    emit(&event);
                }
            }
            Signal::OutputStopped { code } => {
                if self.session.as_ref().is_some_and(|s| !s.is_stopping()) {
                    emit(&Event::error(
                        ErrorCode::RecordingStopped,
                        format!("Recording stopped unexpectedly (libobs output code {code})"),
                    ));
                    self.release_session(2);
                    return Flow::Exit(2);
                }
            }
        }
        Flow::Continue
    }

    pub fn tick(&mut self) {
        if let Some(event) = self.session.as_mut().and_then(Session::poll) {
            emit(&event);
        }
    }

    /// Lets a save that is being written finish, emitting `saved` or `save_failed`. Other input
    /// arriving meanwhile is dropped (this only runs on the way out).
    pub fn wait_for_pending_save(&mut self) {
        let deadline = Instant::now() + session::SAVE_TIMEOUT;
        while self.session.as_ref().is_some_and(|s| s.is_saving()) && Instant::now() < deadline {
            if let Ok(Input::Obs(signal)) = self.rx.recv_timeout(Duration::from_millis(50)) {
                self.signal(signal);
            }
            obs::pump_events();
            self.tick();
        }
    }

    /// Finishes a pending save, then stops the replay buffer. Without the wait, a stop right after
    /// a save would lose the clip's `saved` event.
    fn end_session(&mut self) {
        self.wait_for_pending_save();
        if let Some(s) = self.session.as_mut() {
            s.stop();
        }
    }

    /// Releases libobs objects; a watchdog makes sure a hang there can't keep the process alive.
    fn release_session(&mut self, exit_code: i32) {
        arm_exit_watchdog(exit_code);
        self.session = None;
    }
}

fn audio_devices() -> Event {
    let list = |source_id: &str| -> Vec<AudioDevice> {
        obs::list_property_items(source_id, "device_id")
            .into_iter()
            .map(|(name, id)| AudioDevice { id, name })
            .collect()
    };
    Event::AudioDevices { outputs: list(session::DESKTOP_AUDIO_SOURCE), inputs: list(session::MIC_SOURCE) }
}

/// How often the main loop wakes up without input. Linux needs to iterate GLib often for the portal.
#[cfg(target_os = "linux")]
pub const TICK: Duration = Duration::from_millis(10);
#[cfg(not(target_os = "linux"))]
pub const TICK: Duration = Duration::from_millis(100);

fn run_protocol(ctl: &mut Controller) -> i32 {
    loop {
        let input = ctl.rx.recv_timeout(TICK);
        match input {
            Ok(Input::Line(line)) if line.trim().is_empty() => {}
            Ok(Input::Line(line)) => match parse_command(&line) {
                Ok(cmd) => {
                    if let Flow::Exit(code) = ctl.command(cmd) {
                        return code;
                    }
                }
                Err(e) => emit(&e.to_event()),
            },
            // The app went away: an orphaned helper must not keep recording.
            Ok(Input::StdinClosed) | Err(RecvTimeoutError::Disconnected) => {
                ctl.command(Command::Shutdown);
                return 0;
            }
            Ok(Input::Obs(signal)) => {
                if let Flow::Exit(code) = ctl.signal(signal) {
                    return code;
                }
            }
            Ok(Input::Hotkey) => ctl.save(),
            Err(RecvTimeoutError::Timeout) => {}
        }
        obs::pump_events();
        ctl.tick();
    }
}

// ---------------------------------------------------------------------------------------------
// Startup

struct Args {
    obs_root: Option<PathBuf>,
    mode: Mode,
}

enum Mode {
    Protocol,
    List,
    Selftest(selftest::SelftestArgs),
}

fn parse_args(raw: &[String]) -> Result<Option<Args>, String> {
    if raw.iter().any(|a| a == "--help" || a == "-h") {
        print!("{USAGE}");
        return Ok(None);
    }
    if raw.iter().any(|a| a == "--version" || a == "-V") {
        println!("klipt-recorder {}", env!("CARGO_PKG_VERSION"));
        return Ok(None);
    }
    let mut obs_root = None;
    let mut rest = vec![];
    let mut it = raw.iter();
    while let Some(a) = it.next() {
        if a == "--obs-root" {
            let dir = it.next().ok_or("--obs-root needs a directory")?;
            obs_root = Some(std::path::absolute(dir).map_err(|e| format!("--obs-root {dir}: {e}"))?);
        } else {
            rest.push(a.clone());
        }
    }
    let mode = if rest.iter().any(|a| a == "--selftest") {
        Mode::Selftest(selftest::SelftestArgs::parse(&rest)?)
    } else if rest.iter().any(|a| a == "--list") {
        Mode::List
    } else if let Some(unknown) = rest.first() {
        return Err(format!("unknown argument {unknown} (see --help)"));
    } else {
        Mode::Protocol
    };
    Ok(Some(Args { obs_root, mode }))
}

fn main() {
    let _ = LAUNCH_DIR.set(std::env::current_dir().unwrap_or_default());
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let args = match parse_args(&raw) {
        Ok(Some(a)) => a,
        Ok(None) => return,
        Err(e) => {
            log!("klipt-recorder: {e}");
            std::process::exit(64);
        }
    };
    init_event_output();

    let code = run(args);
    arm_exit_watchdog(code);
    obs::shutdown();
    std::process::exit(code);
}

fn run(args: Args) -> i32 {
    let info = match obs::ObsLayout::detect(args.obs_root.as_deref()).and_then(|layout| obs::init(&layout)) {
        Ok(info) => info,
        Err(e) => {
            emit(&Event::error(ErrorCode::ObsLoadFailed, format!("{e:#}")));
            return 1;
        }
    };
    log!("libobs {} loaded; H.264 encoders: {}", info.version, info.encoders.join(", "));
    emit(&Event::Ready {
        version: env!("CARGO_PKG_VERSION").to_string(),
        platform: if cfg!(windows) { "windows" } else { "linux" }.to_string(),
        encoders: info.encoders,
    });

    let (tx, rx) = mpsc::channel();
    let stdin_tx = tx.clone();
    std::thread::Builder::new()
        .name("stdin".into())
        .spawn(move || {
            // Bytes, not `lines()`: invalid UTF-8 must become a bad_command, not look like EOF.
            let mut stdin = std::io::stdin().lock();
            let mut buf = Vec::new();
            loop {
                buf.clear();
                match stdin.read_until(b'\n', &mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        let line = String::from_utf8_lossy(&buf).into_owned();
                        if stdin_tx.send(Input::Line(line)).is_err() {
                            return;
                        }
                    }
                }
            }
            let _ = stdin_tx.send(Input::StdinClosed);
        })
        .expect("spawn stdin thread");

    let mut ctl = Controller { tx, rx, registered_encoders: obs::registered_encoders(), session: None };
    match args.mode {
        Mode::Protocol => run_protocol(&mut ctl),
        Mode::List => {
            ctl.command(Command::ListDisplays);
            ctl.command(Command::ListAudioDevices);
            0
        }
        Mode::Selftest(st) => selftest::run(&mut ctl, st),
    }
}
