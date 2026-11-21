//! A tiny HTTP server on 127.0.0.1 that streams clip files to the editor's `<video>`/`<audio>`.
//!
//! WebKitGTK's media pipeline (GStreamer) can't stream from Tauri's custom `asset://` scheme, so
//! the editor loads media from `http://127.0.0.1:<port>/<token>` instead. Only files registered
//! with [`MediaServer::url_for`] are served, each under a random unguessable token. Supports
//! `Range` requests so seeking works.

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

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
}

/// A registered file and how it's served.
#[derive(Clone)]
struct Served {
    path: PathBuf,
    /// Served as picture-only: the offsets of its audio tracks' handler types (see `mp4_mute`).
    video_only: Option<Arc<[u64]>>,
}

impl MediaServer {
    /// Binds to a random free port on loopback and serves in background threads.
    pub fn start() -> io::Result<MediaServer> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let port = listener.local_addr()?.port();
        let files: Arc<Mutex<HashMap<String, Served>>> = Arc::default();
        let shared = files.clone();
        let open = Arc::new(AtomicUsize::new(0));
        std::thread::Builder::new().name("klipt-media".into()).spawn(move || {
            for stream in listener.incoming().flatten() {
                if open.fetch_add(1, Ordering::SeqCst) >= MAX_CONNECTIONS {
                    open.fetch_sub(1, Ordering::SeqCst);
                    continue; // dropping the stream closes it
                }
                let (files, open) = (shared.clone(), open.clone());
                let spawned = std::thread::Builder::new().name("klipt-media-conn".into()).spawn(move || {
                    let _ = handle(stream, &files);
                    open.fetch_sub(1, Ordering::SeqCst);
                });
                if spawned.is_err() {
                    crate::elog!("klipt: can't start a media connection thread");
                }
            }
        })?;
        Ok(MediaServer { port, files })
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

    fn register(&self, path: &Path, video_only: bool) -> String {
        let mut files = self.files.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some((token, _)) =
            files.iter().find(|(_, s)| s.path.as_path() == path && s.video_only.is_some() == video_only)
        {
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
        files.insert(token.clone(), Served { path: path.to_path_buf(), video_only });
        format!("http://127.0.0.1:{}/{}", self.port, token)
    }
}

fn handle(stream: TcpStream, files: &Mutex<HashMap<String, Served>>) -> io::Result<()> {
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
    let Some(Served { path, video_only }) = served else { return respond_status(&mut out, "404 Not Found") };
    let Ok(mut file) = File::open(&path) else { return respond_status(&mut out, "404 Not Found") };
    let size = file.metadata()?.len();
    let mime = match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "m4a" => "audio/mp4",
        _ => "video/mp4",
    };

    let (status, start, end) = match range.as_deref().and_then(|r| parse_range(r, size)) {
        Some((s, e)) => ("206 Partial Content", s, e),
        None if range.is_some() && size > 0 => {
            let head = format!(
                "HTTP/1.1 416 Range Not Satisfiable\r\nContent-Range: bytes */{size}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            );
            return out.write_all(head.as_bytes());
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
        let server = MediaServer::start().unwrap();
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
        let server = MediaServer::start().unwrap();
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
        let server = MediaServer::start().unwrap();
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
