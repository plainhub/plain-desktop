//! End-to-end tests for the nas host driving the shared plain-rs router
//! handlers with the REAL nas peer schema (the plain-rs side tests use a
//! stub): authenticated peer GraphQL ingestion through
//! `NasPeerSchemaExec`.
use std::sync::Arc;

use crate::api::context::{AppCtx, LogShell};
use crate::api::server::ServerState;
use crate::api::server::{NasServerState, handlers};
use crate::{
    base64_encode, ed25519_generate, ed25519_sign, xchacha_decrypt_raw, xchacha_encrypt_raw,
};
use axum::body::Bytes;
use axum::http::{HeaderMap, StatusCode};

use crate::chat::db::DPeer;
use crate::chat::enums::{DeviceType, PeerStatus};

fn nas_router_state() -> ServerState {
    let dir = tempfile::tempdir().expect("temp dir");
    let data_dir = dir.path().to_path_buf();
    let prefs = Arc::new(crate::prefs::Prefs::load(&data_dir.join("prefs.json")).unwrap());
    let chat = Arc::new(crate::api::chat::ChatState::nas_init(&data_dir, &prefs).unwrap());
    let config = Arc::new(crate::media::config::Config::parse(
        "[server]\nhttp_port = 8080\n",
    ));
    let (event_tx, _) = tokio::sync::broadcast::channel(64);
    let ctx = AppCtx::assemble(
        data_dir.clone(),
        data_dir.join("cache"),
        data_dir.join("logs"),
        data_dir.join("library.db"),
        prefs.clone(),
        chat.clone(),
        event_tx,
        Arc::new(LogShell {
            version: String::new(),
        }),
        8080,
        8443,
    )
    .unwrap();
    std::mem::forget(dir);
    let db = ctx.media.db.clone();
    ServerState {
        schema: Arc::new(crate::api::schema::nas::build_nas_schema(
            db,
            prefs,
            config.clone(),
            data_dir,
            chat,
            ctx.library.clone(),
        )),
        peer_schema: Arc::new(crate::api::peer_graphql::build_schema()),
        ctx,
        nas: Some(Arc::new(NasServerState {
            config,
            cors: crate::api::server::cors::CorsPolicy::default(),
            peer_schema: Arc::new(crate::api::schema::nas::peer_schema::build_schema()),
        })),
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// A paired peer posts an encrypted + signed `createChatItem`; the nas
/// branch authenticates, executes the NAS peer schema via
/// `NasPeerSchemaExec`, and answers with an encrypted response. The
/// message must land in the DB with the peer as sender.
#[tokio::test]
async fn peer_graphql_create_chat_item_roundtrip_nas_schema() {
    let state = nas_router_state();

    let (kp, vk) = ed25519_generate();
    let key = [31u8; 32];
    state.ctx.chat.service.db.upsert_peer(&DPeer {
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
    crate::chat::events::refresh_peer_key_cache(
        &state.ctx.chat.service.db,
        &state.ctx.chat.service.peer_key_cache,
    );

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

    let resp = handlers::peer_graphql_handler(
        axum::extract::State(state.clone()),
        headers,
        Bytes::from(body),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);

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

    let row = state
        .ctx
        .chat
        .service
        .db
        .get_chat_by_id(&id)
        .expect("row persisted");
    assert_eq!(row.from_id, "peer-a");
    assert_eq!(row.to_id, "me");
}
