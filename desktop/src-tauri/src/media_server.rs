//! A tiny HTTP server on 127.0.0.1 that streams clip files to the editor's `<video>`/`<audio>`.
//!
//! WebKitGTK's media pipeline (GStreamer) can't stream from Tauri's custom `asset://` scheme, so
//! the editor loads media from `http://127.0.0.1:<port>/<token>` instead. Only files registered
//! with [`MediaServer::url_for`] are served, each under a random unguessable token. Supports
//! `Range` requests so seeking works. Clips on the server can be served too, through the
//! read-ahead cache ([`MediaServer::url_for_remote`], see `player_cache`).

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::player_cache::PlayerCache;

/// The longest a request's line and headers may be. The editor's requests are a few hundred bytes.
const MAX_HEAD_BYTES: u64 = 8 * 1024;
/// More connections than the editor ever opens at once (a video and two audio tracks).
const MAX_CONNECTIONS: usize = 32;
/// A client that sends nothing for this long is dropped.
const READ_TIMEOUT: Duration = Duration::from_secs(10);
/// A client that stops reading for this long is dropped. Generous: a paused player may stop
/// reading for a while; if it's dropped, it asks again with a Range when it resumes.
const WRITE_TIMEOUT: Duration = Duration::from_secs(120);

/// Origins of Klipt's own window (Linux, Windows, `pnpm tauri dev`). The editor's `<video>` loads
/// with `crossorigin` (its frame is drawn on a canvas), so these get CORS; other web pages don't.
const APP_ORIGINS: &[&str] =
    &["tauri://localhost", "http://tauri.localhost", "https://tauri.localhost", "http://localhost:1420"];

#[derive(Clone)]
pub struct MediaServer {
    port: u16,
    files: Arc<Mutex<HashMap<String, Served>>>,
    /// Open connections (read by tests to see that a gone player's thread ended).
    #[cfg_attr(not(test), allow(dead_code))]
    open: Arc<AtomicUsize>,
}

/// What a token serves.
#[derive(Clone)]
enum Served {
    /// A file on this PC.
    File {
        path: PathBuf,
        /// Served as picture-only: the offsets of its audio tracks' handler types (see `mp4_mute`).
        video_only: Option<Arc<[u64]>>,
    },
    /// A clip on the server, through the read-ahead cache.
    Remote { url: String },
}

/// How long a request waits for the cache: the server's first answer, or the next piece.
const REMOTE_WAIT: Duration = Duration::from_secs(30);
/// Remote clips are copied out of the cache in pieces this big, each once it has arrived.
const REMOTE_PIECE: u64 = 256 * 1024;
/// A range starting this far past what the cache has downloaded (a seek far into a long
/// recording) is fetched from the server on its own instead of waiting for the download.
const PASS_THROUGH_AHEAD: u64 = 8 * 1024 * 1024;

impl MediaServer {
    /// Binds to a random free port on loopback and serves in background threads.
    pub fn start(cache: PlayerCache) -> io::Result<MediaServer> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let port = listener.local_addr()?.port();
        let files: Arc<Mutex<HashMap<String, Served>>> = Arc::default();
        let shared = files.clone();
        let open = Arc::new(AtomicUsize::new(0));
        let counter = open.clone();
        std::thread::Builder::new().name("klipt-media".into()).spawn(move || {
            for stream in listener.incoming().flatten() {
                if open.fetch_add(1, Ordering::SeqCst) >= MAX_CONNECTIONS {
                    open.fetch_sub(1, Ordering::SeqCst);
                    continue; // dropping the stream closes it
                }
                let (files, open, cache) = (shared.clone(), open.clone(), cache.clone());
                let spawned = std::thread::Builder::new().name("klipt-media-conn".into()).spawn(move || {
                    let _ = handle(stream, &files, &cache);
                    open.fetch_sub(1, Ordering::SeqCst);
                });
                if spawned.is_err() {
                    crate::elog!("klipt: can't start a media connection thread");
                }
            }
        })?;
        Ok(MediaServer { port, files, open: counter })
    }

    /// Registers `path` and returns the URL it is served at. The same path keeps the same URL.
    pub fn url_for(&self, path: &Path) -> String {
        self.register(path, false)
    }

    /// Like [`MediaServer::url_for`], but served without sound: its audio tracks show up as a
    /// type players skip (see `mp4_mute`), so a player opens no audio stream for it. The file on
    /// disk is untouched. If its tracks can't be read, it's served as it is.
    pub fn url_for_video_only(&self, path: &Path) -> String {
        self.register(path, true)
    }

    /// A URL for a clip on the server, played through the read-ahead cache (see `player_cache`).
    /// The same URL keeps the same local URL.
    pub fn url_for_remote(&self, url: &str) -> String {
        let mut files = self.files.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let known = files.iter().find(|(_, s)| matches!(s, Served::Remote { url: u } if u == url));
        let token = match known {
            Some((token, _)) => token.clone(),
            None => {
                let token = uuid::Uuid::new_v4().simple().to_string();
                files.insert(token.clone(), Served::Remote { url: url.to_string() });
                token
            }
        };
        format!("http://127.0.0.1:{}/{}", self.port, token)
    }

    #[cfg(test)]
    fn open_connections(&self) -> usize {
        self.open.load(Ordering::SeqCst)
    }

    fn register(&self, path: &Path, video_only: bool) -> String {
        let mut files = self.files.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some((token, _)) = files.iter().find(|(_, s)| {
            matches!(s, Served::File { path: p, video_only: v } if p.as_path() == path && v.is_some() == video_only)
        }) {
            return format!("http://127.0.0.1:{}/{}", self.port, token);
        }
        let video_only = video_only.then(|| {
            let offsets = File::open(path).and_then(|mut f| crate::mp4_mute::audio_handler_offsets(&mut f));
            Arc::from(offsets.unwrap_or_else(|e| {
                crate::elog!("klipt: can't read the tracks of {}, serving it with sound: {e}", path.display());
                Vec::new()
            }))
        });
        let token = uuid::Uuid::new_v4().simple().to_string();
        files.insert(token.clone(), Served::File { path: path.to_path_buf(), video_only });
        format!("http://127.0.0.1:{}/{}", self.port, token)
    }
}

fn handle(stream: TcpStream, files: &Mutex<HashMap<String, Served>>, cache: &PlayerCache) -> io::Result<()> {
    stream.set_read_timeout(Some(READ_TIMEOUT))?;
    stream.set_write_timeout(Some(WRITE_TIMEOUT))?;
    // A head longer than the limit reads as cut short, so it never grows without bound.
    let mut reader = BufReader::new(stream.try_clone()?.take(MAX_HEAD_BYTES));
    let mut out = stream;
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let target = parts.next().unwrap_or("").to_string();
    let mut range: Option<String> = None;
    let mut origin: Option<String> = None;
    loop {
        let mut h = String::new();
        if reader.read_line(&mut h)? == 0 || h == "\r\n" || h == "\n" {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            if k.trim().eq_ignore_ascii_case("range") {
                range = Some(v.trim().to_string());
            } else if k.trim().eq_ignore_ascii_case("origin") {
                origin = Some(v.trim().to_string());
            }
        }
    }
    if method != "GET" && method != "HEAD" {
        return respond_status(&mut out, "405 Method Not Allowed");
    }
    let token = target.trim_start_matches('/').split('?').next().unwrap_or("");
    let served = files.lock().unwrap_or_else(std::sync::PoisonError::into_inner).get(token).cloned();
    let (path, video_only) = match served {
        None => return respond_status(&mut out, "404 Not Found"),
        Some(Served::Remote { url }) => return serve_remote(out, &method, range, origin, cache, &url),
        Some(Served::File { path, video_only }) => (path, video_only),
    };
    let Ok(mut file) = File::open(&path) else { return respond_status(&mut out, "404 Not Found") };
    let size = file.metadata()?.len();
    let mime = match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "m4a" => "audio/mp4",
        _ => "video/mp4",
    };

    let Some((start, len)) = write_head(&mut out, range.as_deref(), origin, size, mime)? else { return Ok(()) };
    if method == "HEAD" || len == 0 {
        return Ok(());
    }
    file.seek(SeekFrom::Start(start))?;
    match video_only.filter(|offsets| !offsets.is_empty()) {
        None => {
            io::copy(&mut file.take(len), &mut out)?;
        }
        Some(offsets) => {
            // Chunk by chunk, with the audio tracks' handler types swapped as they go by.
            let mut file = file.take(len);
            let mut buf = vec![0u8; 64 * 1024];
            let mut at = start;
            loop {
                let n = file.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                crate::mp4_mute::apply(&mut buf[..n], at, &offsets);
                out.write_all(&buf[..n])?;
                at += n as u64;
            }
        }
    }
    out.flush()
}

/// Writes the response head for `size` bytes (and `range`, if any). Returns the first byte and
/// length of the body to send, or None once a 416 went out.
fn write_head(
    out: &mut TcpStream,
    range: Option<&str>,
    origin: Option<String>,
    size: u64,
    mime: &str,
) -> io::Result<Option<(u64, u64)>> {
    let (status, start, end) = match range.and_then(|r| parse_range(r, size)) {
        Some((s, e)) => ("206 Partial Content", s, e),
        None if range.is_some() && size > 0 => {
            let head = format!(
                "HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */{size}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            );
            out.write_all(head.as_bytes())?;
            return Ok(None);
        }
        None => ("200 OK", 0, size.saturating_sub(1)),
    };
    let len = if size == 0 { 0 } else { end - start + 1 };
    let mut head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {mime}\r\nAccept-Ranges: bytes\r\nContent-Length: {len}\r\nConnection: close\r\nVary: Origin\r\n"
    );
    if let Some(origin) = origin.filter(|o| APP_ORIGINS.contains(&o.as_str())) {
        head.push_str(&format!("Access-Control-Allow-Origin: {origin}\r\n"));
    }
    if status.starts_with("206") {
        head.push_str(&format!("Content-Range: bytes {start}-{end}/{size}\r\n"));
    }
    head.push_str("\r\n");
    out.write_all(head.as_bytes())?;
    Ok(Some((start, len)))
}

/// Serves a server clip from the read-ahead cache, each piece as soon as it has arrived.
fn serve_remote(
    mut out: TcpStream,
    method: &str,
    range: Option<String>,
    origin: Option<String>,
    cache: &PlayerCache,
    url: &str,
) -> io::Result<()> {
    let download = cache.open(url);
    let Ok(size) = download.wait_total(REMOTE_WAIT) else { return respond_status(&mut out, "502 Bad Gateway") };
    let Some((start, len)) = write_head(&mut out, range.as_deref(), origin, size, "video/mp4")? else { return Ok(()) };
    if method == "HEAD" || len == 0 {
        return Ok(());
    }
    let end = start + len; // exclusive
    let (written, done) = download.written();
    if !done && start > written + PASS_THROUGH_AHEAD {
        return pass_through(out, url, start, end);
    }
    let mut file: Option<File> = None;
    let mut at = start;
    let mut buf = vec![0u8; REMOTE_PIECE as usize];
    while at < end {
        let upto = (at + REMOTE_PIECE).min(end);
        download.wait_for(upto, REMOTE_WAIT)?;
        if file.is_none() {
            file = Some(download.open_file()?);
        }
        let f = file.as_mut().expect("opened above");
        f.seek(SeekFrom::Start(at))?;
        let piece = &mut buf[..(upto - at) as usize];
        f.read_exact(piece)?;
        out.write_all(piece)?; // fails once the player has gone: the thread ends
        at = upto;
    }
    out.flush()
}

/// Copies bytes `start..end` of `url` straight from the server (one ranged request), for a range
/// the cache won't have for a while. The head has been sent already.
fn pass_through(mut out: TcpStream, url: &str, start: u64, end: u64) -> io::Result<()> {
    let url = url.to_string();
    tauri::async_runtime::block_on(async move {
        let http = crate::player_cache::http_client().map_err(io::Error::other)?;
        let mut res = http
            .get(&url)
            .header("Range", format!("bytes={start}-{}", end - 1))
            .send()
            .await
            .map_err(io::Error::other)?;
        if res.status() != reqwest::StatusCode::PARTIAL_CONTENT {
            return Err(io::Error::other(format!("the server answered {} to a range", res.status())));
        }
        let mut left = end - start;
        while left > 0 {
            let Some(chunk) = res.chunk().await.map_err(io::Error::other)? else { break };
            let take = chunk.len().min(left as usize);
            out.write_all(&chunk[..take])?; // fails once the player has gone
            left -= take as u64;
        }
        if left > 0 {
            return Err(io::Error::other("the server's answer was cut short"));
        }
        out.flush()
    })
}

fn respond_status(out: &mut TcpStream, status: &str) -> io::Result<()> {
    out.write_all(format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes())
}

/// Parses a single `bytes=a-b`, `bytes=a-` or `bytes=-n` range into inclusive offsets.
fn parse_range(header: &str, size: u64) -> Option<(u64, u64)> {
    let spec = header.strip_prefix("bytes=")?.split(',').next()?.trim();
    let (a, b) = spec.split_once('-')?;
    if size == 0 {
        return None;
    }
    let last = size - 1;
    let (start, end) = if a.is_empty() {
        let n: u64 = b.parse().ok()?;
        if n == 0 {
            return None;
        }
        (size.saturating_sub(n), last)
    } else {
        let s: u64 = a.parse().ok()?;
        let e: u64 = if b.is_empty() { last } else { b.parse::<u64>().ok()?.min(last) };
        (s, e)
    };
    (start <= end && start <= last).then_some((start, end))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player_cache::tests::{body, serve, serve_at, Answer};

    fn test_server() -> MediaServer {
        let dir = tempfile::tempdir().unwrap().keep();
        MediaServer::start(PlayerCache::new(dir, crate::player_cache::PLAYER_CACHE_MAX_BYTES)).unwrap()
    }

    /// The raw reply (head and body) to a GET of `url` with extra header lines.
    fn fetch(server: &MediaServer, url: &str, extra: &str) -> Vec<u8> {
        let path = url.trim_start_matches(&format!("http://127.0.0.1:{}", server.port));
        let mut s = TcpStream::connect(("127.0.0.1", server.port)).unwrap();
        write!(s, "GET {path} HTTP/1.1\r\nHost: x\r\n{extra}\r\n").unwrap();
        let mut reply = Vec::new();
        s.read_to_end(&mut reply).unwrap();
        reply
    }

    fn split(reply: &[u8]) -> (String, Vec<u8>) {
        let at = reply.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
        (String::from_utf8_lossy(&reply[..at]).into_owned(), reply[at..].to_vec())
    }

    #[test]
    fn serves_a_remote_clip_through_the_cache_with_ranges() {
        let (remote, count) = serve(3_000_000, vec![Answer::Body]);
        let server = test_server();
        let url = server.url_for_remote(&remote);
        assert_eq!(url, server.url_for_remote(&remote), "same URL keeps its local URL");
        let (head, data) = split(&fetch(&server, &url, "Range: bytes=1000000-1999999\r\n"));
        assert!(head.starts_with("HTTP/1.1 206"), "{head}");
        assert!(head.contains("Content-Range: bytes 1000000-1999999/3000000"), "{head}");
        assert_eq!(data, body(3_000_000)[1_000_000..2_000_000]);
        let (_, first) = split(&fetch(&server, &url, "Range: bytes=0-99\r\n"));
        assert_eq!(first, body(3_000_000)[..100]);
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_seek_far_ahead_of_the_download_is_answered_from_the_server_at_once() {
        // ~6 s to download all of it; a seek near the end mustn't wait for that.
        let len = 20_000_000;
        let (remote, count) = serve_at(len, vec![Answer::Body], Duration::from_millis(20));
        let server = test_server();
        let url = server.url_for_remote(&remote);
        let (_, first) = split(&fetch(&server, &url, "Range: bytes=0-99\r\n"));
        assert_eq!(first, body(len)[..100]);
        let asked = std::time::Instant::now();
        let (head, data) = split(&fetch(&server, &url, "Range: bytes=18000000-18099999\r\n"));
        assert!(asked.elapsed() < Duration::from_secs(2), "took {:?}", asked.elapsed());
        assert!(head.contains("Content-Range: bytes 18000000-18099999/20000000"), "{head}");
        assert_eq!(data, body(len)[18_000_000..18_100_000]);
        assert_eq!(count.load(Ordering::SeqCst), 2, "the cache's download plus one ranged request");
    }

    #[test]
    fn a_remote_clip_that_fails_upstream_answers_502() {
        let (remote, _) = serve(1000, vec![Answer::Status500]);
        let server = test_server();
        let (head, _) = split(&fetch(&server, &server.url_for_remote(&remote), ""));
        assert!(head.starts_with("HTTP/1.1 502"), "{head}");
    }

    #[test]
    fn a_client_that_goes_away_mid_wait_ends_its_thread() {
        let (remote, _) = serve(6_000_000, vec![Answer::Body]);
        let server = test_server();
        let url = server.url_for_remote(&remote);
        let path = url.trim_start_matches(&format!("http://127.0.0.1:{}", server.port)).to_string();
        {
            let mut s = TcpStream::connect(("127.0.0.1", server.port)).unwrap();
            write!(s, "GET {path} HTTP/1.1\r\nHost: x\r\nRange: bytes=5000000-\r\n\r\n").unwrap();
        } // closed right away
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        while server.open_connections() > 0 && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(server.open_connections(), 0);
    }

    #[test]
    fn parses_ranges() {
        assert_eq!(parse_range("bytes=0-", 100), Some((0, 99)));
        assert_eq!(parse_range("bytes=10-19", 100), Some((10, 19)));
        assert_eq!(parse_range("bytes=90-200", 100), Some((90, 99)));
        assert_eq!(parse_range("bytes=-10", 100), Some((90, 99)));
        assert_eq!(parse_range("bytes=100-", 100), None);
        assert_eq!(parse_range("items=0-1", 100), None);
    }

    #[test]
    fn serves_registered_files_with_ranges_only() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("a.mp4");
        std::fs::write(&f, b"0123456789").unwrap();
        let server = test_server();
        let url = server.url_for(&f);
        assert_eq!(url, server.url_for(&f), "same path keeps its URL");
        let path = url.trim_start_matches(&format!("http://127.0.0.1:{}", server.port)).to_string();

        let get = |p: &str, extra: &str| {
            let mut s = TcpStream::connect(("127.0.0.1", server.port)).unwrap();
            write!(s, "GET {p} HTTP/1.1\r\nHost: x\r\n{extra}\r\n").unwrap();
            let mut body = String::new();
            s.read_to_string(&mut body).unwrap();
            body
        };
        let full = get(&path, "");
        assert!(full.starts_with("HTTP/1.1 200"), "{full}");
        assert!(full.ends_with("0123456789"));
        let part = get(&path, "Range: bytes=2-4\r\n");
        assert!(part.starts_with("HTTP/1.1 206"), "{part}");
        assert!(part.contains("Content-Range: bytes 2-4/10"));
        assert!(part.ends_with("234"));
        assert!(get("/nope", "").starts_with("HTTP/1.1 404"));

        // CORS only for Klipt's own window, never for any web page.
        assert!(!full.contains("Access-Control-Allow-Origin"), "{full}");
        let app = get(&path, "Origin: tauri://localhost\r\n");
        assert!(app.contains("Access-Control-Allow-Origin: tauri://localhost\r\n"), "{app}");
        let web = get(&path, "Origin: https://evil.example\r\n");
        assert!(!web.contains("Access-Control-Allow-Origin"), "{web}");
    }

    #[test]
    fn a_video_only_url_serves_the_audio_tracks_as_unknown() {
        let fixture = crate::ffmpeg::tests::fixture();
        let original = std::fs::read(fixture).unwrap();
        let offsets = crate::mp4_mute::audio_handler_offsets(&mut File::open(fixture).unwrap()).unwrap();
        assert_eq!(offsets.len(), 2);
        let server = test_server();
        let plain = server.url_for(fixture);
        let muted = server.url_for_video_only(fixture);
        assert_ne!(plain, muted, "the same file, served two ways");
        assert_eq!(muted, server.url_for_video_only(fixture), "same path keeps its URL");

        let body = |url: &str, extra: &str| {
            let path = url.trim_start_matches(&format!("http://127.0.0.1:{}", server.port));
            let mut s = TcpStream::connect(("127.0.0.1", server.port)).unwrap();
            write!(s, "GET {path} HTTP/1.1\r\nHost: x\r\n{extra}\r\n").unwrap();
            let mut reply = Vec::new();
            s.read_to_end(&mut reply).unwrap();
            let split = reply.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
            reply[split..].to_vec()
        };
        assert_eq!(body(&plain, ""), original, "the plain URL is untouched");
        let mut expected = original.clone();
        crate::mp4_mute::apply(&mut expected, 0, &offsets);
        assert_eq!(body(&muted, ""), expected);
        for &at in &offsets {
            assert_eq!(&expected[at as usize..at as usize + 4], b"klpt");
        }
        // A range that cuts through a handler type gets just its part patched.
        let (from, to) = (offsets[0] + 2, offsets[0] + 9);
        let part = body(&muted, &format!("Range: bytes={from}-{to}\r\n"));
        assert_eq!(part, expected[from as usize..=to as usize]);
    }

    #[test]
    fn an_endless_header_line_is_cut_off() {
        let server = test_server();
        let mut s = TcpStream::connect(("127.0.0.1", server.port)).unwrap();
        // Far past the limit, with no newline: the server stops reading and answers.
        let junk = vec![b'a'; 64 * 1024];
        let _ = s.write_all(b"GET /");
        let _ = s.write_all(&junk);
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut reply = Vec::new();
        let _ = s.read_to_end(&mut reply);
        let reply = String::from_utf8_lossy(&reply);
        assert!(reply.starts_with("HTTP/1.1 404"), "{reply}");
    }
}
