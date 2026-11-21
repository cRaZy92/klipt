//! The app's log file, so problems can be looked at after the fact (the Windows build has no
//! console). Every `elog!` line goes to stderr, as before, and to `<app data>/logs/klipt.log`.
//!
//! On start the previous run's log becomes `klipt.previous.log`, so a crash's log survives the
//! restart that follows it. A log that grows past [`MAX_BYTES`] is moved there too and restarted.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const FILE_NAME: &str = "klipt.log";
pub const PREVIOUS_NAME: &str = "klipt.previous.log";
/// A run's log is restarted past this size (the recorder can be chatty).
pub const MAX_BYTES: u64 = 8 * 1024 * 1024;

struct LogFile {
    dir: PathBuf,
    file: File,
    written: u64,
}

static LOG: Mutex<Option<LogFile>> = Mutex::new(None);

/// Logs a line: `elog!("klipt: {thing} failed: {e}")`. Same arguments as `eprintln!`.
#[macro_export]
macro_rules! elog {
    ($($arg:tt)*) => {
        $crate::logging::log(::std::format_args!($($arg)*))
    };
}

/// Starts logging to `dir` (created if needed), keeping the previous run's log aside.
pub fn init(dir: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    let current = dir.join(FILE_NAME);
    if current.exists() {
        let _ = fs::rename(&current, dir.join(PREVIOUS_NAME));
    }
    let file = OpenOptions::new().create(true).append(true).open(&current)?;
    *LOG.lock().unwrap_or_else(|p| p.into_inner()) = Some(LogFile { dir: dir.to_path_buf(), file, written: 0 });
    install_panic_hook();
    Ok(())
}

/// The folder the logs are in, once [`init`] ran.
pub fn dir() -> Option<PathBuf> {
    LOG.lock().unwrap_or_else(|p| p.into_inner()).as_ref().map(|l| l.dir.clone())
}

fn timestamp() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string()
}

/// Writes one line to stderr and the log file. Never fails: logging must not break the app.
pub fn log(args: std::fmt::Arguments<'_>) {
    let text = args.to_string();
    eprintln!("{text}");
    let line = format!("{} {text}\n", timestamp());
    let mut guard = LOG.lock().unwrap_or_else(|p| p.into_inner());
    let Some(log) = guard.as_mut() else { return };
    if log.written + line.len() as u64 > MAX_BYTES {
        let current = log.dir.join(FILE_NAME);
        let _ = log.file.flush();
        let _ = fs::rename(&current, log.dir.join(PREVIOUS_NAME));
        match OpenOptions::new().create(true).append(true).open(&current) {
            Ok(file) => {
                log.file = file;
                log.written = 0;
            }
            Err(_) => return,
        }
    }
    if log.file.write_all(line.as_bytes()).is_ok() {
        log.written += line.len() as u64;
    }
}

/// Panics are logged with where they happened, then handled as before.
fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        let name = thread.name().unwrap_or("unnamed");
        let backtrace = std::backtrace::Backtrace::force_capture();
        log(format_args!("klipt: PANIC in thread '{name}': {info}\n{backtrace}"));
        previous(info);
    }));
}

/// The end of the log (this run's, after the previous run's when it fits), at most `max_bytes`.
pub fn tail(max_bytes: u64) -> String {
    let Some(dir) = dir() else { return String::new() };
    if let Some(log) = LOG.lock().unwrap_or_else(|p| p.into_inner()).as_mut() {
        let _ = log.file.flush();
    }
    let current = read_tail(&dir.join(FILE_NAME), max_bytes);
    let left = max_bytes.saturating_sub(current.len() as u64);
    let mut out = String::new();
    if left > 4096 {
        let previous = read_tail(&dir.join(PREVIOUS_NAME), left);
        if !previous.is_empty() {
            out.push_str("===== previous run =====\n");
            out.push_str(&previous);
            out.push_str("\n===== this run =====\n");
        }
    }
    out.push_str(&current);
    out
}

/// The last `max_bytes` of a file, starting at a whole line; empty when it can't be read.
pub fn read_tail(path: &Path, max_bytes: u64) -> String {
    let Ok(mut file) = File::open(path) else { return String::new() };
    let len = file.metadata().map(|m| m.len()).unwrap_or(0);
    let start = len.saturating_sub(max_bytes);
    if file.seek(SeekFrom::Start(start)).is_err() {
        return String::new();
    }
    let mut bytes = Vec::new();
    let _ = file.read_to_end(&mut bytes);
    let text = String::from_utf8_lossy(&bytes).into_owned();
    if start > 0 {
        // Drop the partial first line.
        match text.find('\n') {
            Some(i) => text[i + 1..].to_string(),
            None => String::new(),
        }
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_tail_starts_at_a_whole_line() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.log");
        fs::write(&path, "first line\nsecond line\nthird\n").unwrap();
        assert_eq!(read_tail(&path, 1000), "first line\nsecond line\nthird\n");
        assert_eq!(read_tail(&path, 15), "third\n");
        assert_eq!(read_tail(&dir.path().join("missing"), 10), "");
    }
}
