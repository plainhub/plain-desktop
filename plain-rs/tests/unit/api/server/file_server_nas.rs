//! Unit tests for the nas `/fs` behaviors merged into the shared file
//! server (moved from plain-nas): range handling, recent-file tracking,
//! codec probe and chat-attachment `fid:` resolution.
use crate::media::kv::recent;
use crate::server::ServerState;
use crate::server::test_support::{as_desktop, nas_state_with};
use axum::body::Body;
use axum::extract::Request;
use axum::http::{HeaderName, StatusCode, header};
use axum::response::Response;
use base64::Engine as _;

/// A test state plus the raw 32-byte url_token key used to mint file ids —
/// exactly the XChaCha20-Poly1305 nonce||ct||tag → base64 shape the web
/// client produces.
fn test_state() -> (ServerState, Vec<u8>) {
    let key = [7u8; 32];
    let state = nas_state_with(|prefs| {
        prefs
            .set(
                "url_token",
                &base64::engine::general_purpose::STANDARD.encode(key),
            )
            .expect("set url_token");
    });
    (as_desktop(&state), key.to_vec())
}

fn file_id(key: &[u8], path: &str) -> String {
    let blob = crate::xchacha_encrypt_raw(key, path.as_bytes()).expect("encrypt");
    base64::engine::general_purpose::STANDARD.encode(blob)
}

/// A 10-byte file whose extension keeps it off the image/video special
/// paths, so `serve_file`'s range logic is what is under test.
fn temp_bin(key: &[u8]) -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("movie.bin");
    std::fs::write(&path, b"0123456789").unwrap();
    (dir, file_id(key, &path.to_string_lossy()))
}

async fn call_fs(state: &ServerState, query: &str, headers: &[(&HeaderName, &str)]) -> Response {
    let mut builder = Request::get(format!("/fs?{query}"));
    for (name, value) in headers {
        builder = builder.header(*name, *value);
    }
    let req = builder.body(Body::empty()).unwrap();
    crate::server::file_server::fs_handler(axum::extract::State(state.clone()), req).await
}

async fn body_bytes(resp: Response) -> Vec<u8> {
    axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .expect("body")
        .to_vec()
}

fn range_header(v: &str) -> (&'static HeaderName, &str) {
    (&header::RANGE, v)
}

#[tokio::test]
async fn fs_range_serves_206_with_content_range() {
    let (state, key) = test_state();
    let (_dir, id) = temp_bin(&key);
    let resp = call_fs(&state, &format!("id={id}"), &[range_header("bytes=2-5")]).await;
    assert_eq!(resp.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(
        resp.headers().get(header::CONTENT_RANGE).unwrap(),
        "bytes 2-5/10"
    );
    assert_eq!(resp.headers().get(header::CONTENT_LENGTH).unwrap(), "4");
    assert_eq!(resp.headers().get(header::ACCEPT_RANGES).unwrap(), "bytes");
    assert_eq!(body_bytes(resp).await, b"2345");
}

#[tokio::test]
async fn fs_range_open_end_and_suffix() {
    let (state, key) = test_state();
    let (_dir, id) = temp_bin(&key);
    for (spec, expected) in [("bytes=7-", b"789".as_ref()), ("bytes=-2", b"89".as_ref())] {
        let resp = call_fs(&state, &format!("id={id}"), &[range_header(spec)]).await;
        assert_eq!(resp.status(), StatusCode::PARTIAL_CONTENT, "{spec}");
        assert_eq!(body_bytes(resp).await, expected, "{spec}");
    }
}

#[tokio::test]
async fn fs_unsatisfiable_range_is_416() {
    // RFC 7233 §4.4 — the shape Ktor serves for plain-app. The old
    // behavior answered 200 with the whole file, which forces browsers
    // to re-download from byte 0.
    let (state, key) = test_state();
    let (_dir, id) = temp_bin(&key);
    for spec in ["bytes=100-200", "bytes=10-", "bytes=-0"] {
        let resp = call_fs(&state, &format!("id={id}"), &[range_header(spec)]).await;
        assert_eq!(resp.status(), StatusCode::RANGE_NOT_SATISFIABLE, "{spec}");
        assert_eq!(
            resp.headers().get(header::CONTENT_RANGE).unwrap(),
            "bytes */10",
            "{spec}"
        );
        assert!(body_bytes(resp).await.is_empty(), "{spec}");
    }
}

#[tokio::test]
async fn fs_malformed_or_absent_range_serves_200_full() {
    let (state, key) = test_state();
    let (_dir, id) = temp_bin(&key);
    for spec in ["bytes=zzz", "items=0-9", "bytes=5-2"] {
        let resp = call_fs(&state, &format!("id={id}"), &[range_header(spec)]).await;
        assert_eq!(resp.status(), StatusCode::OK, "{spec}");
        assert_eq!(body_bytes(resp).await, b"0123456789", "{spec}");
    }
    let resp = call_fs(&state, &format!("id={id}"), &[]).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.headers().get(header::ACCEPT_RANGES).unwrap(), "bytes");
    assert_eq!(body_bytes(resp).await, b"0123456789");
}

#[tokio::test]
async fn fs_recent_tracked_once_per_view_not_per_chunk() {
    // Browsers fetch video in many range chunks; only the view-start
    // request (no Range, or byte 0) may rewrite the 500-entry recent
    // list. Continuation chunks must stay off that path.
    let (state, key) = test_state();
    let (_dir, id) = temp_bin(&key);

    // Continuation chunk only: nothing recorded.
    call_fs(&state, &format!("id={id}"), &[range_header("bytes=4-9")]).await;
    assert!(recent::get_recent_files(&state.ctx.prefs, 10).is_empty());

    // View start (range at byte 0) records once…
    call_fs(&state, &format!("id={id}"), &[range_header("bytes=0-9")]).await;
    assert_eq!(recent::get_recent_files(&state.ctx.prefs, 10).len(), 1);

    // …and so does a plain full GET (no Range header).
    call_fs(&state, &format!("id={id}"), &[]).await;
    assert_eq!(recent::get_recent_files(&state.ctx.prefs, 10).len(), 1);
}

#[tokio::test]
async fn fs_probe_reports_codec_json() {
    let (state, key) = test_state();
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/testdata/video-h264-high.mp4");
    let id = file_id(&key, path);
    let resp = call_fs(&state, &format!("id={id}&probe=1"), &[]).await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/json"
    );
    let body = body_bytes(resp).await;
    assert_eq!(body, br#"{"codec":"avc1"}"#.as_ref());
}

/// Chat attachment ids encrypt the `fid:{sha256}.{ext}` URI; the /fs
/// handler resolves them into the content-addressed store under the data
/// dir. Clients build these ids themselves from `content` uris.
#[tokio::test]
async fn fs_serves_fid_uris_from_app_file_store() {
    let (state, key) = test_state();

    // Import a file into the chat state's app-file store.
    let imported = crate::chat::app_file_store::import_bytes(
        &state.ctx.chat.service.db,
        &state.ctx.chat.service.data_dir,
        b"fid-payload-bytes",
        "text/plain",
    )
    .expect("import");
    assert!(imported.fid_suffix.ends_with(".txt"));

    let uri = format!("fid:{}", imported.fid_suffix);
    let id = file_id(&key, &uri);

    let resp = call_fs(&state, &format!("id={id}"), &[]).await;
    assert_eq!(resp.status(), axum::http::StatusCode::OK);
    assert_eq!(body_bytes(resp).await, b"fid-payload-bytes");
}
