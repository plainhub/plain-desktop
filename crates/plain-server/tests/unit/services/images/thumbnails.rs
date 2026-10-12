//! Thumbnail production behind `/fs`. The `/files/thumbnail` endpoint is
//! gone, so the contract is asserted against the function that owns it:
//! concurrent requests for the same key coalesce into one decode, a changed
//! source invalidates the cache, the conditional request answers 304, and a
//! revoked authorization surfaces as a refusal rather than an empty image.

use crate::assembly::server::{ContentServer, ServerState};
use crate::services::images::thumbnails::Request;
use sha2::Digest;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

fn variant(base: &Request, center_crop: Option<bool>, if_none_match: Option<&str>) -> Request {
    Request {
        center_crop: center_crop.unwrap_or(base.center_crop),
        if_none_match: if_none_match.map(str::to_owned),
        ..Request {
            path: base.path.clone(),
            width: base.width,
            height: base.height,
            center_crop: base.center_crop,
            media_id: base.media_id.clone(),
            file_name: base.file_name.clone(),
            if_none_match: None,
        }
    }
}

fn request_for(path: &std::path::Path) -> Request {
    Request {
        path: path.to_str().unwrap().to_owned(),
        width: 80,
        height: 60,
        center_crop: true,
        media_id: String::new(),
        file_name: String::new(),
        if_none_match: None,
    }
}

async fn render(state: &ServerState, request: Request) -> (u16, axum::http::HeaderMap, Vec<u8>) {
    let response = crate::services::images::thumbnails::response(state, request).await;
    let status = response.status().as_u16();
    let headers = response.headers().clone();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap()
        .to_vec();
    (status, headers, bytes)
}

fn header<'a>(headers: &'a axum::http::HeaderMap, name: &str) -> String {
    headers.get(name).unwrap().to_str().unwrap().to_owned()
}

/// Answers every decode request by shipping the source bytes back through the
/// output token, exactly as a platform thumbnail service does.
fn decode_worker(
    state: &ServerState,
    port: u16,
    cache: std::path::PathBuf,
    source: std::path::PathBuf,
) {
    let state = state.clone();
    let (generation, mut requests) = state.host.connect();
    let worker_state = state.clone();
    tokio::spawn(async move {
        while let Some(request) = requests.recv().await {
            if request["method"] == "thumbnailAuthorize" {
                let _ = worker_state.host.reply(
                    generation,
                    serde_json::json!({"id":request["id"],"result":cache.to_str().unwrap()}),
                );
                continue;
            }
            let params = &request["params"];
            let token = params["outputToken"].as_str().unwrap().to_owned();
            let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
            bytes.extend(std::fs::read(&source).unwrap());
            bytes.extend(params.to_string().as_bytes());
            let url = format!("http://127.0.0.1:{port}/files/thumbnail/output/{token}");
            let posted = reqwest::Client::new()
                .post(url)
                .bearer_auth(&plain_rs::utils::base64::base64_encode(&[4; 32]))
                .body(bytes)
                .send()
                .await;
            assert_eq!(
                posted.map(|response| response.status().as_u16()).ok(),
                Some(204),
                "the decode output has to reach the waiting reader"
            );
            let _ = state.host.reply(
                generation,
                serde_json::json!({"id":request["id"],"result":true}),
            );
        }
    });
}

fn server_with_cache() -> (tempfile::TempDir, ContentServer, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let prefs = Arc::new(plain_rs::prefs::Prefs::load(&dir.path().join("system.json")).unwrap());
    prefs.set("client_id", "thumb-fixture").unwrap();
    let cache = dir.path().join("cache");
    let core = ContentServer::start(
        &dir.path().join("plain.db"),
        &plain_rs::utils::base64::base64_encode(&[4; 32]),
        prefs,
    )
    .unwrap();
    (dir, core, cache)
}

#[tokio::test]
async fn concurrent_requests_for_one_key_decode_once() {
    let dir = server_with_cache();
    let (_keep, core, cache) = dir;
    let state = core.runtime_state();
    let source = state.directory.join("source.bin");
    std::fs::write(&source, b"payload").unwrap();

    let decodes = Arc::new(AtomicUsize::new(0));
    let counting = decodes.clone();
    let (generation, mut requests) = state.host.connect();
    let worker_source = source.clone();
    let port = core.port;
    let worker_state = state.clone();
    let worker = tokio::spawn(async move {
        while let Some(request) = requests.recv().await {
            let state = &worker_state;
            let params = &request["params"];
            match request["method"].as_str().unwrap() {
                "thumbnailAuthorize" => {
                    let _ = state.host.reply(
                        generation,
                        serde_json::json!({"id":request["id"],"result":cache.to_str().unwrap()}),
                    );
                }
                "thumbnailDecode" => {
                    counting.fetch_add(1, Ordering::SeqCst);
                    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
                    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
                    bytes.extend(std::fs::read(&worker_source).unwrap());
                    bytes.extend(params.to_string().as_bytes());
                    let posted = reqwest::Client::new()
                        .post(format!(
                            "http://127.0.0.1:{port}/files/thumbnail/output/{}",
                            params["outputToken"].as_str().unwrap()
                        ))
                        .bearer_auth(&plain_rs::utils::base64::base64_encode(&[4; 32]))
                        .body(bytes)
                        .send()
                        .await;
                    assert_eq!(
                        posted.map(|response| response.status().as_u16()).ok(),
                        Some(204),
                        "the decode output has to reach the waiting reader"
                    );
                    let _ = state.host.reply(
                        generation,
                        serde_json::json!({"id":request["id"],"result":true}),
                    );
                }
                other => panic!("unexpected host method {other}"),
            }
        }
    });

    let base = request_for(&source);
    let mut calls = Vec::new();
    for _ in 0..8 {
        let state = state.clone();
        let source = source.clone();
        calls.push(tokio::spawn(async move {
            render(&state, request_for(&source)).await
        }));
    }
    let first = calls.remove(0).await.unwrap();
    for call in calls {
        assert_eq!(first, call.await.unwrap(), "every caller sees one render");
    }
    assert_eq!(
        decodes.load(Ordering::SeqCst),
        1,
        "concurrent requests for one key coalesce into a single decode;          first status {} body {:?}",
        first.0,
        String::from_utf8_lossy(&first.2)
    );
    assert_eq!(first.0, 200);
    assert_eq!(header(&first.1, "content-type"), "image/png");
    assert_eq!(header(&first.1, "cache-control"), "private, max-age=86400");

    worker.abort();
    core.stop_core().await;
}

#[tokio::test]
async fn a_changed_source_and_a_different_shape_both_invalidate() {
    let (_dir, core, cache) = server_with_cache();
    let state = core.runtime_state();
    let source = state.directory.join("source.bin");
    std::fs::write(&source, b"first").unwrap();
    decode_worker(&state, core.port, cache.clone(), source.clone());

    let base = request_for(&source);
    let first = render(&state, variant(&base, None, None)).await;
    assert_eq!(first.0, 200);

    // Same bytes, same key: served from the cache without a decode.
    let again = render(&state, variant(&base, None, None)).await;
    assert_eq!(again, first);

    // A longer source has a different fingerprint, so the cached entry is
    // stale rather than silently reused.
    std::fs::write(&source, b"second-longer").unwrap();
    let changed = render(&state, variant(&base, None, None)).await;
    assert_ne!(
        changed.2, first.2,
        "a changed source must not serve the old bytes"
    );

    // The crop mode is part of the key, so it is a different entry.
    let mut cropped = variant(&base, None, None);
    cropped.center_crop = false;
    let uncropped = render(&state, cropped).await;
    assert_ne!(
        uncropped.2, changed.2,
        "center-crop is part of the cache key"
    );

    core.stop_core().await;
}

#[tokio::test]
async fn a_matching_conditional_request_answers_not_modified() {
    let (_dir, core, cache) = server_with_cache();
    let state = core.runtime_state();
    let source = state.directory.join("source.bin");
    std::fs::write(&source, b"payload").unwrap();
    decode_worker(&state, core.port, cache.clone(), source.clone());

    let base = request_for(&source);
    let first = render(&state, variant(&base, None, None)).await;
    assert_eq!(first.0, 200);
    let etag = header(&first.1, "etag");
    assert_eq!(etag, format!("\"{}\"", hex(&first.2)));

    let mut conditional = variant(&base, None, None);
    conditional.if_none_match = Some(format!("\"old\", {etag}"));
    let not_modified = render(&state, conditional).await;
    assert_eq!(not_modified.0, 304);
    assert!(not_modified.2.is_empty(), "304 carries no body");

    // A weak tag for the same bytes still matches.
    let mut weak = variant(&base, None, None);
    weak.if_none_match = Some(format!("W/{etag}"));
    assert_eq!(render(&state, weak).await.0, 304);

    // The etag of a different shape does not satisfy this request's.
    let mut other = variant(&base, None, None);
    other.center_crop = false;
    let other_etag = header(&render(&state, variant(&other, None, None)).await.1, "etag");
    let stale = variant(&other, None, Some(&etag));
    assert_eq!(render(&state, stale).await.0, 200);
    assert_ne!(other_etag, etag);
}

#[tokio::test]
async fn a_revoked_authorization_is_a_refusal_not_an_empty_image() {
    let (_dir, core, _cache) = server_with_cache();
    let state = core.runtime_state();
    let source = state.directory.join("source.bin");
    std::fs::write(&source, b"payload").unwrap();
    let (generation, mut requests) = state.host.connect();
    let owned = state.clone();
    let worker = tokio::spawn(async move {
        while let Some(request) = requests.recv().await {
            let error = if request["method"] == "thumbnailAuthorize" {
                "permission revoked"
            } else {
                "thumbnailDecode must not run without authorization"
            };
            let _ = owned.host.reply(
                generation,
                serde_json::json!({"id":request["id"],"error":error}),
            );
        }
    });

    let refused = render(&state, request_for(&source)).await;
    assert_eq!(
        refused.0, 400,
        "a refused authorization must not look like an empty thumbnail"
    );
    assert!(
        String::from_utf8_lossy(&refused.2).contains("permission revoked"),
        "the refusal has to say why, got {}",
        String::from_utf8_lossy(&refused.2)
    );
    worker.abort();
    core.stop_core().await;
}

#[tokio::test]
async fn a_source_that_vanishes_produces_no_content() {
    let (_dir, core, cache) = server_with_cache();
    let state = core.runtime_state();
    let source = state.directory.join("missing.bin");
    decode_worker(&state, core.port, cache.clone(), source.clone());
    let rendered = render(&state, request_for(&source)).await;
    assert_eq!(
        rendered.0, 204,
        "a source that is not there has no fingerprint and nothing to render"
    );
    core.stop_core().await;
}

fn hex(bytes: &[u8]) -> String {
    plain_rs::utils::hex::bytes_to_hex(&sha2::Sha256::digest(bytes))
}
