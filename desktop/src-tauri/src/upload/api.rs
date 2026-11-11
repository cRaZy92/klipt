//! The Klipt server's JSON API (see `packages/shared/src/api.ts`): login, me, logout, finalize.

use std::time::Duration;

use reqwest::{Response, StatusCode};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ApiError {
    #[error("unauthorized")]
    Unauthorized,
    #[error("network: {0}")]
    Network(String),
    #[error("server answered {status} ({code})")]
    Server { status: u16, code: String },
    #[error("the server couldn't read the video: {0}")]
    InvalidVideo(String),
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Me {
    pub id: String,
    pub username: String,
    pub is_admin: bool,
}

#[derive(Clone, PartialEq, Deserialize)]
pub struct Session {
    pub token: String,
    pub me: Me,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session").field("token", &"<redacted>").field("me", &self.me).finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    Feed,
    Link,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FinalizeRequest {
    pub upload_id: String,
    pub title: String,
    /// `None` for an unknown game; serialized as `null`, never `""`.
    pub game_name: Option<String>,
    pub steam_app_id: Option<u32>,
    pub visibility: Visibility,
}

/// The server's `Clip`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerClip {
    pub id: String,
    pub title: String,
    pub game_name: Option<String>,
    pub steam_app_id: Option<u32>,
    pub visibility: Visibility,
    pub duration_ms: u64,
    pub width: u32,
    pub height: u32,
    pub size_bytes: u64,
    pub created_at: String,
    pub share_url: String,
    pub video_url: String,
    pub thumbnail_url: String,
}

/// How long finalize keeps retrying while the server is still processing the upload.
#[derive(Debug, Clone, Copy)]
pub struct FinalizeOptions {
    pub poll_interval: Duration,
    pub max_wait: Duration,
}

impl Default for FinalizeOptions {
    fn default() -> Self {
        FinalizeOptions { poll_interval: Duration::from_secs(3), max_wait: Duration::from_secs(10 * 60) }
    }
}

#[derive(Deserialize)]
struct ErrorBody {
    error: Option<String>,
    message: Option<String>,
}

/// Turns a non-success response into the matching `ApiError`.
pub(crate) async fn error_from(resp: Response) -> ApiError {
    let status = resp.status();
    let body: Option<ErrorBody> = resp.json().await.ok();
    error_from_parts(status, body.as_ref().and_then(|b| b.error.clone()), body.and_then(|b| b.message))
}

fn error_from_parts(status: StatusCode, code: Option<String>, message: Option<String>) -> ApiError {
    match status {
        StatusCode::UNAUTHORIZED => ApiError::Unauthorized,
        StatusCode::UNPROCESSABLE_ENTITY => {
            ApiError::InvalidVideo(message.or(code).unwrap_or_else(|| "invalid video".into()))
        }
        _ => ApiError::Server { status: status.as_u16(), code: code.unwrap_or_else(|| "unknown".into()) },
    }
}

pub(crate) fn network(e: reqwest::Error) -> ApiError {
    ApiError::Network(e.without_url().to_string())
}

#[derive(Clone)]
pub struct ApiClient {
    base: String,
    pub(crate) http: reqwest::Client,
}

impl ApiClient {
    pub fn new(base_url: &str) -> Result<ApiClient, ApiError> {
        // Generous: one tus chunk is sent as a single request.
        ApiClient::with_timeout(base_url, Duration::from_secs(120))
    }

    pub fn with_timeout(base_url: &str, timeout: Duration) -> Result<ApiClient, ApiError> {
        // rustls needs a crypto provider; `ring` avoids a C toolchain on Windows. Already
        // installed is fine.
        let _ = rustls::crypto::ring::default_provider().install_default();
        let base = base_url.trim().trim_end_matches('/').to_string();
        match reqwest::Url::parse(&base) {
            Ok(u) if matches!(u.scheme(), "http" | "https") => {}
            _ => return Err(ApiError::Network("invalid server address".into())),
        }
        let http = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(15).min(timeout))
            .timeout(timeout)
            .build()
            .map_err(network)?;
        Ok(ApiClient { base, http })
    }

    pub(crate) fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base)
    }

    /// Whether `url` is on this server (same scheme, host and port), so it may get our token.
    pub(crate) fn same_origin(&self, url: &str) -> bool {
        match (reqwest::Url::parse(&self.base), reqwest::Url::parse(url)) {
            (Ok(a), Ok(b)) => a.origin() == b.origin(),
            _ => false,
        }
    }

    /// Resolves a `Location` header (absolute or relative) against the server address.
    pub(crate) fn resolve(&self, location: &str) -> Result<String, ApiError> {
        reqwest::Url::parse(&format!("{}/", self.base))
            .and_then(|base| base.join(location))
            .map(String::from)
            .map_err(|_| ApiError::Server { status: 0, code: "bad_location".into() })
    }

    pub async fn login(&self, username: &str, password: &str, device_name: &str) -> Result<Session, ApiError> {
        let resp = self
            .http
            .post(self.url("/api/auth/login"))
            .json(&serde_json::json!({
                "username": username,
                "password": password,
                "client": "desktop",
                "deviceName": device_name,
            }))
            .send()
            .await
            .map_err(network)?;
        parse_json(resp).await
    }

    pub async fn me(&self, token: &str) -> Result<Me, ApiError> {
        let resp = self.http.get(self.url("/api/me")).bearer_auth(token).send().await.map_err(network)?;
        parse_json(resp).await
    }

    /// An authenticated GET of a JSON API path, returned as-is (the feed and game lists).
    pub async fn get_json(&self, token: &str, path: &str, query: &[(&str, String)]) -> Result<serde_json::Value, ApiError> {
        let mut url = reqwest::Url::parse(&self.url(path)).map_err(|_| ApiError::Network("invalid server address".into()))?;
        url.query_pairs_mut().extend_pairs(query.iter().map(|(k, v)| (*k, v.as_str())));
        let resp = self.http.get(url).bearer_auth(token).send().await.map_err(network)?;
        parse_json(resp).await
    }

    /// An authenticated PATCH of a JSON API path; returns the answer as-is.
    pub async fn patch_json(&self, token: &str, path: &str, body: &serde_json::Value) -> Result<serde_json::Value, ApiError> {
        let resp = self.http.patch(self.url(path)).bearer_auth(token).json(body).send().await.map_err(network)?;
        parse_json(resp).await
    }

    /// An authenticated POST with no body, for actions that answer 204 (counting a view).
    pub async fn post_empty(&self, token: &str, path: &str) -> Result<(), ApiError> {
        let resp = self.http.post(self.url(path)).bearer_auth(token).send().await.map_err(network)?;
        if resp.status().is_success() {
            Ok(())
        } else {
            Err(error_from(resp).await)
        }
    }

    /// An authenticated DELETE of an API path.
    pub async fn delete(&self, token: &str, path: &str) -> Result<(), ApiError> {
        let resp = self.http.delete(self.url(path)).bearer_auth(token).send().await.map_err(network)?;
        if resp.status().is_success() {
            Ok(())
        } else {
            Err(error_from(resp).await)
        }
    }

    /// Starts downloading `url`, which must be on this server; the caller reads the body.
    pub async fn get_media(&self, url: &str) -> Result<Response, ApiError> {
        if !self.same_origin(url) {
            return Err(ApiError::Network("the video isn't on this server".into()));
        }
        let resp = self.http.get(url).send().await.map_err(network)?;
        if resp.status().is_success() {
            Ok(resp)
        } else {
            Err(error_from(resp).await)
        }
    }

    pub async fn logout(&self, token: &str) -> Result<(), ApiError> {
        let resp = self.http.post(self.url("/api/auth/logout")).bearer_auth(token).send().await.map_err(network)?;
        if resp.status().is_success() {
            Ok(())
        } else {
            Err(error_from(resp).await)
        }
    }

    pub async fn finalize(&self, token: &str, req: &FinalizeRequest) -> Result<ServerClip, ApiError> {
        self.finalize_with(token, req, FinalizeOptions::default()).await
    }

    /// 201 (new clip) and 200 (a repeat finalize of the same upload) both give the clip; 409
    /// `upload_finalizing` means the server is still working on it, so wait and ask again.
    pub async fn finalize_with(
        &self,
        token: &str,
        req: &FinalizeRequest,
        opts: FinalizeOptions,
    ) -> Result<ServerClip, ApiError> {
        let mut req = req.clone();
        // The server rejects "" for a game; an unknown game is null.
        req.game_name = req.game_name.take().map(|g| g.trim().to_string()).filter(|g| !g.is_empty());
        let started = tokio::time::Instant::now();
        loop {
            let resp = self.http.post(self.url("/api/clips")).bearer_auth(token).json(&req).send().await.map_err(network)?;
            if resp.status() == StatusCode::CONFLICT {
                let err = error_from(resp).await;
                let finalizing = matches!(&err, ApiError::Server { code, .. } if code == "upload_finalizing");
                if !finalizing || started.elapsed() + opts.poll_interval > opts.max_wait {
                    return Err(err);
                }
                tokio::time::sleep(opts.poll_interval).await;
                continue;
            }
            return parse_json(resp).await;
        }
    }
}

async fn parse_json<T: serde::de::DeserializeOwned>(resp: Response) -> Result<T, ApiError> {
    if !resp.status().is_success() {
        return Err(error_from(resp).await);
    }
    let status = resp.status().as_u16();
    resp.json().await.map_err(|_| ApiError::Server { status, code: "bad_response".into() })
}
