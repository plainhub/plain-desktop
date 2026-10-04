//! Peer-to-peer GraphQL endpoint (`POST /peer_graphql`).
//!
//! Mirrors the structure of `crate::http_server::main_schemas` so the local and
//! peer-facing surfaces stay symmetrical, while keeping the two concerns
//! physically separated to avoid over-coupling the HTTP layer.
//!
//! Sub-modules:
//!   auth     — peer trust chain (decrypt, signature, timestamp)
//!   context  — per-request `PeerCtx` injected into the schema
//!   schema   — async-graphql `PeerSchema` with the two peer mutations
//!
//! The mutation bodies live on the shared chat service (the same
//! chat service layer the local GraphQL mutations use); the `schema`
//! resolvers are thin and only forward the authenticated arguments.

mod context;
mod schema;

use std::sync::Arc;

use axum::response::Response;

use crate::api::context::AppCtx;
use crate::chat::peer_auth;
use crate::http_server::routes::response::respond;
use crate::xchacha_encrypt_raw;

pub use context::PeerCtx;
pub use schema::{PeerSchema, build_schema};

/// Handle a `POST /peer_graphql` request.
///
/// Takes the encrypted body and the `c-id` / `c-cid` headers, runs the
/// auth chain, executes the GraphQL payload through the typed peer
/// schema, and returns the encrypted response.
pub async fn handle(
    body: &[u8],
    header_client_id: &str,
    header_channel_id: &str,
    ctx: &Arc<AppCtx>,
    peer_schema: &Arc<PeerSchema>,
) -> Response {
    log::info!("[/peer_graphql] request from c-id={header_client_id}");

    // ── 1. Authenticate ──────────────────────────────────────────────────
    let authed = match peer_auth::authenticate(&ctx.db, header_client_id, header_channel_id, body) {
        Ok(a) => a,
        Err(e) => {
            log::warn!("[/peer_graphql] auth failed: {}", e.reason());
            let msg = e.reason();
            return respond(e.http_status(), msg.as_bytes().to_vec(), "text/plain");
        }
    };

    // ── 2. Execute through the typed schema ──────────────────────────────
    // The plaintext payload is a GraphQL-over-HTTP JSON envelope
    // `{"query":"...","variables":{...}}` (same shape as the local
    // executor in `graphql/executor.rs`). Extract `query` + `variables`
    // before handing the query string to `Request::new`.
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

    let peer_ctx = PeerCtx {
        signature: authed.signature_b64,
        timestamp: authed.timestamp,
        peer: authed.peer,
        channel_id: header_channel_id.to_string(),
        app: ctx.clone(),
    };
    let response = peer_schema
        .execute(
            async_graphql::Request::new(query_str)
                .variables(vars)
                .data(peer_ctx),
        )
        .await;
    let response_json =
        serde_json::to_value(&response).unwrap_or_else(|_| serde_json::json!({ "data": null }));

    // ── 3. Encrypt and respond ───────────────────────────────────────────
    let response_text = response_json.to_string();
    match xchacha_encrypt_raw(&authed.key, response_text.as_bytes()) {
        Some(encrypted) => respond(200, encrypted, "application/octet-stream"),
        None => respond(500, Vec::new(), "text/plain"),
    }
}
