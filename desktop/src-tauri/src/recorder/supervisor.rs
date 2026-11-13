//! Keeps the recorder backend running: applies the recording mode, restarts a crashed helper
//! with backoff, debounces saves and runs the save pipeline.
//!
//! [`Supervisor`] is a synchronous state machine driven by [`Input`]s, so tests can drive it with
//! a fake backend and a fake clock. [`SupervisorHandle::spawn`] runs it on its own thread.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use super::{AudioDevice, Display, RecordConfig, RecorderBackend, RecorderEvent};
use crate::errors::{AppError, Result};
use crate::games::Detected;
use crate::library::ClipView;
use crate::settings::RecordingMode;

/// Delays before restart 1, 2 and 3 after an unexpected exit. After that it gives up.
pub const RESTART_BACKOFF: [Duration; 3] = [Duration::from_secs(1), Duration::from_secs(3), Duration::from_secs(10)];
/// A helper that ran this long before crashing gets a fresh set of restarts.
pub const STABLE_RUN: Duration = Duration::from_secs(60);
/// How often "record only while a game runs" checks for a game.
pub const GAME_POLL_INTERVAL: Duration = Duration::from_secs(3);
/// A save that hasn't produced a file by then is given up on, so saving isn't blocked forever.
pub const SAVE_TIMEOUT: Duration = Duration::from_secs(60);

/// Backend error codes that mean the recording never started.
const START_FAILURE_CODES: &[&str] = &["start_failed", "no_encoder", "no_display", "obs_load_failed"];

/// Monotonic time since some fixed start.
pub trait Clock: Send {
    fn now(&self) -> Duration;
}

pub struct SystemClock(Instant);

impl SystemClock {
    pub fn new() -> SystemClock {
        SystemClock(Instant::now())
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        SystemClock::new()
    }
}

impl Clock for SystemClock {
    fn now(&self) -> Duration {
        self.0.elapsed()
    }
}

/// A file the backend saved, with what the app knew when the save was requested.
#[derive(Debug, Clone, PartialEq)]
pub struct SavedClip {
    pub path: PathBuf,
    pub duration_ms: Option<u64>,
    /// The game detected when the save was requested.
    pub game: Detected,
    pub has_mic: bool,
    /// Voice chat was recorded on its own track.
    pub has_voice: bool,
}

/// What the supervisor tells the app (the Tauri sink turns these into events for the UI).
#[derive(Debug, Clone, PartialEq)]
pub enum SupervisorEvent {
    Recording { encoder: String },
    ClipSaved(ClipView),
    /// A save was requested while one was still running: the UI shows "Already saving…".
    SaveBusy,
    /// `message` is friendly English for the user; `code` is for the UI to branch on.
    Error { code: String, message: String },
}

/// Everything the supervisor needs from the outside world.
pub struct Deps {
    pub clock: Box<dyn Clock>,
    pub game_running: Box<dyn FnMut() -> bool + Send>,
    pub detect_game: Box<dyn FnMut() -> Detected + Send>,
    pub process_clip: Box<dyn FnMut(SavedClip) -> Result<ClipView> + Send>,
    pub sink: Box<dyn FnMut(SupervisorEvent) + Send>,
}

pub enum Input {
    Save,
    /// New settings: stop, then apply them.
    Configure(RecordConfig, RecordingMode),
    Shutdown,
    /// Stop recording until `Resume`. The stop isn't a crash and nothing restarts meanwhile.
    Pause,
    /// Undo `Pause`: apply the recording mode afresh.
    Resume,
    /// An event from the helper started as run `generation`. Events of older runs are ignored.
    Backend { generation: u64, event: RecorderEvent },
    /// A deadline may have passed.
    Tick,
    /// Lists displays through the backend (for Settings); the answer goes to the sender.
    ListDisplays(Sender<Result<Vec<Display>>>),
    /// Lists microphones through the backend (for Settings).
    ListAudioDevices(Sender<Result<Vec<AudioDevice>>>),
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum State {
    Idle,
    Running { since: Duration },
    RestartPending { at: Duration },
    /// Gave up after too many crashes; waits for new settings (or, while a game runs, the game ending).
    Failed,
}

struct PendingSave {
    game: Detected,
    deadline: Duration,
}

pub struct Supervisor {
    backend: Box<dyn RecorderBackend + Send>,
    deps: Deps,
    cfg: RecordConfig,
    mode: RecordingMode,
    state: State,
    restarts: usize,
    generation: u64,
    pending_save: Option<PendingSave>,
    next_poll: Option<Duration>,
    /// Paused by the user: nothing starts until resumed.
    paused: bool,
    /// The user was told the recorder program is missing. In game mode every game start tries
    /// again, and the warning shouldn't repeat each time; new settings or a resume reset it.
    missing_reported: bool,
    /// Where backend events are forwarded to (None in tests that feed events by hand).
    inputs: Option<Sender<Input>>,
}

pub fn friendly_message(code: &str) -> String {
    match code {
        "recorder_crashed" => {
            "The recorder stopped unexpectedly and couldn't be restarted. Check your recording settings or restart Klipt."
        }
        "save_failed" => "Klipt couldn't save that clip.",
        "not_recording" => "Klipt isn't recording right now.",
        "no_encoder" => "Klipt couldn't find a video encoder it can use on this PC.",
        "no_display" => "Klipt can't capture the selected display. Pick another one in Settings.",
        "recording_stopped" => "Recording stopped — restarting…",
        "recording_paused" => "Recording is paused. Resume it from the tray icon to save clips.",
        _ => "Something went wrong with recording.",
    }
    .into()
}

impl Supervisor {
    pub fn new(backend: Box<dyn RecorderBackend + Send>, cfg: RecordConfig, mode: RecordingMode, deps: Deps) -> Self {
        Supervisor {
            backend,
            deps,
            cfg,
            mode,
            state: State::Idle,
            restarts: 0,
            generation: 0,
            pending_save: None,
            next_poll: None,
            paused: false,
            missing_reported: false,
            inputs: None,
        }
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn is_recording(&self) -> bool {
        matches!(self.state, State::Running { .. })
    }

    /// Applies the recording mode: starts now (Always) or checks for a game (WhileGameRunning).
    /// Does nothing while paused.
    pub fn begin(&mut self) {
        if self.paused {
            return;
        }
        match self.mode {
            RecordingMode::Always => {
                self.next_poll = None;
                self.start_backend();
            }
            RecordingMode::WhileGameRunning => self.poll_game(),
        }
    }

    /// Handles one input. Returns false once the supervisor has shut down.
    pub fn handle(&mut self, input: Input) -> bool {
        match input {
            Input::Save => self.on_save(),
            Input::Configure(cfg, mode) => {
                self.stop_backend();
                self.cfg = cfg;
                self.mode = mode;
                self.state = State::Idle;
                self.restarts = 0;
                self.missing_reported = false;
                self.begin();
            }
            Input::Shutdown => {
                self.stop_backend();
                return false;
            }
            Input::Backend { generation, event } => {
                if generation == self.generation {
                    self.on_backend_event(event);
                }
            }
            Input::Tick => self.on_tick(),
            Input::ListDisplays(reply) => {
                let _ = reply.send(self.backend.list_displays());
            }
            Input::ListAudioDevices(reply) => {
                let _ = reply.send(self.backend.list_audio_devices());
            }
            Input::Pause => {
                if !self.paused {
                    self.paused = true;
                    self.stop_backend();
                    self.state = State::Idle;
                    self.next_poll = None;
                }
            }
            Input::Resume => {
                if self.paused {
                    self.paused = false;
                    self.state = State::Idle;
                    self.restarts = 0;
                    self.missing_reported = false;
                    self.begin();
                }
            }
        }
        true
    }

    /// The earliest time a `Tick` has something to do.
    pub fn next_deadline(&self) -> Option<Duration> {
        let restart = match self.state {
            State::RestartPending { at } => Some(at),
            _ => None,
        };
        [restart, self.next_poll, self.pending_save.as_ref().map(|p| p.deadline)].into_iter().flatten().min()
    }

    pub fn now(&self) -> Duration {
        self.deps.clock.now()
    }

    fn emit(&mut self, event: SupervisorEvent) {
        (self.deps.sink)(event);
    }

    fn emit_error(&mut self, code: &str) {
        self.emit(SupervisorEvent::Error { code: code.into(), message: friendly_message(code) });
    }

    fn start_backend(&mut self) {
        // A new generation: events of earlier helpers are ignored from now on.
        self.generation += 1;
        let events = self.backend.events();
        if let Some(inputs) = &self.inputs {
            let (inputs, generation) = (inputs.clone(), self.generation);
            thread::spawn(move || {
                for event in events {
                    if inputs.send(Input::Backend { generation, event }).is_err() {
                        return;
                    }
                }
            });
        }
        match self.backend.start(&self.cfg) {
            Ok(()) => self.state = State::Running { since: self.now() },
            Err(e) => {
                eprintln!("klipt: can't start recording: {e}");
                if matches!(e, AppError::RecorderMissing(_)) {
                    // Retrying can't bring back a missing program: that error is the final word,
                    // said once (e.g. "install GPU Screen Recorder").
                    if !self.missing_reported {
                        self.missing_reported = true;
                        self.emit(SupervisorEvent::Error { code: e.code().into(), message: e.user_message() });
                    }
                    self.fail_pending_save();
                    self.state = State::Failed;
                } else {
                    // Tell the user why, together with what happens next.
                    self.on_failure(Some((e.code().into(), e.user_message())));
                }
            }
        }
    }

    /// A requested stop: whatever the helper reports afterwards isn't a crash.
    fn stop_backend(&mut self) {
        if matches!(self.state, State::Running { .. }) {
            self.generation += 1;
            if let Err(e) = self.backend.stop() {
                eprintln!("klipt: can't stop recording: {e}");
            }
        }
        if !matches!(self.state, State::Failed) {
            self.state = State::Idle;
        }
        self.fail_pending_save();
    }

    /// The helper died or never started: schedule a restart, or give up after the last one.
    /// The user gets exactly one message per failure; `cause` (code, friendly message) says why
    /// when it's known, and is folded into that message.
    fn on_failure(&mut self, cause: Option<(String, String)>) {
        self.fail_pending_save();
        match RESTART_BACKOFF.get(self.restarts) {
            Some(delay) => {
                self.restarts += 1;
                eprintln!("klipt: restarting the recorder in {delay:?} (attempt {})", self.restarts);
                self.state = State::RestartPending { at: self.now() + *delay };
                match cause {
                    Some((code, message)) => {
                        self.emit(SupervisorEvent::Error { code, message: format!("{message} Retrying…") })
                    }
                    None => self.emit_error("recording_stopped"),
                }
            }
            None => {
                eprintln!("klipt: the recorder keeps failing; giving up");
                self.state = State::Failed;
                let message = match cause {
                    Some((_, message)) => format!(
                        "{message} Klipt couldn't restart recording. Check your recording settings or restart Klipt."
                    ),
                    None => friendly_message("recorder_crashed"),
                };
                self.emit(SupervisorEvent::Error { code: "recorder_crashed".into(), message });
            }
        }
    }

    fn fail_pending_save(&mut self) {
        if self.pending_save.take().is_some() {
            self.emit_error("save_failed");
        }
    }

    fn poll_game(&mut self) {
        self.next_poll = Some(self.now() + GAME_POLL_INTERVAL);
        let running = (self.deps.game_running)();
        match (running, self.state) {
            (true, State::Idle) => {
                self.restarts = 0;
                self.start_backend();
            }
            (false, State::Running { .. } | State::RestartPending { .. }) => self.stop_backend(),
            // The game ended after the recorder gave up: try afresh with the next game.
            (false, State::Failed) => self.state = State::Idle,
            _ => {}
        }
    }

    fn on_tick(&mut self) {
        let now = self.now();
        if self.pending_save.as_ref().is_some_and(|p| p.deadline <= now) {
            eprintln!("klipt: the save didn't produce a file within {SAVE_TIMEOUT:?}");
            self.fail_pending_save();
        }
        if let State::RestartPending { at } = self.state {
            if at <= now {
                self.start_backend();
            }
        }
        if self.mode == RecordingMode::WhileGameRunning && self.next_poll.is_some_and(|t| t <= now) {
            self.poll_game();
        }
    }

    fn on_save(&mut self) {
        if self.paused {
            self.emit_error("recording_paused");
            return;
        }
        if !matches!(self.state, State::Running { .. }) {
            self.emit_error("not_recording");
            return;
        }
        if self.pending_save.is_some() {
            self.emit(SupervisorEvent::SaveBusy);
            return;
        }
        // The game is captured now: it may close before the file is written.
        let game = (self.deps.detect_game)();
        match self.backend.save() {
            Ok(()) => self.pending_save = Some(PendingSave { game, deadline: self.now() + SAVE_TIMEOUT }),
            Err(AppError::RecorderStarting) => {
                let e = AppError::RecorderStarting;
                self.emit(SupervisorEvent::Error { code: e.code().into(), message: e.user_message() });
            }
            Err(e) => {
                eprintln!("klipt: can't save: {e}");
                self.emit_error("save_failed");
            }
        }
    }

    fn on_backend_event(&mut self, event: RecorderEvent) {
        match event {
            RecorderEvent::Started { encoder } => self.emit(SupervisorEvent::Recording { encoder }),
            RecorderEvent::Saved { path, duration_ms } => {
                let game = match self.pending_save.take() {
                    Some(pending) => pending.game,
                    None => (self.deps.detect_game)(),
                };
                let saved = SavedClip { path, duration_ms, game, has_mic: self.cfg.mic, has_voice: self.cfg.voice_track() };
                match (self.deps.process_clip)(saved) {
                    Ok(view) => self.emit(SupervisorEvent::ClipSaved(view)),
                    Err(e) => {
                        eprintln!("klipt: can't add the saved clip to the library: {e}");
                        self.emit_error("save_failed");
                    }
                }
            }
            RecorderEvent::Busy => self.emit(SupervisorEvent::SaveBusy),
            RecorderEvent::Error { code, message } => {
                eprintln!("klipt: recorder error {code}: {message}");
                if code == "save_failed" {
                    self.fail_pending_save();
                } else if code == "recording_stopped" {
                    // klipt-recorder exits right after this; the `Exited` that follows brings the
                    // one "restarting" notice, so this one isn't shown twice.
                } else if START_FAILURE_CODES.contains(&code.as_str()) && self.is_recording() {
                    self.generation += 1;
                    if let Err(e) = self.backend.stop() {
                        eprintln!("klipt: can't stop recording: {e}");
                    }
                    let message = friendly_message(&code);
                    self.on_failure(Some((code, message)));
                } else if code != "not_recording" && code != "already_recording" {
                    self.emit_error(&code);
                }
            }
            RecorderEvent::Exited { code } => {
                let State::Running { since } = self.state else { return };
                eprintln!("klipt: the recorder exited unexpectedly ({code:?})");
                if self.now().saturating_sub(since) >= STABLE_RUN {
                    self.restarts = 0;
                }
                self.on_failure(None);
            }
        }
    }
}

/// Runs a [`Supervisor`] on its own thread.
pub struct SupervisorHandle {
    tx: Sender<Input>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl SupervisorHandle {
    pub fn spawn(
        backend: Box<dyn RecorderBackend + Send>,
        cfg: RecordConfig,
        mode: RecordingMode,
        deps: Deps,
    ) -> SupervisorHandle {
        let (tx, rx) = mpsc::channel();
        let mut supervisor = Supervisor::new(backend, cfg, mode, deps);
        supervisor.inputs = Some(tx.clone());
        let thread = thread::Builder::new()
            .name("klipt-recorder-supervisor".into())
            .spawn(move || run(supervisor, rx))
            .expect("can't start the recorder supervisor thread");
        SupervisorHandle { tx, thread: Mutex::new(Some(thread)) }
    }

    pub fn save(&self) {
        let _ = self.tx.send(Input::Save);
    }

    /// New settings: stops the recorder and starts it again with them (clearing the buffer).
    pub fn configure(&self, cfg: RecordConfig, mode: RecordingMode) {
        let _ = self.tx.send(Input::Configure(cfg, mode));
    }

    /// The displays the backend can record. Blocks up to `timeout` (the supervisor may be busy
    /// starting the helper).
    pub fn list_displays(&self, timeout: Duration) -> Result<Vec<Display>> {
        self.ask(Input::ListDisplays, timeout)
    }

    /// The microphones the backend can record. Blocks up to `timeout`.
    pub fn list_audio_devices(&self, timeout: Duration) -> Result<Vec<AudioDevice>> {
        self.ask(Input::ListAudioDevices, timeout)
    }

    fn ask<T>(&self, input: impl FnOnce(Sender<Result<T>>) -> Input, timeout: Duration) -> Result<T> {
        let (tx, rx) = mpsc::channel();
        self.tx.send(input(tx)).map_err(|_| AppError::RecorderFailed("the recorder supervisor stopped".into()))?;
        rx.recv_timeout(timeout).unwrap_or_else(|_| Err(AppError::RecorderFailed("the recorder didn't answer in time".into())))
    }

    pub fn pause(&self) {
        let _ = self.tx.send(Input::Pause);
    }

    pub fn resume(&self) {
        let _ = self.tx.send(Input::Resume);
    }

    /// Stops the helper and waits for the supervisor thread to finish. Safe to call twice.
    pub fn shutdown(&self) {
        let _ = self.tx.send(Input::Shutdown);
        if let Some(thread) = self.thread.lock().unwrap_or_else(PoisonError::into_inner).take() {
            let _ = thread.join();
        }
    }
}

fn run(mut supervisor: Supervisor, rx: Receiver<Input>) {
    supervisor.begin();
    loop {
        let input = match supervisor.next_deadline() {
            Some(deadline) => match rx.recv_timeout(deadline.saturating_sub(supervisor.now())) {
                Ok(input) => input,
                Err(RecvTimeoutError::Timeout) => Input::Tick,
                Err(RecvTimeoutError::Disconnected) => Input::Shutdown,
            },
            None => rx.recv().unwrap_or(Input::Shutdown),
        };
        if !supervisor.handle(input) {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::Clip;
    use crate::recorder::Display;
    use std::sync::Arc;

    #[derive(Default)]
    struct Calls {
        starts: usize,
        saves: usize,
        stops: usize,
        /// Sender of the newest `events()` receiver, so a test can play the helper.
        events: Option<Sender<RecorderEvent>>,
        /// When set, `start` fails with this error.
        start_error: Option<fn() -> AppError>,
    }

    struct FakeBackend(Arc<Mutex<Calls>>);

    impl RecorderBackend for FakeBackend {
        fn start(&mut self, _cfg: &RecordConfig) -> Result<()> {
            let mut calls = self.0.lock().unwrap();
            calls.starts += 1;
            calls.start_error.map_or(Ok(()), |e| Err(e()))
        }
        fn save(&mut self) -> Result<()> {
            self.0.lock().unwrap().saves += 1;
            Ok(())
        }
        fn stop(&mut self) -> Result<()> {
            self.0.lock().unwrap().stops += 1;
            Ok(())
        }
        fn events(&self) -> Receiver<RecorderEvent> {
            let (tx, rx) = mpsc::channel();
            self.0.lock().unwrap().events = Some(tx);
            rx
        }
        fn list_displays(&mut self) -> Result<Vec<Display>> {
            Ok(vec![Display { id: "d1".into(), name: "D1".into(), width: 1920, height: 1080, primary: true, x: 0, y: 0 }])
        }
        fn list_audio_devices(&mut self) -> Result<Vec<AudioDevice>> {
            Ok(vec![AudioDevice { id: "m1".into(), name: "Mic".into() }])
        }
    }

    #[derive(Clone, Default)]
    struct FakeClock(Arc<Mutex<Duration>>);

    impl FakeClock {
        fn advance(&self, by: Duration) {
            *self.0.lock().unwrap() += by;
        }
    }

    impl Clock for FakeClock {
        fn now(&self) -> Duration {
            *self.0.lock().unwrap()
        }
    }

    struct World {
        calls: Arc<Mutex<Calls>>,
        clock: FakeClock,
        game_running: Arc<Mutex<bool>>,
        game: Arc<Mutex<Detected>>,
        processed: Arc<Mutex<Vec<SavedClip>>>,
        events: Arc<Mutex<Vec<SupervisorEvent>>>,
    }

    impl World {
        fn new() -> World {
            World {
                calls: Default::default(),
                clock: FakeClock::default(),
                game_running: Default::default(),
                game: Default::default(),
                processed: Default::default(),
                events: Default::default(),
            }
        }

        fn deps(&self) -> Deps {
            let (running, game, processed, events) =
                (self.game_running.clone(), self.game.clone(), self.processed.clone(), self.events.clone());
            Deps {
                clock: Box::new(self.clock.clone()),
                game_running: Box::new(move || *running.lock().unwrap()),
                detect_game: Box::new(move || game.lock().unwrap().clone()),
                process_clip: Box::new(move |saved: SavedClip| {
                    processed.lock().unwrap().push(saved.clone());
                    Ok(fake_view(&saved))
                }),
                sink: Box::new(move |e| events.lock().unwrap().push(e)),
            }
        }

        fn supervisor(&self, mode: RecordingMode) -> Supervisor {
            Supervisor::new(Box::new(FakeBackend(self.calls.clone())), cfg(), mode, self.deps())
        }

        fn starts(&self) -> usize {
            self.calls.lock().unwrap().starts
        }
        fn saves(&self) -> usize {
            self.calls.lock().unwrap().saves
        }
        fn stops(&self) -> usize {
            self.calls.lock().unwrap().stops
        }
        fn events(&self) -> Vec<SupervisorEvent> {
            self.events.lock().unwrap().clone()
        }
        fn error_codes(&self) -> Vec<String> {
            self.events()
                .into_iter()
                .filter_map(|e| match e {
                    SupervisorEvent::Error { code, .. } => Some(code),
                    _ => None,
                })
                .collect()
        }
    }

    fn cfg() -> RecordConfig {
        RecordConfig {
            display_id: None,
            fps: 60,
            size: Some((1920, 1080)),
            bitrate_kbps: 20_000,
            buffer_seconds: 30,
            mic: true,
            mic_device_id: None,
            output_dir: "/clips".into(),
            voice_apps: Vec::new(),
        }
    }

    fn fake_view(saved: &SavedClip) -> ClipView {
        ClipView {
            clip: Clip {
                id: "c1".into(),
                path: saved.path.clone(),
                created_at: 0,
                duration_ms: saved.duration_ms.unwrap_or(0),
                width: 1920,
                height: 1080,
                has_mic: saved.has_mic,
                game_name: saved.game.game_name.clone(),
                steam_app_id: saved.game.steam_app_id,
                exe: saved.game.exe.clone(),
                title: "t".into(),
                trim_start_ms: None,
                trim_end_ms: None,
                game_volume: 1.0,
                mic_volume: 1.0,
                server_clip_id: None,
                share_url: None,
                visibility: None,
                upload_id: None,
                upload_offset: None,
                favorite: false,
                thumbnail_ms: None,
                has_voice: false,
                voice_volume: 1.0,
                master_volume: 1.0,
                kind: crate::library::ClipKind::Clip,
                extra_volumes: Default::default(),
            },
            file_missing: false,
            thumb_path: None,
        }
    }

    fn backend(s: &mut Supervisor, event: RecorderEvent) {
        let generation = s.generation();
        assert!(s.handle(Input::Backend { generation, event }));
    }

    fn saved(path: &str) -> RecorderEvent {
        RecorderEvent::Saved { path: path.into(), duration_ms: Some(30_000) }
    }

    fn game(name: &str) -> Detected {
        Detected { game_name: Some(name.into()), steam_app_id: Some(1), exe: Some("g.exe".into()) }
    }

    #[test]
    fn always_mode_starts_right_away() {
        let w = World::new();
        let mut s = w.supervisor(RecordingMode::Always);
        s.begin();
        assert_eq!(w.starts(), 1);
        assert!(s.is_recording());
        backend(&mut s, RecorderEvent::Started { encoder: "h264".into() });
        assert_eq!(w.events(), vec![SupervisorEvent::Recording { encoder: "h264".into() }]);
    }

    #[test]
    fn double_save_gives_busy_until_the_first_one_lands() {
        let w = World::new();
        let mut s = w.supervisor(RecordingMode::Always);
        s.begin();
        assert!(s.handle(Input::Save));
        assert!(s.handle(Input::Save));
        assert_eq!(w.saves(), 1, "the second save must not reach the backend");
        assert_eq!(w.events(), vec![SupervisorEvent::SaveBusy]);

        backend(&mut s, saved("/clips/a.mp4"));
        assert_eq!(w.processed.lock().unwrap().len(), 1);
        assert!(matches!(w.events().last(), Some(SupervisorEvent::ClipSaved(v)) if v.clip.path == PathBuf::from("/clips/a.mp4")));

        assert!(s.handle(Input::Save));
        assert_eq!(w.saves(), 2, "saving works again after the first save landed");
    }

    #[test]
    fn a_failed_save_unblocks_saving() {
        let w = World::new();
        let mut s = w.supervisor(RecordingMode::Always);
        s.begin();
        s.handle(Input::Save);
        backend(&mut s, RecorderEvent::Error { code: "save_failed".into(), message: "mux failed".into() });
        assert_eq!(w.error_codes(), ["save_failed"]);
        s.handle(Input::Save);
        assert_eq!(w.saves(), 2);
    }

    #[test]
    fn a_save_that_never_lands_times_out() {
        let w = World::new();
        let mut s = w.supervisor(RecordingMode::Always);
        s.begin();
        s.handle(Input::Save);
        w.clock.advance(SAVE_TIMEOUT);
        s.handle(Input::Tick);
        s.handle(Input::Save);
        assert_eq!(w.saves(), 2);
        assert_eq!(w.error_codes(), ["save_failed"]);
    }

    #[test]
    fn backend_busy_is_reported_as_busy() {
        let w = World::new();
        let mut s = w.supervisor(RecordingMode::Always);
        s.begin();
        backend(&mut s, RecorderEvent::Busy);
        assert_eq!(w.events(), vec![SupervisorEvent::SaveBusy]);
    }

    #[test]
    fn save_while_not_recording_is_an_error_not_a_backend_call() {
        let w = World::new();
        let mut s = w.supervisor(RecordingMode::WhileGameRunning);
        s.begin();
        s.handle(Input::Save);
        assert_eq!(w.saves(), 0);
        assert_eq!(w.error_codes(), ["not_recording"]);
        match &w.events()[0] {
            SupervisorEvent::Error { message, .. } => assert!(!message.contains("not_recording"), "{message}"),
            e => panic!("{e:?}"),
        }
    }

    #[test]
    fn game_is_captured_when_the_save_is_requested() {
        let w = World::new();
        let mut s = w.supervisor(RecordingMode::Always);
        s.begin();
        *w.game.lock().unwrap() = game("Active Matter");
        s.handle(Input::Save);
        // The game closes before the file is muxed.
        *w.game.lock().unwrap() = Detected::default();
        backend(&mut s, saved("/clips/a.mp4"));
        let processed = w.processed.lock().unwrap();
        assert_eq!(processed[0].game.game_name.as_deref(), Some("Active Matter"));
        assert!(processed[0].has_mic);
        assert_eq!(processed[0].duration_ms, Some(30_000));
    }

    #[test]
    fn a_failing_pipeline_reports_save_failed() {
        let w = World::new();
        let mut deps = w.deps();
        deps.process_clip = Box::new(|_| Err(AppError::ToolsMissing));
        let mut s = Supervisor::new(Box::new(FakeBackend(w.calls.clone())), cfg(), RecordingMode::Always, deps);
        s.begin();
        s.handle(Input::Save);
        backend(&mut s, saved("/clips/a.mp4"));
        assert_eq!(w.error_codes(), ["save_failed"]);
        s.handle(Input::Save);
        assert_eq!(w.saves(), 2);
    }

    #[test]
    fn unexpected_exits_restart_three_times_with_backoff_then_report_a_crash() {
        let w = World::new();
        let mut s = w.supervisor(RecordingMode::Always);
        s.begin();
        assert_eq!(w.starts(), 1);
        for (i, delay) in RESTART_BACKOFF.iter().enumerate() {
            backend(&mut s, RecorderEvent::Exited { code: Some(1) });
            assert!(!s.is_recording());
            assert_eq!(s.next_deadline(), Some(w.clock.now() + *delay), "restart {}", i + 1);
            w.clock.advance(*delay - Duration::from_millis(1));
            s.handle(Input::Tick);
            assert_eq!(w.starts(), i + 1, "no restart before the backoff is over");
            w.clock.advance(Duration::from_millis(1));
            s.handle(Input::Tick);
            assert_eq!(w.starts(), i + 2, "restart {} after {delay:?}", i + 1);
            assert_eq!(w.error_codes(), vec!["recording_stopped"; i + 1], "one notice per restart");
        }
        backend(&mut s, RecorderEvent::Exited { code: Some(1) });
        let mut expected = vec!["recording_stopped"; 3];
        expected.push("recorder_crashed");
        assert_eq!(w.error_codes(), expected);
        match &w.events()[0] {
            SupervisorEvent::Error { message, .. } => assert_eq!(message, "Recording stopped — restarting…"),
            e => panic!("{e:?}"),
        }
        assert_eq!(s.next_deadline(), None);
        w.clock.advance(Duration::from_secs(60));
        s.handle(Input::Tick);
        assert_eq!(w.starts(), 4, "gave up after 3 restarts");
    }

    #[test]
    fn a_long_run_earns_a_fresh_set_of_restarts() {
        let w = World::new();
        let mut s = w.supervisor(RecordingMode::Always);
        s.begin();
        for delay in RESTART_BACKOFF {
            w.clock.advance(STABLE_RUN);
            backend(&mut s, RecorderEvent::Exited { code: Some(1) });
            w.clock.advance(delay);
            s.handle(Input::Tick);
        }
        w.clock.advance(STABLE_RUN);
        backend(&mut s, RecorderEvent::Exited { code: Some(1) });
        assert_eq!(s.next_deadline(), Some(w.clock.now() + RESTART_BACKOFF[0]));
        assert_eq!(w.error_codes(), vec!["recording_stopped"; 4], "never gave up");
    }

    #[test]
    fn a_start_failure_event_counts_as_a_crash() {
        let w = World::new();
        let mut s = w.supervisor(RecordingMode::Always);
        s.begin();
        backend(&mut s, RecorderEvent::Error { code: "start_failed".into(), message: "x".into() });
        assert_eq!(w.stops(), 1, "the half-started helper is stopped");
        assert_eq!(w.error_codes(), ["start_failed"], "one message");
        w.clock.advance(RESTART_BACKOFF[0]);
        s.handle(Input::Tick);
        assert_eq!(w.starts(), 2);
    }

    #[test]
    fn helper_recording_stopped_then_exit_gives_exactly_one_notice() {
        let w = World::new();
        let mut s = w.supervisor(RecordingMode::Always);
        s.begin();
        // klipt-recorder reports the stop itself, then exits with code 2.
        backend(&mut s, RecorderEvent::Error { code: "recording_stopped".into(), message: "encoder error".into() });
        backend(&mut s, RecorderEvent::Exited { code: Some(2) });
        assert_eq!(w.error_codes(), ["recording_stopped"]);
        assert_eq!(s.next_deadline(), Some(w.clock.now() + RESTART_BACKOFF[0]));
    }

    #[test]
    fn a_start_error_is_shown_and_retried() {
        let w = World::new();
        w.calls.lock().unwrap().start_error = Some(|| AppError::RecorderFailed("spawn failed".into()));
        let mut s = w.supervisor(RecordingMode::Always);
        s.begin();
        assert_eq!(w.error_codes(), ["recorder_failed"], "one message, not cause + \"restarting\"");
        match &w.events()[0] {
            SupervisorEvent::Error { message, .. } => {
                let cause = AppError::RecorderFailed(String::new()).user_message();
                assert_eq!(message, &format!("{cause} Retrying…"));
            }
            e => panic!("{e:?}"),
        }
        w.clock.advance(RESTART_BACKOFF[0]);
        s.handle(Input::Tick);
        assert_eq!(w.starts(), 2);
    }

    #[test]
    fn a_missing_recorder_is_reported_once_without_retries() {
        let w = World::new();
        w.calls.lock().unwrap().start_error = Some(|| AppError::RecorderMissing("gpu-screen-recorder".into()));
        let mut s = w.supervisor(RecordingMode::Always);
        s.begin();
        assert_eq!(w.error_codes(), ["recorder_missing"]);
        match &w.events()[0] {
            SupervisorEvent::Error { message, .. } => {
                assert_eq!(message, &AppError::RecorderMissing(String::new()).user_message());
                assert!(!message.contains("gpu-screen-recorder"), "no raw detail: {message}");
            }
            e => panic!("{e:?}"),
        }
        assert_eq!(s.next_deadline(), None);
        w.clock.advance(Duration::from_secs(60));
        s.handle(Input::Tick);
        assert_eq!(w.starts(), 1, "no retries");
        assert_eq!(w.error_codes(), ["recorder_missing"]);
    }

    #[test]
    fn start_errors_that_exhaust_the_retries_end_in_one_crash_message() {
        let w = World::new();
        w.calls.lock().unwrap().start_error = Some(|| AppError::RecorderFailed("spawn failed".into()));
        let mut s = w.supervisor(RecordingMode::Always);
        s.begin();
        for delay in RESTART_BACKOFF {
            w.clock.advance(delay);
            s.handle(Input::Tick);
        }
        let codes = w.error_codes();
        assert_eq!(codes.len(), RESTART_BACKOFF.len() + 1, "{codes:?}");
        assert_eq!(codes.last().map(String::as_str), Some("recorder_crashed"));
        assert_eq!(s.next_deadline(), None);
    }

    #[test]
    fn a_missing_recorder_is_not_reported_again_at_every_game_start() {
        let w = World::new();
        w.calls.lock().unwrap().start_error = Some(|| AppError::RecorderMissing("gpu-screen-recorder".into()));
        let mut s = w.supervisor(RecordingMode::WhileGameRunning);
        s.begin();
        for running in [true, false, true, false, true] {
            *w.game_running.lock().unwrap() = running;
            w.clock.advance(GAME_POLL_INTERVAL);
            s.handle(Input::Tick);
        }
        assert_eq!(w.starts(), 3, "each game start still tries (the program may be installed by now)");
        assert_eq!(w.error_codes(), ["recorder_missing"]);

        // New settings are a fresh attempt the user asked for: say it again if it still fails.
        s.handle(Input::Configure(cfg(), RecordingMode::WhileGameRunning));
        assert_eq!(w.error_codes(), ["recorder_missing", "recorder_missing"]);
    }

    #[test]
    fn events_from_an_older_run_are_ignored() {
        let w = World::new();
        let mut s = w.supervisor(RecordingMode::Always);
        s.begin();
        let old = s.generation();
        s.handle(Input::Configure(cfg(), RecordingMode::Always));
        assert_eq!((w.stops(), w.starts()), (1, 2));
        assert!(s.handle(Input::Backend { generation: old, event: RecorderEvent::Exited { code: Some(0) } }));
        assert!(s.is_recording());
        assert_eq!(s.next_deadline(), None);
    }

    #[test]
    fn exit_after_a_requested_stop_is_not_a_crash() {
        let w = World::new();
        let mut s = w.supervisor(RecordingMode::WhileGameRunning);
        *w.game_running.lock().unwrap() = true;
        s.begin();
        *w.game_running.lock().unwrap() = false;
        w.clock.advance(GAME_POLL_INTERVAL);
        s.handle(Input::Tick);
        backend(&mut s, RecorderEvent::Exited { code: Some(0) });
        w.clock.advance(Duration::from_secs(30));
        s.handle(Input::Tick);
        assert_eq!(w.starts(), 1);
        assert!(w.error_codes().is_empty());
    }

    #[test]
    fn while_game_running_follows_the_detector() {
        let w = World::new();
        let mut s = w.supervisor(RecordingMode::WhileGameRunning);
        s.begin();
        assert_eq!(w.starts(), 0, "no game, no recording");
        assert_eq!(s.next_deadline(), Some(GAME_POLL_INTERVAL));

        *w.game_running.lock().unwrap() = true;
        w.clock.advance(GAME_POLL_INTERVAL);
        s.handle(Input::Tick);
        assert_eq!(w.starts(), 1, "starts when the game appears");
        assert!(s.is_recording());

        w.clock.advance(GAME_POLL_INTERVAL);
        s.handle(Input::Tick);
        assert_eq!((w.starts(), w.stops()), (1, 0), "keeps recording while the game runs");

        *w.game_running.lock().unwrap() = false;
        w.clock.advance(GAME_POLL_INTERVAL);
        s.handle(Input::Tick);
        assert_eq!(w.stops(), 1, "stops when the game is gone");
        assert!(!s.is_recording());

        *w.game_running.lock().unwrap() = true;
        w.clock.advance(GAME_POLL_INTERVAL);
        s.handle(Input::Tick);
        assert_eq!(w.starts(), 2, "starts again for the next game");
    }

    #[test]
    fn always_mode_does_not_poll_for_games() {
        let w = World::new();
        let mut s = w.supervisor(RecordingMode::Always);
        s.begin();
        assert_eq!(s.next_deadline(), None);
    }

    #[test]
    fn pause_stops_without_a_crash_and_resume_starts_again() {
        let w = World::new();
        let mut s = w.supervisor(RecordingMode::Always);
        s.begin();
        assert!(s.handle(Input::Pause));
        assert_eq!(w.stops(), 1);
        assert!(!s.is_recording());
        assert!(s.is_paused());
        // The stopped helper's exit isn't a crash, and nothing restarts while paused.
        backend(&mut s, RecorderEvent::Exited { code: Some(0) });
        assert_eq!(s.next_deadline(), None);
        w.clock.advance(Duration::from_secs(60));
        s.handle(Input::Tick);
        assert_eq!(w.starts(), 1);
        assert!(w.error_codes().is_empty());
        // Saving while paused tells the user instead of calling the backend.
        s.handle(Input::Save);
        assert_eq!(w.saves(), 0);
        assert_eq!(w.error_codes(), ["recording_paused"]);
        // New settings while paused are kept for later, without starting.
        s.handle(Input::Configure(cfg(), RecordingMode::Always));
        assert_eq!(w.starts(), 1);
        assert!(s.handle(Input::Resume));
        assert_eq!(w.starts(), 2);
        assert!(s.is_recording() && !s.is_paused());
    }

    #[test]
    fn pause_during_a_pending_restart_cancels_it_and_resume_resets_the_backoff() {
        let w = World::new();
        let mut s = w.supervisor(RecordingMode::Always);
        s.begin();
        backend(&mut s, RecorderEvent::Exited { code: Some(1) });
        s.handle(Input::Pause);
        assert_eq!(s.next_deadline(), None);
        w.clock.advance(RESTART_BACKOFF[0]);
        s.handle(Input::Tick);
        assert_eq!(w.starts(), 1);
        s.handle(Input::Resume);
        assert_eq!(w.starts(), 2);
        backend(&mut s, RecorderEvent::Exited { code: Some(1) });
        assert_eq!(s.next_deadline(), Some(w.clock.now() + RESTART_BACKOFF[0]), "fresh restarts after resume");
    }

    #[test]
    fn pause_in_game_mode_stops_polling_until_resumed() {
        let w = World::new();
        let mut s = w.supervisor(RecordingMode::WhileGameRunning);
        *w.game_running.lock().unwrap() = true;
        s.begin();
        assert_eq!(w.starts(), 1);
        s.handle(Input::Pause);
        assert_eq!(s.next_deadline(), None);
        w.clock.advance(GAME_POLL_INTERVAL);
        s.handle(Input::Tick);
        assert_eq!(w.starts(), 1);
        s.handle(Input::Resume);
        assert_eq!(w.starts(), 2, "the game still runs, so it records again");
        assert_eq!(s.next_deadline(), Some(w.clock.now() + GAME_POLL_INTERVAL));
    }

    #[test]
    fn shutdown_stops_the_backend() {
        let w = World::new();
        let mut s = w.supervisor(RecordingMode::Always);
        s.begin();
        assert!(!s.handle(Input::Shutdown));
        assert_eq!(w.stops(), 1);
    }

    #[test]
    fn handle_thread_forwards_backend_events_and_saves() {
        let w = World::new();
        let (tx, rx) = mpsc::channel();
        let mut deps = w.deps();
        deps.clock = Box::new(SystemClock::new());
        deps.sink = Box::new(move |e| {
            let _ = tx.send(e);
        });
        let handle = SupervisorHandle::spawn(
            Box::new(FakeBackend(w.calls.clone())),
            cfg(),
            RecordingMode::Always,
            deps,
        );
        handle.save();
        // The save reached the backend once the backend has a subscriber and a save call.
        let deadline = Instant::now() + Duration::from_secs(5);
        while w.saves() == 0 && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        let helper = w.calls.lock().unwrap().events.clone().expect("subscribed before start");
        helper.send(saved("/clips/a.mp4")).unwrap();
        let event = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(matches!(event, SupervisorEvent::ClipSaved(_)), "{event:?}");
        handle.shutdown();
        handle.shutdown();
        assert_eq!(w.stops(), 1);
    }

    #[test]
    fn handle_lists_through_the_backend_and_configure_restarts() {
        let w = World::new();
        let mut deps = w.deps();
        deps.clock = Box::new(SystemClock::new());
        let handle =
            SupervisorHandle::spawn(Box::new(FakeBackend(w.calls.clone())), cfg(), RecordingMode::Always, deps);
        let timeout = Duration::from_secs(5);
        assert_eq!(handle.list_displays(timeout).unwrap()[0].id, "d1");
        assert_eq!(handle.list_audio_devices(timeout).unwrap()[0].id, "m1");
        assert_eq!(w.starts(), 1);
        handle.configure(RecordConfig { fps: 30, ..cfg() }, RecordingMode::Always);
        // Listing after `configure` is answered after it was handled: inputs are in order.
        handle.list_displays(timeout).unwrap();
        assert_eq!((w.stops(), w.starts()), (1, 2), "stopped and started again with the new settings");
        handle.shutdown();
        assert!(handle.list_displays(timeout).is_err(), "a stopped supervisor answers with an error");
    }
}
