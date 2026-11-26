//! A read-ahead cache for clips played from the server.
//!
//! WebKitGTK's media pipeline reads a video in ~1 MB range requests, one after another, each paying
//! a round trip to the server (~0.3 s through the home tunnel), so a clip arrives at ~3 MB/s and
//! GStreamer waits seconds before it plays. Instead the player loads server clips from the local
//! media server, which asks this cache: the first open of a clip downloads the whole file in ONE
//! stream into `<app cache>/player/`, and range requests are answered from that file as it fills.
//! Finished files stay (up to [`PLAYER_CACHE_MAX_BYTES`], oldest dropped first), so re-opening a
//! clip is instant. The URL is versioned (a replaced video gets a new one), so it is the key.

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant, SystemTime};

/// The most the cache keeps on disk.
pub const PLAYER_CACHE_MAX_BYTES: u64 = 1024 * 1024 * 1024;
/// A server that sends nothing for this long fails the download (the next open retries).
const STALL_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone)]
pub struct PlayerCache {
    inner: Arc<Inner>,
}

struct Inner {
    dir: PathBuf,
    max_bytes: u64,
    downloads: Mutex<HashMap<String, Arc<Download>>>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Phase {
    Running,
    Done,
    Failed,
}

struct Progress {
    total: Option<u64>,
    written: u64,
    phase: Phase,
}

/// One clip's download (running, finished, or read from disk).
pub struct Download {
    part: PathBuf,
    done: PathBuf,
    progress: Mutex<Progress>,
    changed: Condvar,
    /// Another clip was opened: stop (only one clip plays at a time, and a download left running
    /// would slow the one being watched).
    cancelled: AtomicBool,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A stable file name for a URL (FNV-1a, 64 bit).
fn file_key(url: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in url.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}")
}

impl PlayerCache {
    pub fn new(dir: PathBuf, max_bytes: u64) -> PlayerCache {
        PlayerCache { inner: Arc::new(Inner { dir, max_bytes, downloads: Mutex::default() }) }
    }

    /// The running or finished download for `url`, starting one if there is none (or the last one
    /// failed, or its file was evicted).
    /// Other clips' running downloads are cancelled: one clip plays at a time.
    pub fn open(&self, url: &str) -> Arc<Download> {
        let mut downloads = lock(&self.inner.downloads);
        for (other, d) in downloads.iter() {
            if other != url && lock(&d.progress).phase == Phase::Running {
                d.cancelled.store(true, Ordering::SeqCst);
            }
        }
        if let Some(d) = downloads.get(url) {
            let phase = lock(&d.progress).phase;
            if phase == Phase::Running || (phase == Phase::Done && d.done.is_file()) {
                return d.clone();
            }
        }
        let key = file_key(url);
        let part = self.inner.dir.join(format!("{key}.part"));
        let done = self.inner.dir.join(format!("{key}.mp4"));
        if let Ok(meta) = std::fs::metadata(&done) {
            // Kept from before: mark it used so eviction drops it last.
            let _ = File::options().write(true).open(&done).and_then(|f| f.set_modified(SystemTime::now()));
            let d = Arc::new(Download {
                part,
                done,
                progress: Mutex::new(Progress { total: Some(meta.len()), written: meta.len(), phase: Phase::Done }),
                changed: Condvar::new(),
                cancelled: AtomicBool::new(false),
            });
            downloads.insert(url.to_string(), d.clone());
            return d;
        }
        let d = Arc::new(Download {
            part,
            done,
            progress: Mutex::new(Progress { total: None, written: 0, phase: Phase::Running }),
            changed: Condvar::new(),
            cancelled: AtomicBool::new(false),
        });
        downloads.insert(url.to_string(), d.clone());
        drop(downloads);
        let (cache, url, task) = (self.clone(), url.to_string(), d.clone());
        tauri::async_runtime::spawn(async move {
            let ok = match fetch(&url, &task).await {
                Ok(()) => true,
                Err(e) => {
                    if !task.cancelled.load(Ordering::SeqCst) {
                        crate::elog!("klipt: player cache download failed: {e}");
                    }
                    false
                }
            };
            task.finish(ok);
            if ok {
                cache.evict();
            } else {
                let mut downloads = lock(&cache.inner.downloads);
                if downloads.get(&url).is_some_and(|d| Arc::ptr_eq(d, &task)) {
                    downloads.remove(&url);
                }
            }
        });
        d
    }

    /// Deletes finished files, oldest first, until the cache holds at most `max_bytes`, and
    /// leftovers of downloads that never finished (a crash or quit mid-download).
    pub fn evict(&self) {
        let running: Vec<PathBuf> = lock(&self.inner.downloads)
            .values()
            .filter(|d| lock(&d.progress).phase == Phase::Running)
            .map(|d| d.part.clone())
            .collect();
        let Ok(entries) = std::fs::read_dir(&self.inner.dir) else { return };
        let mut kept: Vec<(SystemTime, u64, PathBuf)> = Vec::new();
        for path in entries.flatten().map(|e| e.path()) {
            match path.extension().and_then(|e| e.to_str()) {
                Some("part") if !running.contains(&path) => {
                    let _ = std::fs::remove_file(&path);
                }
                Some("mp4") => {
                    if let Ok(meta) = std::fs::metadata(&path) {
                        kept.push((meta.modified().unwrap_or(SystemTime::UNIX_EPOCH), meta.len(), path));
                    }
                }
                _ => {}
            }
        }
        kept.sort();
        let mut total: u64 = kept.iter().map(|(_, len, _)| len).sum();
        for (_, len, path) in kept {
            if total <= self.inner.max_bytes {
                break;
            }
            if std::fs::remove_file(&path).is_ok() {
                total -= len;
            }
        }
    }
}

/// An HTTP client for server videos: gives up on a server that stalls.
pub(crate) fn http_client() -> Result<reqwest::Client, String> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(STALL_TIMEOUT)
        .build()
        .map_err(|e| e.to_string())
}

/// Streams `url` into the download's `.part` file, then renames it to its finished name.
async fn fetch(url: &str, d: &Download) -> Result<(), String> {
    let http = http_client()?;
    let mut res = http.get(url).send().await.map_err(|e| e.to_string())?;
    if !res.status().is_success() {
        return Err(format!("the server answered {}", res.status()));
    }
    let total = res.content_length().ok_or("the server sent no length")?;
    if let Some(dir) = d.part.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let mut file = File::create(&d.part).map_err(|e| e.to_string())?;
    d.update(|p| p.total = Some(total));
    let mut written = 0u64;
    while let Some(chunk) = res.chunk().await.map_err(|e| e.to_string())? {
        file.write_all(&chunk).map_err(|e| e.to_string())?;
        written += chunk.len() as u64;
        d.update(|p| p.written = written);
        if d.cancelled.load(Ordering::SeqCst) {
            return Err("cancelled: another clip was opened".into());
        }
    }
    if written != total {
        return Err(format!("cut off after {written} of {total} bytes"));
    }
    file.sync_all().map_err(|e| e.to_string())?;
    drop(file);
    std::fs::rename(&d.part, &d.done).map_err(|e| e.to_string())
}

impl Download {
    fn update(&self, f: impl FnOnce(&mut Progress)) {
        f(&mut lock(&self.progress));
        self.changed.notify_all();
    }

    fn finish(&self, ok: bool) {
        self.update(|p| p.phase = if ok { Phase::Done } else { Phase::Failed });
        if !ok {
            let _ = std::fs::remove_file(&self.part);
        }
    }

    /// Waits until `ready` holds, the download fails, or `timeout` passes.
    fn wait(&self, timeout: Duration, ready: impl Fn(&Progress) -> bool) -> io::Result<MutexGuard<'_, Progress>> {
        let deadline = Instant::now() + timeout;
        let mut p = lock(&self.progress);
        loop {
            if ready(&p) {
                return Ok(p);
            }
            if p.phase == Phase::Failed {
                return Err(io::Error::other("the download failed"));
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(io::Error::new(io::ErrorKind::TimedOut, "the download is too slow"));
            }
            p = self.changed.wait_timeout(p, left).unwrap_or_else(PoisonError::into_inner).0;
        }
    }

    /// The file's full size, once the server has answered.
    pub fn wait_total(&self, timeout: Duration) -> io::Result<u64> {
        Ok(self.wait(timeout, |p| p.total.is_some())?.total.unwrap_or(0))
    }

    /// Waits until the first `end_exclusive` bytes are on disk (or the whole file, if it's shorter).
    pub fn wait_for(&self, end_exclusive: u64, timeout: Duration) -> io::Result<()> {
        self.wait(timeout, |p| p.written >= end_exclusive || p.phase == Phase::Done).map(|_| ())
    }

    /// How far it has got: bytes on disk, and whether it's finished.
    pub fn written(&self) -> (u64, bool) {
        let p = lock(&self.progress);
        (p.written, p.phase == Phase::Done)
    }

    /// The file to read from (still being written, or finished).
    pub fn path(&self) -> PathBuf {
        if lock(&self.progress).phase == Phase::Done { self.done.clone() } else { self.part.clone() }
    }

    /// Opens the file to read from. Safe while the download finishes: an open file keeps working
    /// after the rename, and a rename between choosing the name and opening it is retried.
    pub fn open_file(&self) -> io::Result<File> {
        File::open(self.path()).or_else(|_| File::open(&self.done)).or_else(|_| File::open(&self.part))
    }
}


#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::SystemTime;

    const T: Duration = Duration::from_secs(20);

    pub(crate) fn body(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i % 251) as u8).collect()
    }

    #[derive(Clone, Copy)]
    pub(crate) enum Answer {
        Body,
        Status500,
        CutOffHalfway,
    }

    /// A slow HTTP server for one file: answers request N with `answers[N]` (the last repeats),
    /// writing 64 KB every 5 ms. Returns its base URL and the request count.
    pub(crate) fn serve(len: usize, answers: Vec<Answer>) -> (String, Arc<AtomicUsize>) {
        serve_at(len, answers, Duration::from_millis(5))
    }

    /// [`serve`] with `delay` between 64 KB writes. Answers `Range: bytes=a-b` / `bytes=a-` with a
    /// 206 and that slice.
    pub(crate) fn serve_at(len: usize, answers: Vec<Answer>, delay: Duration) -> (String, Arc<AtomicUsize>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let url = format!("http://127.0.0.1:{}/media/clip.v1.mp4", listener.local_addr().unwrap().port());
        let count = Arc::new(AtomicUsize::new(0));
        let seen = count.clone();
        std::thread::spawn(move || {
            let data = body(len);
            for stream in listener.incoming().flatten() {
                let n = seen.fetch_add(1, Ordering::SeqCst);
                let answer = answers[n.min(answers.len() - 1)];
                let data = data.clone();
                std::thread::spawn(move || {
                    let mut reader = BufReader::new(stream.try_clone().unwrap());
                    let mut line = String::new();
                    let mut range: Option<(usize, usize)> = None;
                    while reader.read_line(&mut line).is_ok_and(|n| n > 2) {
                        if let Some(v) = line.to_ascii_lowercase().strip_prefix("range: bytes=") {
                            let (a, b) = v.trim().split_once('-').unwrap();
                            let end = if b.is_empty() { len - 1 } else { b.parse().unwrap() };
                            range = Some((a.parse().unwrap(), end));
                        }
                        line.clear();
                    }
                    let mut out = stream;
                    if let Answer::Status500 = answer {
                        let _ = out.write_all(b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                        return;
                    }
                    let (from, to) = range.unwrap_or((0, len - 1));
                    let head = match range {
                        Some(_) => format!(
                            "HTTP/1.1 206 Partial Content\r\nContent-Type: video/mp4\r\nContent-Length: {}\r\nContent-Range: bytes {from}-{to}/{len}\r\nConnection: close\r\n\r\n",
                            to - from + 1
                        ),
                        None => format!("HTTP/1.1 200 OK\r\nContent-Type: video/mp4\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n"),
                    };
                    if out.write_all(head.as_bytes()).is_err() {
                        return;
                    }
                    let send = if let Answer::CutOffHalfway = answer { (to - from + 1) / 2 } else { to - from + 1 };
                    for chunk in data[from..from + send].chunks(64 * 1024) {
                        if out.write_all(chunk).is_err() {
                            return;
                        }
                        std::thread::sleep(delay);
                    }
                });
            }
        });
        (url, count)
    }

    fn finished_files(dir: &std::path::Path) -> Vec<PathBuf> {
        std::fs::read_dir(dir)
            .map(|r| r.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "mp4")).collect())
            .unwrap_or_default()
    }

    #[test]
    fn downloads_once_and_serves_ranges_as_they_arrive() {
        let dir = tempfile::tempdir().unwrap();
        let (url, count) = serve(3_000_000, vec![Answer::Body]);
        let cache = PlayerCache::new(dir.path().to_path_buf(), PLAYER_CACHE_MAX_BYTES);
        let a = cache.open(&url);
        let b = cache.open(&url);
        assert!(Arc::ptr_eq(&a, &b));
        assert_eq!(a.wait_total(T).unwrap(), 3_000_000);
        a.wait_for(3_000_000, T).unwrap();
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut a.open_file().unwrap(), &mut bytes).unwrap();
        assert_eq!(bytes, body(3_000_000));
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn wait_for_ahead_of_the_download_blocks_until_bytes_arrive() {
        let dir = tempfile::tempdir().unwrap();
        let (url, _) = serve(3_000_000, vec![Answer::Body]);
        let cache = PlayerCache::new(dir.path().to_path_buf(), PLAYER_CACHE_MAX_BYTES);
        let d = cache.open(&url);
        d.wait_for(2_000_000, T).unwrap();
        assert!(d.open_file().unwrap().metadata().unwrap().len() >= 2_000_000);
    }

    #[test]
    fn a_failed_download_errors_waiters_and_retries_next_open() {
        let dir = tempfile::tempdir().unwrap();
        let (url, count) = serve(500_000, vec![Answer::Status500, Answer::Body]);
        let cache = PlayerCache::new(dir.path().to_path_buf(), PLAYER_CACHE_MAX_BYTES);
        assert!(cache.open(&url).wait_total(T).is_err());
        assert!(finished_files(dir.path()).is_empty());
        let again = cache.open(&url);
        again.wait_for(500_000, T).unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn a_cut_off_download_is_not_kept_as_finished() {
        let dir = tempfile::tempdir().unwrap();
        let (url, _) = serve(1_000_000, vec![Answer::CutOffHalfway]);
        let cache = PlayerCache::new(dir.path().to_path_buf(), PLAYER_CACHE_MAX_BYTES);
        let d = cache.open(&url);
        assert!(d.wait_for(1_000_000, T).is_err());
        assert!(finished_files(dir.path()).is_empty());
    }

    #[test]
    fn opening_another_clip_cancels_the_running_download() {
        let dir = tempfile::tempdir().unwrap();
        let (first, _) = serve(3_000_000, vec![Answer::Body]);
        let (second, _) = serve(300_000, vec![Answer::Body]);
        let cache = PlayerCache::new(dir.path().to_path_buf(), PLAYER_CACHE_MAX_BYTES);
        let a = cache.open(&first);
        a.wait_for(100_000, T).unwrap();
        let b = cache.open(&second);
        // The clip left behind stops downloading (it would slow the one being watched).
        assert!(a.wait_for(3_000_000, T).is_err());
        b.wait_for(300_000, T).unwrap();
        let deadline = std::time::Instant::now() + T;
        let parts = || {
            std::fs::read_dir(dir.path()).unwrap().flatten().filter(|e| e.path().extension().is_some_and(|x| x == "part")).count()
        };
        while parts() > 0 && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(parts(), 0, "the cancelled download's partial file is gone");
    }

    #[test]
    fn evict_drops_oldest_finished_files_first() {
        let dir = tempfile::tempdir().unwrap();
        let now = SystemTime::now();
        for (i, name) in ["old.mp4", "mid.mp4", "new.mp4"].iter().enumerate() {
            let path = dir.path().join(name);
            std::fs::write(&path, vec![0u8; 400_000]).unwrap();
            let when = now - Duration::from_secs(300 - 100 * i as u64);
            std::fs::File::options().write(true).open(&path).unwrap().set_modified(when).unwrap();
        }
        PlayerCache::new(dir.path().to_path_buf(), 900_000).evict();
        let mut left: Vec<String> =
            finished_files(dir.path()).iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect();
        left.sort();
        assert_eq!(left, ["mid.mp4", "new.mp4"]);
    }

    #[test]
    fn a_finished_file_survives_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let (url, count) = serve(400_000, vec![Answer::Body]);
        let first = PlayerCache::new(dir.path().to_path_buf(), PLAYER_CACHE_MAX_BYTES);
        first.open(&url).wait_for(400_000, T).unwrap();
        // The rename to the finished name happens right after the last byte.
        let deadline = std::time::Instant::now() + T;
        while finished_files(dir.path()).is_empty() && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        let second = PlayerCache::new(dir.path().to_path_buf(), PLAYER_CACHE_MAX_BYTES);
        let d = second.open(&url);
        assert_eq!(d.wait_total(T).unwrap(), 400_000);
        d.wait_for(400_000, T).unwrap();
        let mut bytes = Vec::new();
        std::io::Read::read_to_end(&mut d.open_file().unwrap(), &mut bytes).unwrap();
        assert_eq!(bytes, body(400_000));
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }
}
