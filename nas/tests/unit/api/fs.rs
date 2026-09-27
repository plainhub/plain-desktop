//! Unit tests for `src/api/fs.rs` — moved out-of-line; compiled
//! as the `tests` child module via `#[cfg(test)] #[path]` there.
use super::{FsQuery, fs_handler};
use crate::api::auth::AppState;
use crate::config::Config;
use crate::db::Db;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, HeaderName, StatusCode, header};
use axum::response::Response;
use base64::Engine as _;
use std::sync::Arc;

/// AppState plus the raw 32-byte url_token key used to mint file ids —
/// exactly the XChaCha20-Poly1305 nonce||ct||tag → base64 shape the web
/// client produces.
fn test_state() -> (AppState, Vec<u8>) {
    let dir = tempfile::tempdir().expect("temp dir");
    let db = Arc::new(Db::open(dir.path()).expect("temp db opens"));
    let data_dir = dir.path().to_path_buf();
    let prefs = Arc::new(crate::prefs::Prefs::load(&data_dir.join("prefs.json")).unwrap());
    std::mem::forget(dir); // the db handle must outlive the test
    let key = [7u8; 32];
    prefs
        .set(
            "url_token",
            &base64::engine::general_purpose::STANDARD.encode(key),
        )
        .expect("set url_token");
    let config = Arc::new(Config::parse("[server]\nhttp_port = 8080\n"));
    let chat = crate::test_support::chat_state(&data_dir);
    let schema = crate::gql::build_schema(
        db.clone(),
        prefs.clone(),
        config.clone(),
        data_dir,
        chat.clone(),
    );
    (
        AppState {
            chat,
            config,
            db,
            prefs,
            ws_hub: Arc::new(crate::ws_hub::WsHub::new()),
            cors: crate::api::cors::CorsPolicy::from_config(&Config::default()),
            schema,
            peer_schema: crate::gql::peer_schema::build_schema(),
        },
        key.to_vec(),
    )
}

fn file_id(key: &[u8], path: &str) -> String {
    let blob = crate::crypto::encrypt(key, path.as_bytes()).expect("encrypt");
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

async fn call_fs(
    state: &AppState,
    query: serde_json::Value,
    headers: &[(&HeaderName, &str)],
) -> Response {
    let q: FsQuery = serde_json::from_value(query).expect("valid FsQuery");
    let mut hm = HeaderMap::new();
    for (name, value) in headers {
        hm.insert(name.clone(), value.parse().unwrap());
    }
    fs_handler(State(state.clone()), Query(q), hm).await
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
    let resp = call_fs(
        &state,
        serde_json::json!({ "id": id }),
        &[range_header("bytes=2-5")],
    )
    .await;
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
        let resp = call_fs(
            &state,
            serde_json::json!({ "id": id }),
            &[range_header(spec)],
        )
        .await;
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
        let resp = call_fs(
            &state,
            serde_json::json!({ "id": id }),
            &[range_header(spec)],
        )
        .await;
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
        let resp = call_fs(
            &state,
            serde_json::json!({ "id": id }),
            &[range_header(spec)],
        )
        .await;
        assert_eq!(resp.status(), StatusCode::OK, "{spec}");
        assert_eq!(body_bytes(resp).await, b"0123456789", "{spec}");
    }
    let resp = call_fs(&state, serde_json::json!({ "id": id }), &[]).await;
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
    call_fs(
        &state,
        serde_json::json!({ "id": id }),
        &[range_header("bytes=4-9")],
    )
    .await;
    assert!(crate::db::recent::get_recent_files(&state.prefs, 10).is_empty());

    // View start (range at byte 0) records once…
    call_fs(
        &state,
        serde_json::json!({ "id": id }),
        &[range_header("bytes=0-9")],
    )
    .await;
    assert_eq!(
        crate::db::recent::get_recent_files(&state.prefs, 10).len(),
        1
    );

    // …and so does a plain full GET (no Range header).
    call_fs(&state, serde_json::json!({ "id": id }), &[]).await;
    assert_eq!(
        crate::db::recent::get_recent_files(&state.prefs, 10).len(),
        1
    );
}

#[tokio::test]
async fn fs_probe_reports_codec_json() {
    let (state, key) = test_state();
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../plain-rs/testdata/video-h264-high.mp4"
    );
    let id = file_id(&key, &path);
    let resp = call_fs(&state, serde_json::json!({ "id": id, "probe": "1" }), &[]).await;
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
    let imported = plain_rs::chat::app_file_store::import_bytes(
        &state.chat.service.db,
        &state.chat.service.data_dir,
        b"fid-payload-bytes",
        "text/plain",
    )
    .expect("import");
    assert!(imported.fid_suffix.ends_with(".txt"));

    let uri = format!("fid:{}", imported.fid_suffix);
    let blob = crate::crypto::encrypt(&key, uri.as_bytes()).expect("encrypt");
    let id = base64::engine::general_purpose::STANDARD.encode(blob);

    let resp = call_fs(&state, serde_json::json!({ "id": id }), &[]).await;
    assert_eq!(resp.status(), axum::http::StatusCode::OK);
    assert_eq!(body_bytes(resp).await, b"fid-payload-bytes");
}
