use std::sync::Mutex;
use std::time::Duration;

use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

use super::api::*;
use super::auth::{self, Auth};
use super::tus::{self, TusOptions, UploadError};
use crate::errors::AppError;

const BODY: &[u8] = b"0123456789abcdefghij"; // 20 bytes: chunks of 8 -> 8, 8, 4

fn clip_json() -> serde_json::Value {
    serde_json::json!({
        "id": "abcdefghijkl", "title": "Nice", "gameName": null, "steamAppId": null, "visibility": "feed",
        "durationMs": 1000, "width": 1920, "height": 1080, "sizeBytes": 20,
        "createdAt": "2026-01-01T00:00:00.000Z", "owner": {"username": "adam"},
        "shareUrl": "https://x/c/abcdefghijkl", "videoUrl": "https://x/media/abcdefghijkl.mp4",
        "thumbnailUrl": "https://x/media/abcdefghijkl.jpg"
    })
}

fn fast(chunk: u64) -> TusOptions {
    TusOptions { chunk_size: chunk, backoff: vec![Duration::from_millis(5); 5] }
}

fn file() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("clip.mp4");
    std::fs::write(&p, BODY).unwrap();
    (dir, p)
}

fn offset_of(r: &Request) -> u64 {
    r.headers.get("upload-offset").unwrap().to_str().unwrap().parse().unwrap()
}

fn created(server: &MockServer) -> Mock {
    Mock::given(method("POST")).and(path("/api/uploads")).and(header("upload-length", "20")).and(header("tus-resumable", "1.0.0")).and(header("authorization", "Bearer tok")).respond_with(
        ResponseTemplate::new(201).insert_header("Location", format!("{}/api/uploads/up1", server.uri()).as_str()),
    )
}

fn patch_ok(from: u64, to: u64) -> Mock {
    Mock::given(method("PATCH"))
        .and(path("/api/uploads/up1"))
        .and(header("upload-offset", from.to_string().as_str()))
        .and(header("tus-resumable", "1.0.0"))
        .and(header("content-type", "application/offset+octet-stream"))
        .respond_with(ResponseTemplate::new(204).insert_header("Upload-Offset", to.to_string().as_str()))
}

fn req() -> FinalizeRequest {
    FinalizeRequest {
        upload_id: "up1".into(),
        title: "Nice".into(),
        game_name: Some("".into()),
        steam_app_id: None,
        visibility: Visibility::Feed,
    }
}

#[tokio::test]
async fn happy_path_uploads_in_chunks_then_finalizes() {
    let server = MockServer::start().await;
    created(&server).expect(1).mount(&server).await;
    patch_ok(0, 8).expect(1).mount(&server).await;
    patch_ok(8, 16).expect(1).mount(&server).await;
    patch_ok(16, 20).expect(1).mount(&server).await;
    Mock::given(method("POST"))
        .and(path("/api/clips"))
        .and(header("authorization", "Bearer tok"))
        .and(body_json(serde_json::json!({
            "uploadId": "up1", "title": "Nice", "gameName": null, "steamAppId": null, "visibility": "feed"
        })))
        .respond_with(ResponseTemplate::new(201).set_body_json(clip_json()))
        .expect(1)
        .mount(&server)
        .await;

    let (_d, f) = file();
    let client = ApiClient::new(&server.uri()).unwrap();
    let progress = Mutex::new(vec![]);
    let persisted = Mutex::new(vec![]);
    let out = tus::upload_with(
        &client, "tok", &f, None,
        |a, b| progress.lock().unwrap().push((a, b)),
        |u, o| persisted.lock().unwrap().push((u.to_string(), o)),
        &fast(8),
    )
    .await
    .unwrap();
    assert_eq!(out.upload_id, "up1");
    assert_eq!(*progress.lock().unwrap(), vec![(8, 20), (16, 20), (20, 20)]);
    let persisted = persisted.lock().unwrap();
    assert_eq!(persisted.last().unwrap().1, 20);
    assert!(persisted.iter().all(|(u, _)| u.ends_with("/api/uploads/up1")));

    let clip = client.finalize("tok", &req()).await.unwrap();
    assert_eq!(clip.id, "abcdefghijkl");
}

#[tokio::test]
async fn relative_location_is_resolved_against_the_base_url() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).and(path("/api/uploads"))
        .respond_with(ResponseTemplate::new(201).insert_header("Location", "/api/uploads/up1"))
        .mount(&server).await;
    patch_ok(0, 20).expect(1).mount(&server).await;
    let (_d, f) = file();
    let client = ApiClient::new(&server.uri()).unwrap();
    let out = tus::upload_with(&client, "tok", &f, None, |_, _| {}, |_, _| {}, &fast(64)).await.unwrap();
    assert_eq!(out.upload_id, "up1");
    assert_eq!(out.upload_url, format!("{}/api/uploads/up1", server.uri()));
}

#[tokio::test]
async fn dropped_connection_resumes_from_the_offset_the_head_reports() {
    let server = MockServer::start().await;
    created(&server).expect(1).mount(&server).await;
    patch_ok(0, 8).expect(1).mount(&server).await;
    // Chunk 2 hangs past the client timeout once (the connection "drops"), though the server
    // did receive it: HEAD says 16, so the next PATCH must start at 16, not at 8.
    Mock::given(method("PATCH")).and(path("/api/uploads/up1")).and(header("upload-offset", "8"))
        .respond_with(ResponseTemplate::new(204).insert_header("Upload-Offset", "16").set_delay(Duration::from_millis(600)))
        .up_to_n_times(1).expect(1).with_priority(1).mount(&server).await;
    Mock::given(method("HEAD")).and(path("/api/uploads/up1"))
        .respond_with(ResponseTemplate::new(200).insert_header("Upload-Offset", "16").insert_header("Upload-Length", "20"))
        .expect(1).mount(&server).await;
    patch_ok(16, 20).expect(1).mount(&server).await;

    let (_d, f) = file();
    let client = ApiClient::with_timeout(&server.uri(), Duration::from_millis(150)).unwrap();
    let out = tus::upload_with(&client, "tok", &f, None, |_, _| {}, |_, _| {}, &fast(8)).await.unwrap();
    assert_eq!(out.upload_id, "up1");

    let reqs = server.received_requests().await.unwrap();
    let patches: Vec<u64> = reqs.iter().filter(|r| r.method.as_str() == "PATCH").map(offset_of).collect();
    assert_eq!(patches, vec![0, 8, 16]);
    let last = reqs.iter().rfind(|r| r.method.as_str() == "PATCH").unwrap();
    assert_eq!(last.body, &BODY[16..]);
}

#[tokio::test]
async fn a_5xx_on_patch_is_retried_via_head() {
    let server = MockServer::start().await;
    created(&server).mount(&server).await;
    Mock::given(method("PATCH")).and(header("upload-offset", "0")).respond_with(ResponseTemplate::new(503))
        .up_to_n_times(1).with_priority(1).mount(&server).await;
    Mock::given(method("HEAD")).respond_with(ResponseTemplate::new(200).insert_header("Upload-Offset", "0")).expect(1).mount(&server).await;
    patch_ok(0, 20).expect(1).mount(&server).await;
    let (_d, f) = file();
    let client = ApiClient::new(&server.uri()).unwrap();
    tus::upload_with(&client, "tok", &f, None, |_, _| {}, |_, _| {}, &fast(64)).await.unwrap();
}

#[tokio::test]
async fn gives_up_with_network_after_five_retries() {
    let server = MockServer::start().await;
    created(&server).mount(&server).await;
    Mock::given(method("PATCH")).respond_with(ResponseTemplate::new(503)).expect(6).mount(&server).await;
    Mock::given(method("HEAD")).respond_with(ResponseTemplate::new(200).insert_header("Upload-Offset", "0")).mount(&server).await;
    let (_d, f) = file();
    let client = ApiClient::new(&server.uri()).unwrap();
    let err = tus::upload_with(&client, "tok", &f, None, |_, _| {}, |_, _| {}, &fast(64)).await.unwrap_err();
    assert!(matches!(err, UploadError::Api(ApiError::Network(_))), "{err:?}");
}

#[tokio::test]
async fn unauthorized_patch_fails_immediately_without_retries() {
    let server = MockServer::start().await;
    created(&server).mount(&server).await;
    Mock::given(method("PATCH")).respond_with(ResponseTemplate::new(401)).expect(1).mount(&server).await;
    Mock::given(method("HEAD")).respond_with(ResponseTemplate::new(200)).expect(0).mount(&server).await;
    let (_d, f) = file();
    let client = ApiClient::new(&server.uri()).unwrap();
    let err = tus::upload_with(&client, "tok", &f, None, |_, _| {}, |_, _| {}, &fast(8)).await.unwrap_err();
    assert!(matches!(err, UploadError::Api(ApiError::Unauthorized)), "{err:?}");
}

#[tokio::test]
async fn resume_continues_from_the_head_offset_not_the_saved_one() {
    let server = MockServer::start().await;
    Mock::given(method("HEAD")).and(path("/api/uploads/up1"))
        .respond_with(ResponseTemplate::new(200).insert_header("Upload-Offset", "16").insert_header("Upload-Length", "20"))
        .expect(1).mount(&server).await;
    patch_ok(16, 20).expect(1).mount(&server).await;
    let (_d, f) = file();
    let client = ApiClient::new(&server.uri()).unwrap();
    let url = format!("{}/api/uploads/up1", server.uri());
    let out = tus::upload_with(&client, "tok", &f, Some((url, 8)), |_, _| {}, |_, _| {}, &fast(8)).await.unwrap();
    assert_eq!(out.upload_id, "up1");
}

#[tokio::test]
async fn expired_upload_on_resume_starts_a_fresh_one() {
    let server = MockServer::start().await;
    Mock::given(method("HEAD")).and(path("/api/uploads/old")).respond_with(ResponseTemplate::new(404)).expect(1).mount(&server).await;
    created(&server).expect(1).mount(&server).await;
    patch_ok(0, 20).expect(1).mount(&server).await;
    let (_d, f) = file();
    let client = ApiClient::new(&server.uri()).unwrap();
    let url = format!("{}/api/uploads/old", server.uri());
    let out = tus::upload_with(&client, "tok", &f, Some((url, 8)), |_, _| {}, |_, _| {}, &fast(64)).await.unwrap();
    assert_eq!(out.upload_id, "up1");
}

#[tokio::test]
async fn finalize_409_finalizing_then_200_succeeds() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).and(path("/api/clips"))
        .respond_with(ResponseTemplate::new(409).set_body_json(serde_json::json!({"error": "upload_finalizing"})))
        .up_to_n_times(2).with_priority(1).expect(2).mount(&server).await;
    Mock::given(method("POST")).and(path("/api/clips")).respond_with(ResponseTemplate::new(200).set_body_json(clip_json())).expect(1).mount(&server).await;
    let client = ApiClient::new(&server.uri()).unwrap();
    let opts = FinalizeOptions { poll_interval: Duration::from_millis(5), max_wait: Duration::from_secs(5) };
    assert_eq!(client.finalize_with("tok", &req(), opts).await.unwrap().id, "abcdefghijkl");
}

#[tokio::test]
async fn finalize_gives_up_when_still_finalizing_after_max_wait_and_other_409s_are_server_errors() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).and(path("/api/clips"))
        .respond_with(ResponseTemplate::new(409).set_body_json(serde_json::json!({"error": "upload_finalizing"})))
        .mount(&server).await;
    let client = ApiClient::new(&server.uri()).unwrap();
    let opts = FinalizeOptions { poll_interval: Duration::from_millis(5), max_wait: Duration::from_millis(40) };
    let err = client.finalize_with("tok", &req(), opts).await.unwrap_err();
    assert_eq!(err, ApiError::Server { status: 409, code: "upload_finalizing".into() });

    let server = MockServer::start().await;
    Mock::given(method("POST")).and(path("/api/clips"))
        .respond_with(ResponseTemplate::new(409).set_body_json(serde_json::json!({"error": "upload_incomplete"})))
        .expect(1).mount(&server).await;
    let client = ApiClient::new(&server.uri()).unwrap();
    let err = client.finalize("tok", &req()).await.unwrap_err();
    assert_eq!(err, ApiError::Server { status: 409, code: "upload_incomplete".into() });
}

#[tokio::test]
async fn finalize_422_is_invalid_video_with_the_server_message() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).and(path("/api/clips"))
        .respond_with(ResponseTemplate::new(422).set_body_json(serde_json::json!({"error": "corrupt_video", "message": "That video file is damaged."})))
        .mount(&server).await;
    let client = ApiClient::new(&server.uri()).unwrap();
    let err = client.finalize("tok", &req()).await.unwrap_err();
    assert_eq!(err, ApiError::InvalidVideo("That video file is damaged.".into()));
}

#[tokio::test]
async fn login_sends_desktop_client_and_returns_the_session() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).and(path("/api/auth/login"))
        .and(body_json(serde_json::json!({"username": "adam", "password": "pw", "client": "desktop", "deviceName": "box"})))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"token": "tok", "me": {"id": "1", "username": "adam", "isAdmin": true}})))
        .expect(1).mount(&server).await;
    Mock::given(method("GET")).and(path("/api/me")).and(header("authorization", "Bearer tok"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"id": "1", "username": "adam", "isAdmin": true})))
        .mount(&server).await;
    let client = ApiClient::new(&format!("{}/", server.uri())).unwrap();
    let s = client.login("adam", "pw", "box").await.unwrap();
    assert_eq!((s.token.as_str(), s.me.username.as_str()), ("tok", "adam"));
    assert!(client.me("tok").await.unwrap().is_admin);
}

#[tokio::test]
async fn login_401_is_unauthorized() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).and(path("/api/auth/login"))
        .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({"error": "invalid_credentials"})))
        .mount(&server).await;
    let client = ApiClient::new(&server.uri()).unwrap();
    assert_eq!(client.login("a", "b", "c").await.unwrap_err(), ApiError::Unauthorized);
}

#[tokio::test]
async fn unreachable_server_is_a_network_error() {
    let client = ApiClient::new("http://127.0.0.1:1").unwrap();
    assert!(matches!(client.login("a", "b", "c").await, Err(ApiError::Network(_))));
}

#[test]
fn api_errors_map_to_friendly_app_errors() {
    let cases = [
        (ApiError::Unauthorized, "unauthorized", "Your session has expired. Please log in again."),
        (ApiError::Network("x".into()), "network", "Can't reach the Klipt server. Check your connection and try again."),
        (ApiError::Server { status: 500, code: "boom".into() }, "server_error", "The server had a problem. Try again in a moment."),
        (ApiError::InvalidVideo("m".into()), "invalid_video", "The server couldn't read this video. It may be damaged."),
    ];
    for (api, code, message) in cases {
        let app = AppError::from(api);
        assert_eq!((app.code(), app.user_message().as_str()), (code, message));
    }
}

#[test]
fn auth_round_trips_and_clears() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(auth::load(dir.path()), None);
    let a = Auth { server_url: "https://clips.example.com".into(), token: "t".into(), username: "adam".into() };
    auth::save(dir.path(), &a).unwrap();
    auth::save(dir.path(), &a).unwrap(); // overwrite works
    assert_eq!(auth::load(dir.path()), Some(a));
    let raw: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.path().join("auth.json")).unwrap()).unwrap();
    assert_eq!(raw["serverUrl"], "https://clips.example.com");
    auth::clear(dir.path()).unwrap();
    auth::clear(dir.path()).unwrap();
    assert_eq!(auth::load(dir.path()), None);
}

#[cfg(unix)]
#[test]
fn auth_file_is_private() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let a = Auth { server_url: "u".into(), token: "t".into(), username: "n".into() };
    auth::save(dir.path(), &a).unwrap();
    let mode = std::fs::metadata(dir.path().join("auth.json")).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[tokio::test]
async fn get_json_sends_the_token_and_encoded_query() {
    use wiremock::matchers::query_param;
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/clips"))
        .and(header("authorization", "Bearer tok"))
        .and(query_param("scope", "feed"))
        .and(query_param("game", "A & B"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"items": [], "nextCursor": null})))
        .expect(1)
        .mount(&server)
        .await;
    let client = ApiClient::new(&server.uri()).unwrap();
    let body = client
        .get_json("tok", "/api/clips", &[("scope", "feed".into()), ("game", "A & B".into())])
        .await
        .unwrap();
    assert_eq!(body["nextCursor"], serde_json::Value::Null);
    Mock::given(method("GET")).and(path("/api/games")).respond_with(ResponseTemplate::new(401)).mount(&server).await;
    assert_eq!(client.get_json("tok", "/api/games", &[]).await.unwrap_err(), ApiError::Unauthorized);
}
