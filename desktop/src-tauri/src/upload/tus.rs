//! Resumable uploads over the tus 1.0.0 protocol (the server mounts @tus/server at
//! `/api/uploads`). The file is read one chunk at a time, never whole.

use std::io::SeekFrom;
use std::path::Path;
use std::time::Duration;

use base64::Engine;
use reqwest::header::{HeaderMap, CONTENT_TYPE, LOCATION};
use reqwest::{Response, StatusCode};
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt};

use super::api::{error_from, network, ApiClient, ApiError};

const TUS_VERSION: &str = "1.0.0";

pub const DEFAULT_CHUNK_SIZE: u64 = 8 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum UploadError {
    #[error(transparent)]
    Api(#[from] ApiError),
    #[error("i/o: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, PartialEq)]
pub struct UploadOutcome {
    pub upload_id: String,
    pub upload_url: String,
}

#[derive(Debug, Clone)]
pub struct TusOptions {
    pub chunk_size: u64,
    pub backoff: Vec<Duration>,
}

impl Default for TusOptions {
    fn default() -> Self {
        TusOptions { chunk_size: DEFAULT_CHUNK_SIZE, backoff: [1, 2, 4, 8, 16].map(Duration::from_secs).to_vec() }
    }
}

/// Uploads `file`, resuming `(upload_url, offset)` from a previous run when given.
/// `progress(sent, total)` fires after every chunk; `persist(upload_url, offset)` after every
/// accepted chunk so a restarted app can pick the upload back up.
pub async fn upload(
    client: &ApiClient,
    token: &str,
    file: &Path,
    resume: Option<(String, u64)>,
    progress: impl Fn(u64, u64),
    persist: impl Fn(&str, u64),
) -> Result<UploadOutcome, UploadError> {
    upload_with(client, token, file, resume, progress, persist, &TusOptions::default()).await
}

/// What went wrong in one step, as far as the retry loop cares.
enum Fail {
    /// Network trouble or a 5xx: ask the server where we are and carry on, after a backoff.
    Retry(String),
    /// The upload no longer exists on the server (or isn't our file): start a fresh one.
    Gone,
    Fatal(UploadError),
}

impl From<ApiError> for Fail {
    fn from(e: ApiError) -> Self {
        match e {
            ApiError::Network(d) => Fail::Retry(d),
            ApiError::Server { status, .. } if status >= 500 => Fail::Retry(format!("server answered {status}")),
            e => Fail::Fatal(e.into()),
        }
    }
}

enum Progress {
    Done,
    Advanced,
    /// A step that moved nothing forward (a HEAD): doesn't reset the retry budget.
    Idle,
}

struct State {
    url: Option<String>,
    offset: u64,
    need_head: bool,
}

pub async fn upload_with(
    client: &ApiClient,
    token: &str,
    file: &Path,
    resume: Option<(String, u64)>,
    progress: impl Fn(u64, u64),
    persist: impl Fn(&str, u64),
    opts: &TusOptions,
) -> Result<UploadOutcome, UploadError> {
    let mut handle = File::open(file).await?;
    let total = handle.metadata().await?.len();
    let filename = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "clip.mp4".into());
    let chunk_size = opts.chunk_size.max(1);

    let mut st = match resume {
        Some((url, offset)) => State { url: Some(url), offset, need_head: true },
        None => State { url: None, offset: 0, need_head: false },
    };
    let mut failures = 0usize;
    let mut restarted = false;

    loop {
        let step = step(client, token, &mut handle, total, &filename, chunk_size, &mut st, &progress, &persist).await;
        match step {
            Ok(Progress::Done) => break,
            Ok(Progress::Advanced) => failures = 0,
            Ok(Progress::Idle) => {}
            Err(Fail::Fatal(e)) => return Err(e),
            Err(Fail::Gone) => {
                if restarted {
                    return Err(ApiError::Server { status: 404, code: "upload_expired".into() }.into());
                }
                restarted = true;
                st = State { url: None, offset: 0, need_head: false };
            }
            Err(Fail::Retry(detail)) => {
                let Some(wait) = opts.backoff.get(failures) else {
                    return Err(ApiError::Network(detail).into());
                };
                tokio::time::sleep(*wait).await;
                failures += 1;
                st.need_head = st.url.is_some();
            }
        }
    }
    let url = st.url.expect("a finished upload has a URL");
    let upload_id = upload_id_of(&url);
    Ok(UploadOutcome { upload_id, upload_url: url })
}

/// The server's finalize expects the id tus put last in the upload URL.
fn upload_id_of(url: &str) -> String {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    path.trim_end_matches('/').rsplit('/').next().unwrap_or_default().to_string()
}

#[allow(clippy::too_many_arguments)]
async fn step(
    client: &ApiClient,
    token: &str,
    handle: &mut File,
    total: u64,
    filename: &str,
    chunk_size: u64,
    st: &mut State,
    progress: &impl Fn(u64, u64),
    persist: &impl Fn(&str, u64),
) -> Result<Progress, Fail> {
    let Some(url) = st.url.clone() else {
        let url = create(client, token, total, filename).await?;
        persist(&url, 0);
        st.url = Some(url);
        st.offset = 0;
        return Ok(if total == 0 { Progress::Done } else { Progress::Advanced });
    };
    if st.need_head {
        st.offset = head(client, token, &url, total).await?;
        st.need_head = false;
        progress(st.offset, total);
        return Ok(if st.offset >= total { Progress::Done } else { Progress::Idle });
    }
    let len = chunk_size.min(total - st.offset) as usize;
    let mut chunk = vec![0u8; len];
    handle.seek(SeekFrom::Start(st.offset)).await.map_err(|e| Fail::Fatal(e.into()))?;
    handle.read_exact(&mut chunk).await.map_err(|e| Fail::Fatal(e.into()))?;
    let new_offset = patch(client, token, &url, st.offset, chunk).await?;
    if new_offset > total || new_offset <= st.offset {
        return Err(ApiError::Server { status: 204, code: "bad_offset".into() }.into());
    }
    st.offset = new_offset;
    persist(&url, new_offset);
    progress(new_offset, total);
    Ok(if new_offset >= total { Progress::Done } else { Progress::Advanced })
}

fn offset_header(headers: &HeaderMap, status: StatusCode) -> Result<u64, Fail> {
    headers
        .get("upload-offset")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| ApiError::Server { status: status.as_u16(), code: "bad_response".into() }.into())
}

/// Maps an unexpected response: 404/410 means the upload is gone; everything else per `ApiError`.
async fn unexpected(resp: Response) -> Fail {
    match resp.status() {
        StatusCode::NOT_FOUND | StatusCode::GONE => Fail::Gone,
        // Offset mismatch: our idea of the offset is stale, so re-sync with a HEAD.
        StatusCode::CONFLICT => Fail::Retry("offset mismatch".into()),
        _ => error_from(resp).await.into(),
    }
}

async fn create(client: &ApiClient, token: &str, total: u64, filename: &str) -> Result<String, Fail> {
    let metadata = format!("filename {}", base64::engine::general_purpose::STANDARD.encode(filename));
    let resp = client
        .http
        .post(client.url("/api/uploads"))
        .bearer_auth(token)
        .header("Tus-Resumable", TUS_VERSION)
        .header("Upload-Length", total)
        .header("Upload-Metadata", metadata)
        .send()
        .await
        .map_err(|e| Fail::from(network(e)))?;
    if resp.status() != StatusCode::CREATED {
        return Err(match unexpected(resp).await {
            // A 404 on create is a wrong address, not an expired upload.
            Fail::Gone => ApiError::Server { status: 404, code: "not_found".into() }.into(),
            other => other,
        });
    }
    let location = resp
        .headers()
        .get(LOCATION)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| ApiError::Server { status: 201, code: "bad_response".into() })?;
    Ok(client.resolve(location)?)
}

async fn head(client: &ApiClient, token: &str, url: &str, total: u64) -> Result<u64, Fail> {
    let resp = client
        .http
        .head(url)
        .bearer_auth(token)
        .header("Tus-Resumable", TUS_VERSION)
        .send()
        .await
        .map_err(|e| Fail::from(network(e)))?;
    if !resp.status().is_success() {
        return Err(unexpected(resp).await);
    }
    // A different length means the clip file changed since this upload began.
    let length: Option<u64> = resp.headers().get("upload-length").and_then(|v| v.to_str().ok()).and_then(|v| v.parse().ok());
    if length.is_some_and(|l| l != total) {
        return Err(Fail::Gone);
    }
    let offset = offset_header(resp.headers(), resp.status())?;
    if offset > total {
        return Err(Fail::Gone);
    }
    Ok(offset)
}

async fn patch(client: &ApiClient, token: &str, url: &str, offset: u64, chunk: Vec<u8>) -> Result<u64, Fail> {
    let resp = client
        .http
        .patch(url)
        .bearer_auth(token)
        .header("Tus-Resumable", TUS_VERSION)
        .header("Upload-Offset", offset)
        .header(CONTENT_TYPE, "application/offset+octet-stream")
        .body(chunk)
        .send()
        .await
        .map_err(|e| Fail::from(network(e)))?;
    if !resp.status().is_success() {
        return Err(unexpected(resp).await);
    }
    offset_header(resp.headers(), resp.status())
}
