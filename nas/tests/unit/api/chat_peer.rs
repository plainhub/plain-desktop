//! Unit tests for `src/api/chat_peer.rs` — moved out-of-line; compiled
//! as the `tests` child module via `#[cfg(test)] #[path]` there.

use super::*;
use axum::body::Bytes;
use axum::http::{HeaderMap, StatusCode};
use std::net::SocketAddr;
use std::sync::Arc;

use plain_rs::chat::db::DPeer;
use plain_rs::chat::enums::{DeviceType, PeerStatus};
use plain_rs::{
    base64_encode, ed25519_generate, ed25519_sign, xchacha_decrypt_raw, xchacha_encrypt_raw,
};

fn app_state() -> AppState {
    let dir = tempfile::tempdir().expect("temp dir");
    let db = Arc::new(crate::db::Db::open(dir.path()).expect("temp db opens"));
    let data_dir = dir.path().to_path_buf();
    let prefs = Arc::new(crate::prefs::Prefs::load(&data_dir.join("prefs.json")).unwrap());
    std::mem::forget(dir); // handles must outlive the test
    let config = Arc::new(crate::config::Config::parse("[server]\nhttp_port = 8080\n"));
    let chat = crate::test_support::chat_state(&data_dir);
    let schema = crate::gql::build_schema(
        db.clone(),
        prefs.clone(),
        config.clone(),
        data_dir,
        chat.clone(),
    );
    AppState {
        config,
        db,
        prefs,
        ws_hub: Arc::new(crate::ws_hub::WsHub::new()),
        cors: crate::api::cors::CorsPolicy::from_config(&crate::config::Config::default()),
        schema,
        chat,
        peer_schema: crate::gql::peer_schema::build_schema(),
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[tokio::test]
async fn nearby_dispatches_known_prefixes() {
    let state = app_state();
    let addr: SocketAddr = "203.0.113.9:50000".parse().unwrap();

    let resp = nearby_handler(
        axum::extract::State(state.clone()),
        axum::extract::ConnectInfo(addr),
        Bytes::from_static(b"DISCOVER:"),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = nearby_handler(
        axum::extract::State(state),
        axum::extract::ConnectInfo(addr),
        Bytes::from_static(b"WHAT:"),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

/// End-to-end ingestion: a paired peer posts an encrypted + signed
/// `createChatItem`; the handler authenticates, executes the peer schema,
/// and answers with an encrypted response. The message must land in the
/// DB with the peer as sender.
#[tokio::test]
async fn peer_graphql_create_chat_item_roundtrip() {
    let state = app_state();

    // Seed a paired peer with a known shared key + Ed25519 identity.
    let (kp, vk) = ed25519_generate();
    let key = [31u8; 32];
    state.chat.service.db.upsert_peer(&DPeer {
        id: "peer-a".into(),
        name: "Phone".into(),
        ip: "203.0.113.9".into(),
        key: base64_encode(&key),
        public_key: base64_encode(&vk),
        status: PeerStatus::Paired,
        port: 2443,
        device_type: DeviceType::Phone,
        token: String::new(),
        created_at: "2026-01-01T00:00:00Z".into(),
        updated_at: "2026-01-01T00:00:00Z".into(),
    });
    plain_rs::chat::events::refresh_peer_key_cache(
        &state.chat.service.db,
        &state.chat.service.peer_key_cache,
    );

    // Build the wire body: encrypt(signature|timestamp|graphql_json).
    let graphql_json = serde_json::json!({
        "query": "mutation CreateChatItem($content: String!) { createChatItem(content: $content) { id fromId toId } }",
        "variables": { "content": "{\"type\":\"TEXT\",\"value\":{\"text\":\"hi nas\"}}" }
    })
    .to_string();
    let ts = now_ms();
    let sig = ed25519_sign(&kp, format!("{ts}{graphql_json}").as_bytes());
    let payload = format!("{sig}|{ts}|{graphql_json}");
    let body = xchacha_encrypt_raw(&key, payload.as_bytes()).expect("encrypt body");

    let mut headers = HeaderMap::new();
    headers.insert("c-id", "peer-a".parse().unwrap());

    let resp = peer_graphql_handler(
        axum::extract::State(state.clone()),
        headers,
        Bytes::from(body),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);

    // Decrypt the response with the same shared key; the peer schema must
    // have created the item.
    let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let plain = xchacha_decrypt_raw(&key, &bytes).expect("decrypt response");
    let v: serde_json::Value = serde_json::from_slice(&plain).unwrap();
    assert!(v.get("errors").is_none(), "{v}");
    let item = &v["data"]["createChatItem"];
    let id = item["id"].as_str().unwrap().to_string();
    assert_eq!(item["fromId"], "peer-a");
    assert_eq!(item["toId"], "me");

    // The message landed in the DB with the peer as sender.
    let row = state
        .chat
        .service
        .db
        .get_chat_by_id(&id)
        .expect("row persisted");
    assert_eq!(row.from_id, "peer-a");
    assert_eq!(row.to_id, "me");
}

/// Unknown peer → 401 with the auth reason.
#[tokio::test]
async fn peer_graphql_rejects_unknown_peer() {
    let state = app_state();
    let mut headers = HeaderMap::new();
    headers.insert("c-id", "ghost".parse().unwrap());
    let resp = peer_graphql_handler(
        axum::extract::State(state),
        headers,
        Bytes::from_static(b"whatever"),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}
