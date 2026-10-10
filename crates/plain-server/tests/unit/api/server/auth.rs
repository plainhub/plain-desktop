//! Unit tests for the nas auth handlers (moved from plain-nas).
use super::init_session;
use crate::http_server::test_support::nas_state;
use crate::media::kv::PasswordStore;
use axum::http::{HeaderMap, StatusCode};

fn cid_headers(cid: &str) -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert("c-id", cid.parse().unwrap());
    h
}

async fn init_json(
    state: &crate::http_server::ServerState,
    headers: HeaderMap,
) -> (StatusCode, serde_json::Value) {
    let resp = init_session(state, &headers).await;
    let status = resp.status();
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    (status, serde_json::from_slice(&body).unwrap())
}

#[tokio::test]
async fn init_never_401s_and_triages_needs_setup() {
    let state = nas_state();

    // Fresh server: 200 + needsSetup, regardless of any probe.
    let (status, v) = init_json(&state, cid_headers("cid-1")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["needsSetup"], true);
    assert!(!v["signaturePublicKey"].as_str().unwrap().is_empty());

    // Initialized server: plain-app alignment — 200 + needsSetup:false even
    // with no login probe presented.
    PasswordStore::new(&state.ctx.prefs)
        .set(&"ab".repeat(64))
        .unwrap();
    let (status, v) = init_json(&state, cid_headers("cid-1")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["needsSetup"], false);

    // The signature public key is stable across calls and clients.
    let (_, v2) = init_json(&state, cid_headers("cid-2")).await;
    assert_eq!(v["signaturePublicKey"], v2["signaturePublicKey"]);
}

#[tokio::test]
async fn init_rejects_missing_client_id() {
    let state = nas_state();
    let resp = init_session(&state, &HeaderMap::new()).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}
