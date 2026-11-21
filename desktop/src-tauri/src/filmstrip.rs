//! The editor's filmstrip: one small JPEG per 100 ms of a clip, cached in `<cache>/edit/<id>-strip`
//! so the timeline can show frames and scrub without decoding the video.

use std::fs;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::SystemTime;

use crate::errors::{AppError, Result};
use crate::ffmpeg::Tools;

pub const FRAMES_PER_SECOND: u32 = 10;
pub const WIDTH: u32 = 640;

const COMPLETE_MARKER: &str = "complete";
const OPENED_MARKER: &str = "opened";
const STRIP_SUFFIX: &str = "-strip";
/// How often a running extraction looks at its stop flag.
const STOP_POLL: std::time::Duration = std::time::Duration::from_millis(20);
/// How long `Jobs` waits for a stopped job to finish before carrying on.
const STOP_WAIT: std::time::Duration = std::time::Duration::from_secs(3);

/// Where the filmstrip of clip `id` lives: `<cache>/edit/<id>-strip`.
pub fn strip_dir(cache_dir: &Path, id: &str) -> PathBuf {
    cache_dir.join("edit").join(format!("{id}{STRIP_SUFFIX}"))
}

/// The frame at `index` (0 = 0 ms, 1 = 100 ms, ...): `<dir>/<index padded to 6>.jpg`.
pub fn frame_path(dir: &Path, index: u32) -> PathBuf {
    dir.join(format!("{index:06}.jpg"))
}

/// How many frames a clip of `duration_ms` has.
pub fn expected_frames(duration_ms: u64) -> u32 {
    let step = 1000 / FRAMES_PER_SECOND as u64;
    duration_ms.div_ceil(step) as u32
}

/// Whether extraction finished, so every frame on disk is whole.
pub fn is_complete(dir: &Path) -> bool {
    dir.join(COMPLETE_MARKER).exists()
}

/// How many frames from 0 on are readable. Until extraction is complete the highest one is left
/// out, since it may be half-written.
pub fn ready_frames(dir: &Path) -> u32 {
    let mut count = 0;
    while frame_path(dir, count).exists() {
        count += 1;
    }
    if is_complete(dir) { count } else { count.saturating_sub(1) }
}

/// Notes that the strip was just opened (its `opened` file's mtime is the last open).
pub fn mark_opened(dir: &Path) {
    let marker = dir.join(OPENED_MARKER);
    let result = fs::create_dir_all(dir).and_then(|()| {
        fs::OpenOptions::new().write(true).create(true).truncate(false).open(&marker)?.set_modified(SystemTime::now())
    });
    if let Err(e) = result {
        crate::elog!("klipt: can't mark {} opened: {e}", marker.display());
    }
}

/// One cached filmstrip, as far as eviction cares.
pub struct StripInfo {
    pub id: String,
    pub bytes: u64,
    pub opened: SystemTime,
}

/// The strips to delete, least recently opened first, so the rest fit in `cap_bytes`.
/// The strip `keep_id` is never chosen, even if it alone is over the cap.
pub fn choose_evictions(strips: &[StripInfo], cap_bytes: u64, keep_id: &str) -> Vec<String> {
    let mut total: u64 = strips.iter().map(|s| s.bytes).sum();
    let mut candidates: Vec<&StripInfo> = strips.iter().filter(|s| s.id != keep_id).collect();
    candidates.sort_by_key(|s| s.opened);
    let mut chosen = Vec::new();
    for strip in candidates {
        if total <= cap_bytes {
            break;
        }
        total -= strip.bytes;
        chosen.push(strip.id.clone());
    }
    chosen
}

fn list_strips(cache_dir: &Path) -> Vec<StripInfo> {
    let Ok(entries) = fs::read_dir(cache_dir.join("edit")) else { return Vec::new() };
    entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let id = name.strip_suffix(STRIP_SUFFIX)?.to_string();
            let dir = entry.path();
            if !dir.is_dir() {
                return None;
            }
            let bytes = fs::read_dir(&dir)
                .map(|files| files.flatten().filter_map(|f| f.metadata().ok()).filter(|m| m.is_file()).map(|m| m.len()).sum())
                .unwrap_or(0);
            let opened = fs::metadata(dir.join(OPENED_MARKER))
                .and_then(|m| m.modified())
                .or_else(|_| fs::metadata(&dir).and_then(|m| m.modified()))
                .unwrap_or(SystemTime::UNIX_EPOCH);
            Some(StripInfo { id, bytes, opened })
        })
        .collect()
}

/// Deletes the least recently opened strips until the cache's strips fit in `cap_bytes`
/// (never `keep_id`'s). Best effort.
pub fn evict(cache_dir: &Path, cap_bytes: u64, keep_id: &str) {
    for id in choose_evictions(&list_strips(cache_dir), cap_bytes, keep_id) {
        let dir = strip_dir(cache_dir, &id);
        if let Err(e) = fs::remove_dir_all(&dir) {
            crate::elog!("klipt: can't delete {}: {e}", dir.display());
        }
    }
}

/// The ffmpeg arguments that write the frames of `input` from frame `start` on into `dir`.
fn ffmpeg_args(input: &Path, dir: &Path, start: u32) -> Vec<std::ffi::OsString> {
    let step_ms = 1000 / FRAMES_PER_SECOND as u64;
    let mut args: Vec<std::ffi::OsString> = ["-nostdin", "-hide_banner", "-loglevel", "error", "-y"].map(Into::into).into();
    // Two threads for decoding (before `-i`), one for the filters, two for the JPEG encoder: a
    // background job that leaves the CPU to the game and the editor.
    args.extend(["-threads", "2", "-filter_threads", "1", "-ss"].map(Into::into));
    args.push(format!("{:.3}", start as f64 * step_ms as f64 / 1000.0).into());
    args.extend(["-i".into(), input.into(), "-an".into(), "-vf".into()]);
    args.push(format!("fps={FRAMES_PER_SECOND},scale={WIDTH}:-2").into());
    args.extend(["-q:v", "5", "-start_number"].map(Into::into));
    args.push(start.to_string().into());
    args.extend(["-threads", "2", "-progress", "pipe:1", "-nostats"].map(Into::into));
    args.push(dir.join("%06d.jpg").into());
    args
}

/// Extracts the filmstrip of `input` into `dir` at the lowest CPU priority, continuing after the
/// frames already there (the half-written trailing one is deleted first). `progress` gets the
/// number of readable frames as ffmpeg advances; `stop` kills ffmpeg (returns "stopped"). Writes
/// the `complete` marker on success.
pub fn extract(
    tools: &Tools,
    input: &Path,
    dir: &Path,
    duration_ms: u64,
    stop: &AtomicBool,
    mut progress: impl FnMut(u32),
) -> Result<()> {
    fs::create_dir_all(dir)?;
    if is_complete(dir) {
        return Ok(());
    }
    let start = ready_frames(dir);
    // Not counted by `ready_frames`, so possibly half-written; ffmpeg rewrites it.
    let _ = fs::remove_file(frame_path(dir, start));
    if start >= expected_frames(duration_ms) {
        fs::write(dir.join(COMPLETE_MARKER), b"")?;
        return Ok(());
    }

    let mut command = Command::new(&tools.ffmpeg);
    command.args(ffmpeg_args(input, dir, start));
    command.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // SAFETY: setpriority is async-signal-safe, so it is fine between fork and exec.
        unsafe {
            command.pre_exec(|| {
                libc::setpriority(libc::PRIO_PROCESS, 0, 19);
                Ok(())
            });
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const IDLE_PRIORITY_CLASS: u32 = 0x40;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(IDLE_PRIORITY_CLASS | CREATE_NO_WINDOW);
    }
    let mut child = command
        .spawn()
        .map_err(|e| AppError::ToolFailed { tool: "ffmpeg", detail: format!("can't start {}: {e}", tools.ffmpeg.display()) })?;
    // stderr on its own thread, so a chatty ffmpeg can't block on a full pipe.
    let mut stderr = child.stderr.take().expect("piped");
    let stderr_reader = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stderr.read_to_string(&mut text);
        text
    });
    // Progress lines are read on their own thread, so a stop is noticed within a few
    // milliseconds even while ffmpeg is quiet (it is niced, and may be slow to print).
    let stdout = child.stdout.take().expect("piped");
    let (lines_tx, lines_rx) = std::sync::mpsc::channel::<String>();
    let stdout_reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(std::result::Result::ok) {
            if lines_tx.send(line).is_err() {
                break;
            }
        }
    });
    let mut stopped = false;
    loop {
        if stop.load(Ordering::SeqCst) {
            stopped = true;
            let _ = child.kill();
            break;
        }
        match lines_rx.recv_timeout(STOP_POLL) {
            Ok(line) => {
                if line.starts_with("out_time_us=") {
                    progress(ready_frames(dir));
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    drop(lines_rx);
    let status = child.wait()?;
    let _ = stdout_reader.join();
    let stderr = stderr_reader.join().unwrap_or_default();
    if stopped || stop.load(Ordering::SeqCst) {
        return Err(AppError::ToolFailed { tool: "ffmpeg", detail: "stopped".into() });
    }
    // No new frame after a resume: the strip had already reached the video's end (the clip can be
    // recorded as a little longer than its video). ffmpeg fails on an empty output, so its status
    // can't tell this apart; a run from frame 0 that wrote nothing is a real failure.
    let wrote_nothing = !frame_path(dir, start).exists();
    if wrote_nothing && (status.success() || start > 0) {
        fs::write(dir.join(COMPLETE_MARKER), b"")?;
        progress(ready_frames(dir));
        return Ok(());
    }
    if !status.success() || wrote_nothing {
        let tail: Vec<&str> = stderr.lines().rev().take(5).collect();
        return Err(AppError::ToolFailed {
            tool: "ffmpeg",
            detail: format!("exited with {status}: {}", tail.into_iter().rev().collect::<Vec<_>>().join(" | ")),
        });
    }
    fs::write(dir.join(COMPLETE_MARKER), b"")?;
    progress(ready_frames(dir));
    Ok(())
}

/// Signalled when a job's closure has returned (also if it panicked).
#[derive(Default)]
struct Done {
    finished: std::sync::Mutex<bool>,
    changed: std::sync::Condvar,
}

impl Done {
    fn finish(&self) {
        *self.finished.lock().unwrap_or_else(|e| e.into_inner()) = true;
        self.changed.notify_all();
    }

    /// Waits until the job finished; false if it didn't within `STOP_WAIT`.
    fn wait(&self) -> bool {
        let guard = self.finished.lock().unwrap_or_else(|e| e.into_inner());
        let (guard, _) = self
            .changed
            .wait_timeout_while(guard, STOP_WAIT, |finished| !*finished)
            .unwrap_or_else(|e| e.into_inner());
        *guard
    }
}

struct Running {
    id: String,
    stop: Arc<AtomicBool>,
    done: Arc<Done>,
}

/// The one filmstrip extraction that may be running: starting another stops it.
#[derive(Default)]
pub struct Jobs {
    running: std::sync::Mutex<Option<Running>>,
}

impl Jobs {
    /// Stops the running job, if any, then runs `run` for clip `id` on a blocking thread, once
    /// the stopped job has finished (so two never write the same folder). `run` should return
    /// soon after its flag turns true.
    pub fn start(&self, id: String, run: impl FnOnce(&AtomicBool) + Send + 'static) {
        let stop = Arc::new(AtomicBool::new(false));
        let done = Arc::new(Done::default());
        let previous = self.slot().replace(Running { id, stop: stop.clone(), done: done.clone() });
        if let Some(previous) = &previous {
            previous.stop.store(true, Ordering::SeqCst);
        }
        tauri::async_runtime::spawn_blocking(move || {
            if let Some(previous) = previous {
                if !previous.done.wait() {
                    crate::elog!("klipt: the previous filmstrip job didn't stop in time");
                }
            }
            // Signals even if `run` panics.
            struct Finish(Arc<Done>);
            impl Drop for Finish {
                fn drop(&mut self) {
                    self.0.finish();
                }
            }
            let _finish = Finish(done);
            run(&stop);
        });
    }

    /// Stops the job of clip `id`, if that is the one running, and waits (a few seconds at most)
    /// until it has finished, so nothing is still writing its folder when this returns.
    /// Blocks: call it off the async threads.
    pub fn stop(&self, id: &str) {
        let done = match &*self.slot() {
            Some(running) if running.id == id => {
                running.stop.store(true, Ordering::SeqCst);
                running.done.clone()
            }
            _ => return,
        };
        if !done.wait() {
            crate::elog!("klipt: the filmstrip job of clip {id} didn't stop in time");
        }
    }

    pub fn stop_all(&self) {
        let done = match &*self.slot() {
            Some(running) => {
                running.stop.store(true, Ordering::SeqCst);
                running.done.clone()
            }
            None => return,
        };
        done.wait();
    }

    fn slot(&self) -> std::sync::MutexGuard<'_, Option<Running>> {
        self.running.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    fn info(id: &str, bytes: u64, secs: u64) -> StripInfo {
        StripInfo { id: id.into(), bytes, opened: SystemTime::UNIX_EPOCH + Duration::from_secs(secs) }
    }

    fn strip_with_frames(frames: &[u32]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for &i in frames {
            fs::write(frame_path(dir.path(), i), b"x").unwrap();
        }
        dir
    }

    #[test]
    fn expected_frames_rounds_up() {
        assert_eq!(expected_frames(0), 0);
        assert_eq!(expected_frames(1), 1);
        assert_eq!(expected_frames(100), 1);
        assert_eq!(expected_frames(101), 2);
        assert_eq!(expected_frames(67_000), 670);
    }

    #[test]
    fn ready_frames_skips_the_last_until_complete() {
        let dir = strip_with_frames(&[0, 1, 2, 3, 4]);
        assert_eq!(ready_frames(dir.path()), 4);
        fs::write(dir.path().join("complete"), b"").unwrap();
        assert!(is_complete(dir.path()));
        assert_eq!(ready_frames(dir.path()), 5);

        let gappy = strip_with_frames(&[0, 1, 3]);
        assert_eq!(ready_frames(gappy.path()), 1);
    }

    #[test]
    fn choose_evictions_drops_least_recently_opened_until_under_cap() {
        let strips = [info("a", 100, 1), info("b", 100, 3), info("c", 100, 2)];
        assert_eq!(choose_evictions(&strips, 150, ""), ["a", "c"]);
    }

    #[test]
    fn choose_evictions_never_drops_the_kept_strip() {
        let strips = [info("a", 300, 1), info("b", 100, 2)];
        assert_eq!(choose_evictions(&strips, 150, "a"), ["b"]);
    }

    #[test]
    fn choose_evictions_none_under_cap() {
        let strips = [info("a", 100, 1), info("b", 100, 2)];
        assert!(choose_evictions(&strips, 200, "").is_empty());
    }

    #[test]
    fn evict_deletes_the_oldest_opened_strip_dir() {
        let cache = tempfile::tempdir().unwrap();
        for (id, secs) in [("old", 1), ("new", 2)] {
            let dir = strip_dir(cache.path(), id);
            fs::create_dir_all(&dir).unwrap();
            fs::write(frame_path(&dir, 0), vec![0u8; 100]).unwrap();
            mark_opened(&dir);
            let opened = SystemTime::UNIX_EPOCH + Duration::from_secs(secs);
            fs::File::options().write(true).open(dir.join("opened")).unwrap().set_modified(opened).unwrap();
        }
        evict(cache.path(), 150, "");
        assert!(!strip_dir(cache.path(), "old").exists());
        assert!(strip_dir(cache.path(), "new").exists());
    }

    /// The value right after `flag` in `args`, and where `flag` is.
    fn arg_after(args: &[std::ffi::OsString], flag: &str, from: usize) -> Option<(usize, String)> {
        let i = args.iter().skip(from).position(|a| a == flag)? + from;
        Some((i, args.get(i + 1)?.to_string_lossy().into_owned()))
    }

    #[test]
    fn ffmpeg_limits_decoding_and_filtering_threads_too() {
        let args = ffmpeg_args(Path::new("in.mp4"), Path::new("strip"), 0);
        let input = args.iter().position(|a| a == "-i").unwrap();
        let (before_input, decode) = arg_after(&args, "-threads", 0).unwrap();
        assert!(before_input < input, "no -threads before -i: {args:?}");
        assert_eq!(decode, "2");
        let (_, encode) = arg_after(&args, "-threads", input).expect("no -threads after -i");
        assert_eq!(encode, "2");
        let (_, filter) = arg_after(&args, "-filter_threads", 0).expect("no -filter_threads");
        assert_eq!(filter, "1");
    }

    #[test]
    fn extract_completes_a_strip_that_stopped_at_the_last_frame() {
        // The clip is recorded as longer than its video (7 s vs 6 s): the strip stopped after the
        // video's last frame, and resuming there makes ffmpeg write nothing new.
        let dir = strip_with_frames(&(0..=60).collect::<Vec<_>>());
        let stop = AtomicBool::new(false);
        extract(&crate::ffmpeg::tests::tools(), crate::ffmpeg::tests::fixture(), dir.path(), 7000, &stop, |_| {}).unwrap();
        assert!(is_complete(dir.path()));
        assert_eq!(ready_frames(dir.path()), 60);
    }

    #[test]
    fn extract_writes_ten_frames_per_second() {
        let dir = tempfile::tempdir().unwrap();
        let stop = AtomicBool::new(false);
        extract(&crate::ffmpeg::tests::tools(), crate::ffmpeg::tests::fixture(), dir.path(), 6000, &stop, |_| {}).unwrap();
        assert_eq!(ready_frames(dir.path()), expected_frames(6000));
        assert_eq!(ready_frames(dir.path()), 60);
        assert!(is_complete(dir.path()));
        let probe = crate::ffmpeg::probe(&crate::ffmpeg::tests::tools(), &frame_path(dir.path(), 0)).unwrap();
        assert_eq!(probe.width, 640);
    }

    #[test]
    fn extract_resumes_after_stop() {
        let tools = crate::ffmpeg::tests::tools();
        let dir = tempfile::tempdir().unwrap();
        let stop = AtomicBool::new(false);
        let result = extract(&tools, crate::ffmpeg::tests::fixture(), dir.path(), 6000, &stop, |count| {
            if count >= 10 {
                stop.store(true, Ordering::SeqCst);
            }
        });
        assert!(result.is_err());
        assert!(!is_complete(dir.path()));
        let partial = ready_frames(dir.path());
        assert!((10..60).contains(&partial), "partial = {partial}");
        let first_modified = fs::metadata(frame_path(dir.path(), 0)).unwrap().modified().unwrap();

        let stop = AtomicBool::new(false);
        extract(&tools, crate::ffmpeg::tests::fixture(), dir.path(), 6000, &stop, |_| {}).unwrap();
        assert!(is_complete(dir.path()));
        assert_eq!(ready_frames(dir.path()), 60);
        assert_eq!(fs::metadata(frame_path(dir.path(), 0)).unwrap().modified().unwrap(), first_modified);
    }

    #[test]
    fn a_new_job_waits_for_the_one_it_stops() {
        let jobs = Jobs::default();
        let log = Arc::new(std::sync::Mutex::new(Vec::new()));
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let l = log.clone();
        jobs.start("a".into(), move |stop| {
            started_tx.send(()).unwrap();
            while !stop.load(Ordering::SeqCst) {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            std::thread::sleep(std::time::Duration::from_millis(150));
            l.lock().unwrap().push("a ended");
        });
        started_rx.recv().unwrap();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let l = log.clone();
        jobs.start("b".into(), move |_| {
            l.lock().unwrap().push("b started");
            done_tx.send(()).unwrap();
        });
        done_rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        assert_eq!(*log.lock().unwrap(), vec!["a ended", "b started"]);
    }

    #[test]
    fn stopping_another_clip_leaves_the_job_running() {
        let jobs = Jobs::default();
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let seen_stop = Arc::new(AtomicBool::new(false));
        let (flag, release) = (seen_stop.clone(), Arc::new(AtomicBool::new(false)));
        let r = release.clone();
        jobs.start("a".into(), move |stop| {
            started_tx.send(()).unwrap();
            while !r.load(Ordering::SeqCst) {
                flag.store(stop.load(Ordering::SeqCst), Ordering::SeqCst);
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        });
        started_rx.recv().unwrap();
        jobs.stop("other");
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert!(!seen_stop.load(Ordering::SeqCst));
        release.store(true, Ordering::SeqCst);
    }

    #[test]
    fn stop_returns_after_the_job_returned() {
        let jobs = Jobs::default();
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let finished = Arc::new(AtomicBool::new(false));
        let f = finished.clone();
        jobs.start("a".into(), move |stop| {
            started_tx.send(()).unwrap();
            while !stop.load(Ordering::SeqCst) {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            std::thread::sleep(std::time::Duration::from_millis(150));
            f.store(true, Ordering::SeqCst);
        });
        started_rx.recv().unwrap();
        jobs.stop("a");
        assert!(finished.load(Ordering::SeqCst));
    }
}
