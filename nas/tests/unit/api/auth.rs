//! Unit tests for `src/api/auth.rs` — moved out-of-line; compiled
//! as the `tests` child module via `#[cfg(test)] #[path]` there.
use super::{AppState, init_handler};
use crate::config::Config;
use crate::db::{Db, PasswordStore};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use std::sync::Arc;

fn test_state() -> AppState {
    let dir = tempfile::tempdir().expect("temp dir");
    let db = Arc::new(Db::open(dir.path()).expect("temp db opens"));
    let data_dir = dir.path().to_path_buf();
    let prefs = Arc::new(crate::prefs::Prefs::load(&data_dir.join("prefs.json")).unwrap());
    std::mem::forget(dir); // the db handle must outlive the test
    let config = Arc::new(Config::parse("[server]\nhttp_port = 8080\n"));
    let chat = crate::chat::test_state(&data_dir);
    let schema = crate::gql::build_schema(
        db.clone(),
        prefs.clone(),
        config.clone(),
        data_dir,
        chat.clone(),
    );
    AppState {
        chat,
        config,
        db,
        prefs,
        ws_hub: Arc::new(crate::ws_hub::WsHub::new()),
        cors: crate::api::cors::CorsPolicy::from_config(&Config::default()),
        schema,
    }
}

fn cid_headers(cid: &str) -> HeaderMap {
    let mut h = HeaderMap::new();
    h.insert("c-id", cid.parse().unwrap());
    h
}

async fn init_json(state: &AppState, headers: HeaderMap) -> (StatusCode, serde_json::Value) {
    let resp = init_handler(State(state.clone()), headers).await;
    let status = resp.status();
    let body = axum::body::to_bytes(resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    (status, serde_json::from_slice(&body).unwrap())
}

#[tokio::test]
async fn init_never_401s_and_triages_needs_setup() {
    let state = test_state();

    // Fresh server: 200 + needsSetup, regardless of any probe.
    let (status, v) = init_json(&state, cid_headers("cid-1")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["needsSetup"], true);
    assert!(!v["signaturePublicKey"].as_str().unwrap().is_empty());

    // Initialized server: plain-app alignment — 200 + needsSetup:false even
    // with no login probe presented.
    PasswordStore::new(&state.prefs)
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
    let state = test_state();
    let resp = init_handler(State(state), HeaderMap::new()).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}
