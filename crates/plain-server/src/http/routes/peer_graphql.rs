use std::sync::Arc;

use axum::response::Response;

use crate::assembly::context::AppCtx;
use crate::http::routes::local::response::respond;
use plain_rs::crypto::xchacha_encrypt_raw;
use plain_rs::transport::auth as peer_auth;

use crate::graphql::resolvers::peers::context::PeerCtx;
use crate::graphql::schemas::peer::PeerSchema;

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
