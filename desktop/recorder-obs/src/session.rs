//! One recording session: display + system audio (track 1) + mic (track 2) → H.264 + AAC →
//! libobs `replay_buffer`. `save` asks the replay buffer to write its last N seconds to disk.

use crate::obs::{self, api, cs, from_c, Data};
use crate::protocol::{ErrorCode, Event, StartParams};
use std::ffi::c_void;
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------------------------
// Pure logic (unit-tested)

/// Tracks whether a save is in flight, so a second hotkey press can't start an overlapping save.
#[derive(Debug)]
pub struct SaveTracker {
    timeout: Duration,
    pending: Option<PendingSave>,
}

#[derive(Debug, Clone, Copy)]
struct PendingSave {
    since: Instant,
    duration_ms: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Busy;

impl SaveTracker {
    pub fn new(timeout: Duration) -> Self {
        Self { timeout, pending: None }
    }

    /// Marks a save as started. `duration_ms` is the clip length to report when it finishes.
    pub fn begin(&mut self, now: Instant, duration_ms: u64) -> Result<(), Busy> {
        if self.pending.is_some() {
            return Err(Busy);
        }
        self.pending = Some(PendingSave { since: now, duration_ms });
        Ok(())
    }

    /// The replay buffer reported a written file. Returns the clip length if a save was pending.
    pub fn finish(&mut self) -> Option<u64> {
        self.pending.take().map(|p| p.duration_ms)
    }

    /// Gives up on a save that never completed. Returns true once, when the timeout passes.
    pub fn check_timeout(&mut self, now: Instant) -> bool {
        match self.pending {
            Some(p) if now.duration_since(p.since) >= self.timeout => {
                self.pending = None;
                true
            }
            _ => false,
        }
    }

    pub fn is_saving(&self) -> bool {
        self.pending.is_some()
    }
}

/// The replay buffer holds at most `buffer_seconds`, and less right after start.
pub fn clip_duration_ms(recording_for: Duration, buffer_seconds: u32) -> u64 {
    (recording_for.as_millis() as u64).min(buffer_seconds as u64 * 1000)
}

pub const FILENAME_FORMAT: &str = "Klipt %CCYY-%MM-%DD %hh-%mm-%ss";

/// Names clips so that quick saves never collide. libobs names the file when the save starts, to
/// the second, and overwrites an existing file. Saves less than 2 s after the previous one get
/// " (2)", " (3)", … (2 s also covers a save requested at :59.99 but named in the next second).
#[derive(Debug, Default)]
pub struct ClipNamer {
    last: Option<Instant>,
    burst: u32,
}

impl ClipNamer {
    pub fn next_format(&mut self, now: Instant) -> String {
        self.burst = match self.last {
            Some(prev) if now.duration_since(prev) < Duration::from_secs(2) => self.burst + 1,
            _ => 0,
        };
        self.last = Some(now);
        if self.burst == 0 {
            FILENAME_FORMAT.to_string()
        } else {
            format!("{FILENAME_FORMAT} ({})", self.burst + 1)
        }
    }
}

/// Detects a save that failed while writing. libobs only signals `saved` on success; on failure
/// muxing just ends. `get_last_replay` returns "" while muxing and the new path once it ended, so a
/// new path without a `saved` signal shortly after means the save failed.
#[derive(Debug)]
pub struct MuxWatch {
    path_before: String,
    ended_at: Option<Instant>,
}

impl MuxWatch {
    pub const GRACE: Duration = Duration::from_secs(1);

    pub fn new(path_before: String) -> Self {
        Self { path_before, ended_at: None }
    }

    /// `current` is what `get_last_replay` returns now. True when the save should be reported failed.
    pub fn failed(&mut self, current: &str, now: Instant) -> bool {
        if current.is_empty() || current == self.path_before {
            self.ended_at = None; // still muxing, or the save hasn't started yet
            return false;
        }
        let ended = *self.ended_at.get_or_insert(now);
        now.duration_since(ended) >= Self::GRACE
    }
}

/// Scales `src` to fit inside `max`, keeping the aspect ratio, never upscaling,
/// with even dimensions (required by NV12/H.264).
pub fn fit_inside(src_w: u32, src_h: u32, max_w: u32, max_h: u32) -> (u32, u32) {
    let even = |v: f64| (((v / 2.0).round() as u32) * 2).max(2);
    if src_w == 0 || src_h == 0 {
        return (even(max_w as f64), even(max_h as f64));
    }
    let scale = (max_w as f64 / src_w as f64).min(max_h as f64 / src_h as f64).min(1.0);
    (even(src_w as f64 * scale), even(src_h as f64 * scale))
}

/// Encoder ids to try, in order. `auto` means every known H.264 encoder that is registered.
pub fn encoder_candidates(requested: &str, registered: &[String]) -> Vec<String> {
    if requested == "auto" {
        obs::H264_ENCODERS.iter().filter(|id| registered.iter().any(|r| r == *id)).map(|s| s.to_string()).collect()
    } else {
        vec![requested.to_string()]
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Setting {
    Str(&'static str),
    Int(i64),
    Bool(bool),
}

/// CBR at `bitrate_kbps`, one keyframe per second, no B-frames, plus per-encoder speed settings.
/// Keys are the ones OBS 33's encoders read (unknown keys would be silently ignored, so the
/// encoder's startup log is the way to confirm them).
pub fn video_encoder_settings(encoder_id: &str, bitrate_kbps: u32) -> Vec<(&'static str, Setting)> {
    let mut s = vec![
        ("rate_control", Setting::Str("CBR")),
        ("bitrate", Setting::Int(bitrate_kbps as i64)),
        ("keyint_sec", Setting::Int(1)),
        ("profile", Setting::Str("high")),
        ("bf", Setting::Int(0)),
    ];
    if encoder_id.contains("nvenc") {
        s.extend([
            // The obs-nvenc plugin (OBS 30.2+) reads "preset"; "preset2" belonged to the old
            // FFmpeg-based NVENC encoder and is ignored (the default p5 would be used).
            ("preset", Setting::Str("p4")),
            ("tune", Setting::Str("hq")),
            ("multipass", Setting::Str("disabled")),
            ("lookahead", Setting::Bool(false)),
        ]);
    } else if encoder_id.contains("amf") {
        s.push(("preset", Setting::Str("quality")));
    } else if encoder_id == "obs_x264" {
        s.push(("preset", Setting::Str("veryfast")));
    }
    s
}

// ---------------------------------------------------------------------------------------------
// Platform source ids

#[cfg(windows)]
mod ids {
    pub const DISPLAY: &str = "monitor_capture";
    pub const DESKTOP_AUDIO: &str = "wasapi_output_capture";
    pub const MIC: &str = "wasapi_input_capture";
}
#[cfg(not(windows))]
mod ids {
    pub const DISPLAY: &str = "pipewire-screen-capture-source";
    pub const DESKTOP_AUDIO: &str = "pulse_output_capture";
    pub const MIC: &str = "pulse_input_capture";
}
pub use ids::{DESKTOP_AUDIO as DESKTOP_AUDIO_SOURCE, MIC as MIC_SOURCE};

/// How long the display gets to deliver its first frame. On Linux this includes the KDE
/// screen-share dialog the first time.
#[cfg(windows)]
const DISPLAY_WAIT: Duration = Duration::from_secs(5);
#[cfg(not(windows))]
const DISPLAY_WAIT: Duration = Duration::from_secs(180);

/// Backstop for a save that never even starts writing. Write failures are caught much sooner by
/// [`MuxWatch`].
pub const SAVE_TIMEOUT: Duration = Duration::from_secs(60);

// ---------------------------------------------------------------------------------------------
// Session

/// Things libobs tells us from its own threads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    Saved,
    OutputStopped { code: i64 },
}

pub type Notify = Box<dyn Fn(Signal) + Send + Sync>;

struct SignalContext {
    notify: Notify,
    stopped: Mutex<bool>,
    stopped_cv: Condvar,
}

unsafe extern "C" fn on_saved(data: *mut c_void, _cd: *mut obs::calldata_t) {
    let ctx = &*(data as *const SignalContext);
    (ctx.notify)(Signal::Saved);
}

unsafe extern "C" fn on_stop(data: *mut c_void, cd: *mut obs::calldata_t) {
    let ctx = &*(data as *const SignalContext);
    let code = obs::calldata_int(cd, "code");
    *ctx.stopped.lock().unwrap() = true;
    ctx.stopped_cv.notify_all();
    (ctx.notify)(Signal::OutputStopped { code });
}

#[derive(Debug)]
pub struct StartError {
    pub code: ErrorCode,
    pub message: String,
}

impl StartError {
    fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self { code, message: message.into() }
    }
    pub fn to_event(&self) -> Event {
        Event::error(self.code, self.message.clone())
    }
}

pub struct Session {
    scene: *mut obs::obs_scene_t,
    display: *mut obs::obs_source_t,
    audio_sources: Vec<*mut obs::obs_source_t>,
    audio_encoders: Vec<*mut obs::obs_encoder_t>,
    video_encoder: *mut obs::obs_encoder_t,
    output: *mut obs::obs_output_t,
    signals: *mut SignalContext,
    signals_connected: bool,
    started_at: Instant,
    buffer_seconds: u32,
    saves: SaveTracker,
    namer: ClipNamer,
    mux_watch: Option<MuxWatch>,
    stopping: bool,
    pub encoder: String,
    pub width: u32,
    pub height: u32,
}

impl Session {
    fn empty(params: &StartParams, notify: Notify) -> Self {
        let signals =
            Box::into_raw(Box::new(SignalContext { notify, stopped: Mutex::new(false), stopped_cv: Condvar::new() }));
        Self {
            scene: std::ptr::null_mut(),
            display: std::ptr::null_mut(),
            audio_sources: vec![],
            audio_encoders: vec![],
            video_encoder: std::ptr::null_mut(),
            output: std::ptr::null_mut(),
            signals,
            signals_connected: false,
            started_at: Instant::now(),
            buffer_seconds: params.buffer_seconds,
            saves: SaveTracker::new(SAVE_TIMEOUT),
            namer: ClipNamer::default(),
            mux_watch: None,
            stopping: false,
            encoder: String::new(),
            width: 0,
            height: 0,
        }
    }

    /// Builds the pipeline and starts the replay buffer. On error everything created so far is
    /// released (by `Drop`).
    pub fn start(params: &StartParams, registered_encoders: &[String], notify: Notify) -> Result<Self, StartError> {
        let mut s = Self::empty(params, notify);
        unsafe { s.build(params, registered_encoders)? };
        Ok(s)
    }

    unsafe fn build(&mut self, params: &StartParams, registered_encoders: &[String]) -> Result<(), StartError> {
        let o = api();
        // Relative to where the recorder was started, not OBS's bin folder (the current directory).
        let output_dir = crate::resolve_path(&params.output_dir).to_string_lossy().into_owned();
        std::fs::create_dir_all(&output_dir).map_err(|e| {
            StartError::new(ErrorCode::StartFailed, format!("Cannot create the clips folder {output_dir}: {e}"))
        })?;

        // --- canvas size and display source ---
        let (display_settings, (width, height)) = display_settings(params)?;
        obs::reset_video(params.fps, width, height)
            .map_err(|e| StartError::new(ErrorCode::StartFailed, format!("Video setup failed: {e}")))?;
        self.width = width;
        self.height = height;

        self.display = o.obs_source_create(
            cs(ids::DISPLAY).as_ptr(),
            cs("display").as_ptr(),
            display_settings.0,
            std::ptr::null_mut(),
        );
        drop(display_settings);
        if self.display.is_null() {
            return Err(StartError::new(
                ErrorCode::StartFailed,
                format!("Could not create the {} source", ids::DISPLAY),
            ));
        }
        self.scene = o.obs_scene_create(cs("klipt").as_ptr());
        let item = o.obs_scene_add(self.scene, self.display);
        let mut bounds = obs::vec2::default();
        bounds.__bindgen_anon_1.__bindgen_anon_1 =
            obs::vec2__bindgen_ty_1__bindgen_ty_1 { x: width as f32, y: height as f32 };
        o.obs_sceneitem_set_bounds_type(item, obs::obs_bounds_type_OBS_BOUNDS_SCALE_INNER);
        o.obs_sceneitem_set_bounds_alignment(item, obs::OBS_ALIGN_CENTER);
        o.obs_sceneitem_set_bounds(item, &bounds);
        o.obs_set_output_source(0, o.obs_scene_get_source(self.scene));

        let waited = Instant::now();
        while o.obs_source_get_width(self.display) == 0 {
            if waited.elapsed() > DISPLAY_WAIT {
                return Err(StartError::new(ErrorCode::NoDisplay, "The display didn't deliver any picture"));
            }
            obs::pump_events();
            std::thread::sleep(Duration::from_millis(10));
        }
        log!(
            "display: {}x{} after {} ms, canvas {width}x{height}@{}",
            o.obs_source_get_width(self.display),
            o.obs_source_get_height(self.display),
            waited.elapsed().as_millis(),
            params.fps
        );
        after_display_ready(self.display);

        // --- audio: system → track 1 (mixer bit 0), mic → track 2 (mixer bit 1) ---
        let desktop = Data::new().str("device_id", "default");
        let src = o.obs_source_create(
            cs(ids::DESKTOP_AUDIO).as_ptr(),
            cs("system audio").as_ptr(),
            desktop.0,
            std::ptr::null_mut(),
        );
        if src.is_null() {
            return Err(StartError::new(ErrorCode::StartFailed, "Could not capture system audio"));
        }
        o.obs_source_set_audio_mixers(src, 1 << 0);
        o.obs_set_output_source(1, src);
        self.audio_sources.push(src);
        if params.mic {
            let mic = Data::new().str("device_id", params.mic_device_id.as_deref().unwrap_or("default"));
            let src = o.obs_source_create(cs(ids::MIC).as_ptr(), cs("mic").as_ptr(), mic.0, std::ptr::null_mut());
            if src.is_null() {
                return Err(StartError::new(ErrorCode::StartFailed, "Could not capture the microphone"));
            }
            o.obs_source_set_audio_mixers(src, 1 << 1);
            o.obs_set_output_source(2, src);
            self.audio_sources.push(src);
        }
        for track in 0..self.audio_sources.len() {
            let aac = Data::new().int("bitrate", 160);
            let enc = o.obs_audio_encoder_create(
                cs("ffmpeg_aac").as_ptr(),
                cs(&format!("aac track {}", track + 1)).as_ptr(),
                aac.0,
                track,
                std::ptr::null_mut(),
            );
            if enc.is_null() {
                return Err(StartError::new(ErrorCode::StartFailed, "Could not create the AAC audio encoder"));
            }
            o.obs_encoder_set_audio(enc, o.obs_get_audio());
            self.audio_encoders.push(enc);
        }

        // --- replay buffer ---
        let rb = Data::new()
            .int("max_time_sec", params.buffer_seconds as i64)
            .int("max_size_mb", 8192)
            .str("directory", &output_dir)
            .str("format", FILENAME_FORMAT)
            .str("extension", "mp4")
            .bool("allow_spaces", true);
        self.output =
            o.obs_output_create(cs("replay_buffer").as_ptr(), cs("klipt replay").as_ptr(), rb.0, std::ptr::null_mut());
        if self.output.is_null() {
            return Err(StartError::new(ErrorCode::StartFailed, "Could not create the replay buffer"));
        }
        let sh = o.obs_output_get_signal_handler(self.output);
        o.signal_handler_connect(sh, cs("saved").as_ptr(), Some(on_saved), self.signals as *mut c_void);
        o.signal_handler_connect(sh, cs("stop").as_ptr(), Some(on_stop), self.signals as *mut c_void);
        self.signals_connected = true;
        for (i, enc) in self.audio_encoders.iter().enumerate() {
            o.obs_output_set_audio_encoder(self.output, *enc, i);
        }

        // --- video encoder: the first candidate that creates and starts ---
        let candidates = encoder_candidates(&params.encoder, registered_encoders);
        let mut created_any = false;
        let mut last_error = String::from("no H.264 encoder is available");
        for id in &candidates {
            let mut settings = Data::new();
            for (k, v) in video_encoder_settings(id, params.bitrate_kbps) {
                settings = match v {
                    Setting::Str(s) => settings.str(k, s),
                    Setting::Int(i) => settings.int(k, i),
                    Setting::Bool(b) => settings.bool(k, b),
                };
            }
            let enc =
                o.obs_video_encoder_create(cs(id).as_ptr(), cs("video").as_ptr(), settings.0, std::ptr::null_mut());
            if enc.is_null() {
                log!("encoder {id}: could not be created");
                continue;
            }
            created_any = true;
            o.obs_encoder_set_video(enc, o.obs_get_video());
            o.obs_output_set_video_encoder(self.output, enc);
            if o.obs_output_start(self.output) {
                log!("encoder {id}: started");
                self.video_encoder = enc;
                self.encoder = id.clone();
                self.started_at = Instant::now();
                return Ok(());
            }
            last_error = from_c(o.obs_output_get_last_error(self.output));
            if last_error.is_empty() {
                last_error = format!("{id} failed to start");
            }
            log!("encoder {id}: start failed: {last_error}");
            o.obs_output_set_video_encoder(self.output, std::ptr::null_mut());
            o.obs_encoder_release(enc);
        }
        if created_any {
            Err(StartError::new(ErrorCode::StartFailed, format!("Recording could not start: {last_error}")))
        } else {
            Err(StartError::new(
                ErrorCode::NoEncoder,
                format!("No usable H.264 encoder (tried {})", candidates.join(", ")),
            ))
        }
    }

    /// Asks the replay buffer to write its contents. The result arrives as [`Signal::Saved`].
    pub fn save(&mut self) -> Result<(), Event> {
        let now = Instant::now();
        let duration_ms = clip_duration_ms(self.started_at.elapsed(), self.buffer_seconds);
        if self.saves.begin(now, duration_ms).is_err() {
            return Err(Event::error(ErrorCode::Busy, "Already saving"));
        }
        let ok = unsafe {
            let o = api();
            // libobs reads "format" when the save starts; a fresh name per burst avoids overwriting.
            let format = Data::new().str("format", &self.namer.next_format(now));
            o.obs_output_update(self.output, format.0);
            self.mux_watch = Some(MuxWatch::new(self.last_replay_path()));
            let mut cd: obs::calldata_t = std::mem::zeroed();
            let called = o.proc_handler_call(o.obs_output_get_proc_handler(self.output), cs("save").as_ptr(), &mut cd);
            o.bfree(cd.stack as *mut c_void);
            called
        };
        if !ok {
            self.saves.finish();
            self.mux_watch = None;
            return Err(Event::error(ErrorCode::SaveFailed, "The replay buffer refused to save"));
        }
        Ok(())
    }

    /// The last written replay, or "" while the replay buffer is writing one.
    fn last_replay_path(&self) -> String {
        unsafe { obs::call_output_proc(self.output, "get_last_replay", Some("path")) }.unwrap_or_default()
    }

    pub fn is_saving(&self) -> bool {
        self.saves.is_saving()
    }

    /// Handles [`Signal::Saved`]: returns the `saved` event for the pending save.
    pub fn on_saved(&mut self) -> Option<Event> {
        let duration_ms = self.saves.finish()?;
        self.mux_watch = None;
        let path = self.last_replay_path();
        if path.is_empty() {
            return Some(Event::error(ErrorCode::SaveFailed, "The replay buffer didn't report a file"));
        }
        // libobs reports "C:/Users/..." on Windows; hand the app a native path.
        let path = if cfg!(windows) { path.replace('/', "\\") } else { path };
        Some(Event::Saved { path, duration_ms })
    }

    /// Periodic check for saves that failed: a write that ended without `saved`, or a save that
    /// never started.
    pub fn poll(&mut self) -> Option<Event> {
        let now = Instant::now();
        if self.saves.is_saving() {
            let current = self.last_replay_path();
            if self.mux_watch.as_mut().is_some_and(|w| w.failed(&current, now)) {
                self.saves.finish();
                self.mux_watch = None;
                return Some(Event::error(ErrorCode::SaveFailed, "The clip file could not be written"));
            }
        }
        if self.saves.check_timeout(now) {
            self.mux_watch = None;
            return Some(Event::error(ErrorCode::SaveFailed, "Saving the clip took too long"));
        }
        None
    }

    /// True once `stop` was requested; an `OutputStopped` signal after that is expected.
    pub fn is_stopping(&self) -> bool {
        self.stopping
    }

    /// Stops the replay buffer (discarding its contents). Release happens on drop.
    pub fn stop(&mut self) {
        self.stopping = true;
        if self.output.is_null() {
            return;
        }
        unsafe {
            let o = api();
            if !o.obs_output_active(self.output) {
                return;
            }
            o.obs_output_stop(self.output);
            let ctx = &*self.signals;
            let guard = ctx.stopped.lock().unwrap_or_else(|e| e.into_inner());
            let (_guard, timeout) =
                ctx.stopped_cv.wait_timeout_while(guard, Duration::from_secs(5), |stopped| !*stopped).unwrap();
            if timeout.timed_out() {
                log!("replay buffer didn't stop within 5 s; forcing");
                o.obs_output_force_stop(self.output);
            }
            // The stop signal fires before libobs finishes tearing down; releasing before the output
            // is inactive can race with obs_output_destroy's own stop and hang.
            let deadline = Instant::now() + Duration::from_secs(5);
            while o.obs_output_active(self.output) && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.stop();
        unsafe {
            let o = api();
            if !self.output.is_null() {
                if self.signals_connected {
                    let sh = o.obs_output_get_signal_handler(self.output);
                    o.signal_handler_disconnect(sh, cs("saved").as_ptr(), Some(on_saved), self.signals as *mut c_void);
                    o.signal_handler_disconnect(sh, cs("stop").as_ptr(), Some(on_stop), self.signals as *mut c_void);
                }
                o.obs_output_release(self.output);
            }
            if !self.video_encoder.is_null() {
                o.obs_encoder_release(self.video_encoder);
            }
            for enc in self.audio_encoders.drain(..) {
                o.obs_encoder_release(enc);
            }
            for channel in 0..3 {
                o.obs_set_output_source(channel, std::ptr::null_mut());
            }
            for src in self.audio_sources.drain(..) {
                o.obs_source_release(src);
            }
            if !self.scene.is_null() {
                o.obs_scene_release(self.scene);
            }
            if !self.display.is_null() {
                o.obs_source_release(self.display);
            }
            drop(Box::from_raw(self.signals));
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Platform display setup

/// Windows: capture the chosen monitor (DXGI or WGC, picked by libobs) and size the canvas to
/// the monitor scaled into the requested box.
#[cfg(windows)]
fn display_settings(params: &StartParams) -> Result<(Data, (u32, u32)), StartError> {
    let displays = crate::displays::list();
    let chosen = match &params.display_id {
        Some(id) => displays.iter().find(|d| &d.id == id),
        None => displays.iter().find(|d| d.primary).or(displays.first()),
    }
    .ok_or_else(|| StartError::new(ErrorCode::NoDisplay, "That display isn't connected"))?;
    let size = fit_inside(chosen.width, chosen.height, params.width, params.height);
    let settings = Data::new().str("monitor_id", &chosen.id).int("method", 0).bool("capture_cursor", true);
    Ok((settings, size))
}

/// Linux: the screen-cast portal picks the screen; a saved restore token skips the dialog.
#[cfg(not(windows))]
fn display_settings(params: &StartParams) -> Result<(Data, (u32, u32)), StartError> {
    let mut settings = Data::new().bool("ShowCursor", true);
    if let Ok(token) = std::fs::read_to_string(restore_token_path()) {
        settings = settings.str("RestoreToken", token.trim());
    }
    Ok((settings, fit_inside(params.width, params.height, params.width, params.height)))
}

#[cfg(not(windows))]
fn restore_token_path() -> std::path::PathBuf {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/share"));
    base.join("klipt").join("obs-portal-restore-token")
}

#[cfg(windows)]
unsafe fn after_display_ready(_display: *mut obs::obs_source_t) {}

#[cfg(not(windows))]
unsafe fn after_display_ready(display: *mut obs::obs_source_t) {
    let o = api();
    let settings = o.obs_source_get_settings(display);
    let token = from_c(o.obs_data_get_string(settings, cs("RestoreToken").as_ptr()));
    o.obs_data_release(settings);
    if !token.is_empty() {
        let path = restore_token_path();
        let _ = std::fs::create_dir_all(path.parent().unwrap());
        if let Err(e) = std::fs::write(&path, token) {
            log!("could not save the portal restore token: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_save_while_saving_is_busy() {
        let t0 = Instant::now();
        let mut s = SaveTracker::new(Duration::from_secs(120));
        assert_eq!(s.begin(t0, 30_000), Ok(()));
        assert_eq!(s.begin(t0 + Duration::from_millis(50), 30_000), Err(Busy));
        assert!(s.is_saving());
    }

    #[test]
    fn finishing_reports_the_pending_duration_and_allows_the_next_save() {
        let t0 = Instant::now();
        let mut s = SaveTracker::new(Duration::from_secs(120));
        s.begin(t0, 4_200).unwrap();
        assert_eq!(s.finish(), Some(4_200));
        assert!(!s.is_saving());
        assert_eq!(s.begin(t0 + Duration::from_secs(1), 30_000), Ok(()));
    }

    #[test]
    fn a_saved_signal_without_a_pending_save_is_ignored() {
        let mut s = SaveTracker::new(Duration::from_secs(120));
        assert_eq!(s.finish(), None);
    }

    #[test]
    fn quick_saves_get_distinct_file_names() {
        let t0 = Instant::now();
        let mut n = ClipNamer::default();
        assert_eq!(n.next_format(t0), FILENAME_FORMAT);
        assert_eq!(n.next_format(t0 + Duration::from_millis(500)), format!("{FILENAME_FORMAT} (2)"));
        assert_eq!(n.next_format(t0 + Duration::from_millis(1900)), format!("{FILENAME_FORMAT} (3)"));
        // a pause of 2 s or more starts over: the timestamp alone is unique again
        assert_eq!(n.next_format(t0 + Duration::from_millis(3900)), FILENAME_FORMAT);
        assert_eq!(n.next_format(t0 + Duration::from_millis(4000)), format!("{FILENAME_FORMAT} (2)"));
    }

    #[test]
    fn mux_watch_ignores_the_old_path_and_running_writes() {
        let t0 = Instant::now();
        let mut w = MuxWatch::new("C:/clips/old.mp4".into());
        // save not started yet: libobs still reports the previous file
        assert!(!w.failed("C:/clips/old.mp4", t0 + Duration::from_secs(5)));
        // writing: get_last_replay returns ""
        assert!(!w.failed("", t0 + Duration::from_secs(6)));
    }

    #[test]
    fn mux_watch_reports_a_write_that_ended_without_saved() {
        let t0 = Instant::now();
        let mut w = MuxWatch::new(String::new());
        assert!(!w.failed("C:/clips/new.mp4", t0));
        assert!(!w.failed("C:/clips/new.mp4", t0 + Duration::from_millis(900)));
        assert!(w.failed("C:/clips/new.mp4", t0 + MuxWatch::GRACE));
    }

    #[test]
    fn mux_watch_restarts_its_grace_if_writing_resumes() {
        let t0 = Instant::now();
        let mut w = MuxWatch::new("old".into());
        assert!(!w.failed("new", t0));
        assert!(!w.failed("", t0 + Duration::from_millis(500)));
        assert!(!w.failed("new", t0 + Duration::from_millis(1200)));
        assert!(w.failed("new", t0 + Duration::from_millis(2200)));
    }

    #[test]
    fn a_stuck_save_times_out_once_and_frees_the_tracker() {
        let t0 = Instant::now();
        let mut s = SaveTracker::new(Duration::from_secs(120));
        s.begin(t0, 30_000).unwrap();
        assert!(!s.check_timeout(t0 + Duration::from_secs(119)));
        assert!(s.check_timeout(t0 + Duration::from_secs(120)));
        assert!(!s.check_timeout(t0 + Duration::from_secs(121)));
        assert_eq!(s.begin(t0 + Duration::from_secs(122), 30_000), Ok(()));
    }

    #[test]
    fn clip_duration_is_capped_by_the_buffer() {
        assert_eq!(clip_duration_ms(Duration::from_millis(4_200), 30), 4_200);
        assert_eq!(clip_duration_ms(Duration::from_secs(100), 30), 30_000);
    }

    #[test]
    fn fit_inside_keeps_aspect_and_never_upscales() {
        assert_eq!(fit_inside(2560, 1440, 1920, 1080), (1920, 1080));
        assert_eq!(fit_inside(1920, 1080, 1920, 1080), (1920, 1080));
        assert_eq!(fit_inside(1280, 720, 1920, 1080), (1280, 720));
        // 21:9 ultrawide into a 16:9 box: width-limited
        assert_eq!(fit_inside(3440, 1440, 1920, 1080), (1920, 804));
        // 16:10 into 16:9: height-limited
        assert_eq!(fit_inside(1920, 1200, 1920, 1080), (1728, 1080));
        // odd results are rounded to even
        assert_eq!(fit_inside(1366, 768, 1280, 720), (1280, 720));
        let (w, h) = fit_inside(1001, 1001, 777, 777);
        assert_eq!((w % 2, h % 2), (0, 0));
    }

    #[test]
    fn auto_tries_hardware_encoders_before_x264() {
        let registered: Vec<String> = ["ffmpeg_aac", "obs_x264", "h264_texture_amf", "obs_nvenc_h264_tex"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(encoder_candidates("auto", &registered), vec!["obs_nvenc_h264_tex", "h264_texture_amf", "obs_x264"]);
        assert_eq!(encoder_candidates("obs_x264", &registered), vec!["obs_x264"]);
        assert_eq!(encoder_candidates("auto", &[]), Vec::<String>::new());
    }

    #[test]
    fn nvenc_gets_the_measured_settings() {
        let s = video_encoder_settings("obs_nvenc_h264_tex", 20000);
        for expected in [
            ("rate_control", Setting::Str("CBR")),
            ("bitrate", Setting::Int(20000)),
            ("keyint_sec", Setting::Int(1)),
            ("preset", Setting::Str("p4")),
            ("tune", Setting::Str("hq")),
            ("multipass", Setting::Str("disabled")),
            ("lookahead", Setting::Bool(false)),
            ("bf", Setting::Int(0)),
            ("profile", Setting::Str("high")),
        ] {
            assert!(s.contains(&expected), "missing {expected:?}");
        }
        assert!(!s.iter().any(|(k, _)| *k == "preset2"), "obs-nvenc ignores preset2");
        let amf = video_encoder_settings("h264_texture_amf", 14000);
        assert!(amf.contains(&("bitrate", Setting::Int(14000))));
        assert!(amf.contains(&("preset", Setting::Str("quality"))));
        assert!(!amf.iter().any(|(k, _)| *k == "tune"));
    }
}
