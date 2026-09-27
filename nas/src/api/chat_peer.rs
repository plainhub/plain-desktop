//! Chat peer endpoints: `POST /nearby` (pairing transport) and
//! `POST /peer_graphql` (encrypted peer GraphQL ingestion).
//!
//! Neither route uses the session auth — the pairing protocol carries
//! its own ECDH + Ed25519 handshake and the peer GraphQL chain is
//! `plain_rs::chat::peer_auth::authenticate`.

use super::auth::AppState;
use axum::body::Bytes;
use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use plain_rs::xchacha_encrypt_raw;
use std::net::SocketAddr;

/// LAN pairing transport — mirrors plain-app `NearbyRoutes`. The request
/// body is the prefix-prefixed wire format (`PAIR_REQUEST:{…}`, …);
/// returns 200 for a known message type, 400 otherwise.
pub async fn nearby_handler(
    State(state): State<AppState>,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
    body: Bytes,
) -> Response {
    let text = String::from_utf8_lossy(&body).to_string();
    let remote_ip = remote.ip().to_string();
    let known = state.chat.pairing.handle_nearby_post(&text, &remote_ip);
    if known {
        (StatusCode::OK, "1").into_response()
    } else {
        log::error!(
            "NearbyRoutes: unknown message type, body={}",
            &text.chars().take(50).collect::<String>()
        );
        (
            StatusCode::BAD_REQUEST,
            [(axum::http::header::CONTENT_TYPE, "text/plain")],
            "unknown message type",
        )
            .into_response()
    }
}

/// Peer GraphQL ingestion. Auth chain → typed peer schema → encrypted
/// response, mirroring plain-app `PeerGraphQLService.handle`.
pub async fn peer_graphql_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let header_client_id = headers
        .get("c-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    let header_channel_id = headers
        .get("c-cid")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    log::info!("[/peer_graphql] request from c-id={header_client_id}");

    // ── 1. Authenticate (key selection, decrypt, timestamp, signature) ──
    let authed = match plain_rs::chat::peer_auth::authenticate(
        &state.chat.service.db,
        header_client_id,
        header_channel_id,
        &body,
        &state.chat.service.channel_key_cache,
    ) {
        Ok(a) => a,
        Err(e) => {
            log::warn!("[/peer_graphql] auth failed: {}", e.reason());
            return (
                StatusCode::UNAUTHORIZED,
                [(axum::http::header::CONTENT_TYPE, "text/plain")],
                e.reason().to_string(),
            )
                .into_response();
        }
    };

    // ── 2. Execute through the typed peer schema ──────────────────────
    // The plaintext payload is a GraphQL-over-HTTP JSON envelope
    // `{"query":"...","variables":{...}}`.
    let request_value: serde_json::Value = serde_json::from_str(&authed.graphql_json)
        .unwrap_or_else(|_| serde_json::json!({ "data": null }));
    let query_str = request_value
        .get("query")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    let vars: async_graphql::Variables = request_value
        .get("variables")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();

    let peer_ctx = crate::gql::peer_schema::PeerCtx {
        state: state.chat.clone(),
        peer: authed.peer,
        channel_id: header_channel_id.to_string(),
    };
    let response = state
        .chat
        .peer_schema
        .execute(
            async_graphql::Request::new(query_str)
                .variables(vars)
                .data(peer_ctx),
        )
        .await;
    let response_json =
        serde_json::to_value(&response).unwrap_or_else(|_| serde_json::json!({ "data": null }));

    // ── 3. Encrypt and respond ────────────────────────────────────────
    let response_text = response_json.to_string();
    match xchacha_encrypt_raw(&authed.key, response_text.as_bytes()) {
        Some(encrypted) => (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "application/octet-stream")],
            encrypted,
        )
            .into_response(),
        None => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/api/chat_peer.rs"]
mod tests;
