//! klipt-recorder: Klipt's recording helper process, built on libobs.
//!
//! The app starts it with piped stdio and speaks the JSON-lines protocol in `protocol.rs`.
//! `--selftest` records without the app, for testing on real hardware.

mod displays;
mod hotkey_win;
mod obs;
mod protocol;
mod selftest;
mod session;

use protocol::{parse_command, AudioDevice, Command, ErrorCode, Event};
use session::{Session, Signal};
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

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
  --hotkey KEYS       Windows: save when KEYS is pressed, e.g. ALT+F9
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

/// Writes one event per line to stdout and flushes, so the app sees it immediately.
pub fn emit(event: &Event) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{}", event.to_line());
    let _ = out.flush();
}

pub enum Flow {
    Continue,
    Exit(i32),
}

/// Applies commands and libobs signals to the (at most one) recording session.
pub struct Controller {
    tx: Sender<Input>,
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
                if let Some(s) = self.session.take() {
                    s.stop();
                }
                emit(&Event::Stopped);
                return Flow::Exit(0);
            }
            Command::Shutdown => {
                if let Some(s) = self.session.take() {
                    s.stop();
                }
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
                    self.session = None;
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
const TICK: Duration = Duration::from_millis(10);
#[cfg(not(target_os = "linux"))]
const TICK: Duration = Duration::from_millis(100);

fn run_protocol(ctl: &mut Controller, rx: &Receiver<Input>) -> i32 {
    loop {
        match rx.recv_timeout(TICK) {
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
            obs_root = Some(PathBuf::from(it.next().ok_or("--obs-root needs a directory")?));
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
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let args = match parse_args(&raw) {
        Ok(Some(a)) => a,
        Ok(None) => return,
        Err(e) => {
            eprintln!("klipt-recorder: {e}");
            std::process::exit(64);
        }
    };

    let code = run(args);
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
    eprintln!("libobs {} loaded; H.264 encoders: {}", info.version, info.encoders.join(", "));
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
            for line in std::io::stdin().lock().lines() {
                match line {
                    Ok(l) => {
                        if stdin_tx.send(Input::Line(l)).is_err() {
                            return;
                        }
                    }
                    Err(_) => break,
                }
            }
            let _ = stdin_tx.send(Input::StdinClosed);
        })
        .expect("spawn stdin thread");

    let mut ctl = Controller { tx: tx.clone(), registered_encoders: obs::registered_encoders(), session: None };
    match args.mode {
        Mode::Protocol => run_protocol(&mut ctl, &rx),
        Mode::List => {
            ctl.command(Command::ListDisplays);
            ctl.command(Command::ListAudioDevices);
            0
        }
        Mode::Selftest(st) => selftest::run(&mut ctl, &tx, &rx, st),
    }
}
