//! Peer-to-peer GraphQL endpoint (`POST /peer_graphql`).
//!
//! Mirrors the structure of `crate::local::graphql` so the local and
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
use tokio::io::AsyncWrite;

use crate::local::graphql::context::AppCtx;
use crate::local::server::response::respond;
use plain_rs::chat::peer_auth;
use plain_rs::xchacha_encrypt_raw;

pub use context::PeerCtx;
pub use schema::{PeerSchema, build_schema};

/// Handle a fully-parsed `POST /peer_graphql` request.
///
/// The caller is responsible for HTTP framing (parsing the request line,
/// headers, and body). This function takes the encrypted body and the
/// `c-id` / `c-cid` headers, runs the auth chain, executes the GraphQL
/// payload through the typed peer schema, and writes the encrypted
/// response back to `wr`.
pub async fn handle<W>(
    wr: &mut W,
    body: &[u8],
    header_client_id: &str,
    header_channel_id: &str,
    ctx: &Arc<AppCtx>,
    peer_schema: &Arc<PeerSchema>,
) where
    W: AsyncWrite + Unpin,
{
    log::info!("[/peer_graphql] request from c-id={header_client_id}");

    // ── 1. Authenticate ──────────────────────────────────────────────────
    let authed = match peer_auth::authenticate(
        &ctx.db,
        header_client_id,
        header_channel_id,
        body,
        &ctx.chat.service.channel_key_cache,
    ) {
        Ok(a) => a,
        Err(e) => {
            log::warn!("[/peer_graphql] auth failed: {}", e.reason());
            let msg = e.reason();
            respond(wr, 401, "Unauthorized", msg.as_bytes(), "text/plain").await;
            return;
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
        Some(encrypted) => {
            respond(wr, 200, "OK", &encrypted, "application/octet-stream").await;
        }
        None => {
            respond(wr, 500, "Internal Server Error", b"", "text/plain").await;
        }
    }
}
