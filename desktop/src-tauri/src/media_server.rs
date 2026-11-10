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
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct MediaServer {
    port: u16,
    files: Arc<Mutex<HashMap<String, PathBuf>>>,
}

impl MediaServer {
    /// Binds to a random free port on loopback and serves in background threads.
    pub fn start() -> io::Result<MediaServer> {
        let listener = TcpListener::bind(("127.0.0.1", 0))?;
        let port = listener.local_addr()?.port();
        let files: Arc<Mutex<HashMap<String, PathBuf>>> = Arc::default();
        let shared = files.clone();
        std::thread::Builder::new().name("klipt-media".into()).spawn(move || {
            for stream in listener.incoming().flatten() {
                let files = shared.clone();
                let _ = std::thread::Builder::new()
                    .name("klipt-media-conn".into())
                    .spawn(move || {
                        let _ = handle(stream, &files);
                    });
            }
        })?;
        Ok(MediaServer { port, files })
    }

    /// Registers `path` and returns the URL it is served at. The same path keeps the same URL.
    pub fn url_for(&self, path: &Path) -> String {
        let mut files = self.files.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let token = files
            .iter()
            .find(|(_, p)| p.as_path() == path)
            .map(|(t, _)| t.clone())
            .unwrap_or_else(|| {
                let t = uuid::Uuid::new_v4().simple().to_string();
                files.insert(t.clone(), path.to_path_buf());
                t
            });
        format!("http://127.0.0.1:{}/{}", self.port, token)
    }
}

fn handle(stream: TcpStream, files: &Mutex<HashMap<String, PathBuf>>) -> io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut out = stream;
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let target = parts.next().unwrap_or("").to_string();
    let mut range: Option<String> = None;
    loop {
        let mut h = String::new();
        if reader.read_line(&mut h)? == 0 || h == "\r\n" || h == "\n" {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            if k.trim().eq_ignore_ascii_case("range") {
                range = Some(v.trim().to_string());
            }
        }
    }
    if method != "GET" && method != "HEAD" {
        return respond_status(&mut out, "405 Method Not Allowed");
    }
    let token = target.trim_start_matches('/').split('?').next().unwrap_or("");
    let path = files.lock().unwrap_or_else(std::sync::PoisonError::into_inner).get(token).cloned();
    let Some(path) = path else { return respond_status(&mut out, "404 Not Found") };
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
        "HTTP/1.1 {status}\r\nContent-Type: {mime}\r\nAccept-Ranges: bytes\r\nContent-Length: {len}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n"
    );
    if status.starts_with("206") {
        head.push_str(&format!("Content-Range: bytes {start}-{end}/{size}\r\n"));
    }
    head.push_str("\r\n");
    out.write_all(head.as_bytes())?;
    if method == "HEAD" || len == 0 {
        return Ok(());
    }
    file.seek(SeekFrom::Start(start))?;
    io::copy(&mut file.take(len), &mut out)?;
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
    }
}
