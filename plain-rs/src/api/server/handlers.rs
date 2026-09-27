//! Request handlers for the shared API routes (`/init`, `/graphql`,
//! `/peer_graphql`, `/nearby`, health) plus the desktop DLNA/WS/404
//! fallback dispatch — a direct port of the old `http_handler::handle`
//! route table onto axum extractors, merged with plain-nas's graphql
//! transport (session keys, dev bearer, replay wrapper).

use axum::body::Body;
use axum::body::Bytes;
use axum::extract::ConnectInfo;
use axum::extract::FromRequestParts;
use axum::extract::Request;
use axum::extract::State;
use axum::extract::ws::WebSocketUpgrade;
use axum::http::{Method, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::Value;
use serde_json::json;
use std::collections::HashMap;
use std::net::SocketAddr;

use super::request_key::{RequestKeyError, resolve_request_key};
use super::response::{APP_ID, respond};
use super::ws;
use crate::api::dlna;
use crate::api::executor::execute_graphql;
use crate::api::server::ServerState;
use crate::xchacha_encrypt;

pub async fn health() -> Response {
    respond(200, APP_ID.as_bytes().to_vec(), "text/plain")
}

/// `POST /init` — one handler, two host semantics:
///
/// * nas: plain-app alignment (`SystemRoutes.kt`) — never 401; a
///   missing `c-id` is 400; answers `{needsSetup, signaturePublicKey}`
///   from the fjall kv password/signature stores.
/// * desktop: no password management. If the body decrypts with the
///   URL token the client is authenticated → empty body (frontend:
///   token + empty body → auto-login); otherwise return
///   `{signaturePublicKey}` (the Ed25519 verifying key — last 32 bytes
///   of the 64-byte keypair) so the frontend can proceed with the
///   handshake.
pub async fn init(State(state): State<ServerState>, headers: axum::http::HeaderMap, body: Bytes) -> Response {
    #[cfg(feature = "nas")]
    if state.nas.is_some() {
        return super::auth::init_nas(&state, &headers).await;
    }
    let _ = headers;
    let ctx = &state.ctx;
    let authenticated =
        !body.is_empty() && !ctx.token.is_empty() && crate::xchacha_decrypt(&ctx.token, &body).is_some();

    if authenticated {
        // Frontend: `r.status === 200 && token && !bodyText` → finishLoginSuccess()
        respond(200, Vec::new(), "text/plain")
    } else {
        let kp_bytes = crate::base64_decode(&ctx.identity.ed25519_keypair);
        let signature_public_key = if kp_bytes.len() == 64 {
            crate::base64_encode(&kp_bytes[32..])
        } else {
            String::new()
        };
        let json = json!({ "signaturePublicKey": signature_public_key });
        respond(200, json.to_string().into_bytes(), "application/json")
    }
}

/// `POST /graphql` — one handler, two execution paths behind one
/// request-key resolution and one replay-wrapper strip:
///
/// * nas: session key (or config dev bearer) decrypts the body; the
///   strict `async_graphql::Request` parse feeds the type-erased schema
///   with the cid injected; the response re-encrypts (dev bearer →
///   plaintext JSON).
/// * desktop: URL token decrypts; the body parses as a JSON value
///   (malformed → `{}` fallback), runs through the local executor's
///   stub pre-filter, and the response re-encrypts with the token.
pub async fn graphql(
    State(state): State<ServerState>,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> Response {
    #[cfg_attr(not(feature = "nas"), allow(unused_variables))]
    let (key, cid) = match resolve_request_key(&state, &headers, true) {
        Ok(v) => v,
        Err(e) => return nas_key_error(&e),
    };

    #[cfg(feature = "nas")]
    if state.nas.is_some() {
        let decrypted: Bytes = match super::request_key::decrypt_body(&key, &body) {
            Some(b) => b.into(),
            // Nas contract: a body the session key cannot decrypt is a
            // 400 with the JSON error shape (Go parity).
            None => {
                return (
                    StatusCode::BAD_REQUEST,
                    axum::Json(json!({ "errors": [{ "message": "Decryption failed" }] })),
                )
                    .into_response()
            }
        };
        return run_graphql_nas(&state, key, decrypted, cid).await;
    }

    // Desktop contract: token-decrypt failure is a bare 401.
    let Some(plaintext) = super::request_key::decrypt_body(&key, &body) else {
        return respond(401, Vec::new(), "text/plain");
    };
    let json_bytes = strip_replay_wrapper(&plaintext).into_bytes();
    let request: Value = serde_json::from_slice(&json_bytes).unwrap_or_else(|_| json!({}));
    let Some(local_schema) = state.local_schema() else {
        return respond(500, b"no local schema".to_vec(), "text/plain");
    };
    let response_json = execute_graphql(&local_schema, request, state.ctx.clone()).await;
    let response_text = response_json.to_string();
    match xchacha_encrypt(&state.ctx.token, response_text.as_bytes()) {
        Some(encrypted) => respond(200, encrypted, "application/octet-stream"),
        None => respond(500, Vec::new(), "text/plain"),
    }
}

/// The nas execution path: strict Request parse → type-erased schema
/// with the cid as per-request data → encrypt (or plaintext JSON for
/// the dev bearer) with the request key.
#[cfg(feature = "nas")]
async fn run_graphql_nas(
    state: &ServerState,
    key: super::request_key::RequestKey,
    body: Bytes,
    cid: String,
) -> Response {
    // Session-mode clients wrap the GraphQL JSON with replay protection:
    // "TIMESTAMP|NONCE|JSON" (plain-desktop `wrapWithReplayProtection`).
    // Dev-mode Bearer requests carry the bare JSON — accept both.
    let payload = strip_replay_wrapper(&body);
    let request: async_graphql::Request = match serde_json::from_slice(payload.as_bytes()) {
        Ok(r) => r,
        Err(_) => {
            let resp = async_graphql::Response::from_errors(vec![async_graphql::ServerError::new(
                "Bad request",
                None,
            )]);
            return wrap_graphql_response(&resp, &key);
        }
    };
    let resp = state.schema.execute(request, &cid).await;
    wrap_graphql_response(&resp, &key)
}

#[cfg(feature = "nas")]
fn wrap_graphql_response(
    resp: &async_graphql::Response,
    key: &super::request_key::RequestKey,
) -> Response {
    let bytes = match serde_json::to_vec(&resp) {
        Ok(b) => b,
        Err(_) => return nas_server_error("encode failed"),
    };
    if let Some(enc) = super::request_key::encrypt_body(key, &bytes) {
        let content_type = match key {
            super::request_key::RequestKey::DevBearer => "application/json",
            _ => "application/octet-stream",
        };
        respond(200, enc, content_type)
    } else {
        nas_server_error("encrypt failed")
    }
}

/// Render a key-resolution failure with the plain-nas JSON error shape
/// (mirrors Go `requireAuth`'s branches). Only reachable on the nas
/// host — desktop resolution never fails.
fn nas_key_error(e: &RequestKeyError) -> Response {
    let msg = match e {
        RequestKeyError::MissingCid => "Unauthorized",
        RequestKeyError::SessionNotFound | RequestKeyError::BadSessionToken => "Unauthorized",
        RequestKeyError::DevHeaderMissing => {
            "Unauthorized: make sure add http headers `{\"authorization\": \"Bearer <dev_token>\"}`"
        }
        RequestKeyError::DevTokenInvalid => "Unauthorized: token is invalid",
    };
    (
        StatusCode::UNAUTHORIZED,
        axum::Json(json!({ "errors": [{ "message": msg }] })),
    )
        .into_response()
}

#[cfg(feature = "nas")]
fn nas_server_error(msg: &str) -> Response {
    respond(
        500,
        json!({ "errors": [{ "message": msg }] }).to_string().into_bytes(),
        "application/json",
    )
}

pub async fn peer_graphql_handler(
    State(state): State<ServerState>,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> Response {
    let header_client_id = header_string(&headers, "c-id");
    let header_channel_id = header_string(&headers, "c-cid");

    // Nas branch: this phase the nas crate still owns its peer schema
    // (its resolvers take the nas `PeerCtx`); run the old
    // nas chat_peer flow behind the type-erased executor. Phase 3
    // collapses this into the shared `peer_graphql::handle`.
    #[cfg(feature = "nas")]
    if let Some(nas) = state.nas.as_ref() {
        return peer_graphql_nas(&state, nas, &header_client_id, &header_channel_id, &body).await;
    }

    super::super::peer_graphql::handle(
        &body,
        &header_client_id,
        &header_channel_id,
        &state.ctx,
        &state.peer_schema,
    )
    .await
}

/// Peer GraphQL ingestion, nas flavor: auth chain → nas peer schema →
/// encrypted response, mirroring plain-app `PeerGraphQLService.handle`.
#[cfg(feature = "nas")]
async fn peer_graphql_nas(
    state: &ServerState,
    nas: &std::sync::Arc<super::NasServerState>,
    header_client_id: &str,
    header_channel_id: &str,
    body: &[u8],
) -> Response {
    log::info!("[/peer_graphql] request from c-id={header_client_id}");

    // ── 1. Authenticate (key selection, decrypt, timestamp, signature) ──
    let authed = match crate::chat::peer_auth::authenticate(
        &state.ctx.chat.service.db,
        header_client_id,
        header_channel_id,
        body,
        &state.ctx.chat.service.channel_key_cache,
    ) {
        Ok(a) => a,
        Err(e) => {
            log::warn!("[/peer_graphql] auth failed: {}", e.reason());
            return respond(401, e.reason().as_bytes().to_vec(), "text/plain");
        }
    };

    // ── 2. Execute through the typed peer schema ──────────────────────
    // The plaintext payload is a GraphQL-over-HTTP JSON envelope
    // `{"query":"...","variables":{...}}`.
    let request_value: Value = serde_json::from_str(&authed.graphql_json)
        .unwrap_or_else(|_| json!({ "data": null }));
    let query_str = request_value
        .get("query")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let vars: async_graphql::Variables = request_value
        .get("variables")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();

    let response = nas
        .peer_schema
        .execute(
            async_graphql::Request::new(query_str).variables(vars),
            authed.peer,
            header_channel_id,
            state.ctx.chat.clone(),
        )
        .await;
    let response_json =
        serde_json::to_value(&response).unwrap_or_else(|_| json!({ "data": null }));

    // ── 3. Encrypt and respond ────────────────────────────────────────
    let response_text = response_json.to_string();
    match crate::xchacha_encrypt_raw(&authed.key, response_text.as_bytes()) {
        Some(encrypted) => respond(200, encrypted, "application/octet-stream"),
        None => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

/// `POST /nearby` — LAN transport for pairing messages. The request body
/// is the prefix-prefixed wire format the BLE nearby service uses
/// ("PAIR_REQUEST:{…}"). Mirrors plain-app `NearbyRoutes`.
pub async fn nearby(
    State(state): State<ServerState>,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
    body: Bytes,
) -> Response {
    let text = String::from_utf8_lossy(&body).to_string();
    let remote_ip = remote.ip().to_string();
    let known = state.ctx.chat.pairing.handle_nearby_post(&text, &remote_ip);
    if known {
        respond(200, b"1".to_vec(), "text/plain")
    } else {
        log::error!(
            "NearbyRoutes: unknown message type, body={}",
            &text.chars().take(50).collect::<String>()
        );
        respond(400, b"unknown message type".to_vec(), "text/plain")
    }
}

/// Desktop unmatched paths: DLNA receiver routes first, then WebSocket
/// upgrades (any path; `/status…` selects the peer-status socket), then
/// 404. Same order the hand-rolled dispatch used.
pub async fn fallback(State(state): State<ServerState>, req: Request) -> Response {
    let method = req.method().as_str().to_owned();
    let path = req.uri().path().to_owned();
    if dlna::is_receiver_path(&method, &path) {
        return dlna_route(&state, req, &method, &path).await;
    }
    if req.method() == Method::GET {
        let (mut parts, _) = req.into_parts();
        if let Ok(upgrade) = WebSocketUpgrade::from_request_parts(&mut parts, &()).await {
            let raw_path = parts
                .uri
                .path_and_query()
                .map(|pq| pq.as_str().to_owned())
                .unwrap_or_else(|| "/".to_owned());
            log::debug!("local_server: new WS connection path={raw_path}");
            if raw_path.starts_with("/status") {
                return upgrade
                    .on_upgrade(move |socket| ws::status_socket(socket, raw_path, state.ctx.clone()))
                    .into_response();
            }
            return upgrade
                .on_upgrade(move |socket| ws::chat_socket(socket, raw_path, state))
                .into_response();
        }
    }
    respond(404, Vec::new(), "text/plain")
}

/// DLNA MediaRenderer receiver routes — served plain (no token) so remote
/// control points can reach them. Gated by the DLNA toggle + running
/// engine, mirroring plain-app's `handleDlnaReceiver` (404 when disabled).
async fn dlna_route(state: &ServerState, req: Request, method: &str, path: &str) -> Response {
    let ctx = &state.ctx;
    if !crate::prefs::dlna::enabled(&ctx.prefs) || !ctx.dlna_engine.is_running() {
        return respond(404, Vec::new(), "text/plain");
    }
    let headers = req.headers().clone();
    let body = match axum::body::to_bytes(req.into_body(), 1024 * 1024).await {
        Ok(b) => String::from_utf8_lossy(&b).to_string(),
        Err(_) => return respond(400, b"bad dlna body".to_vec(), "text/plain"),
    };
    let mut dlna_headers = HashMap::new();
    if let Some(v) = headers.get("soapaction").and_then(|v| v.to_str().ok()) {
        dlna_headers.insert("soapaction".to_string(), v.to_string());
    }
    if let Some(v) = headers.get("c-name").and_then(|v| v.to_str().ok()) {
        dlna_headers.insert("c-name".to_string(), v.to_string());
    }
    let local_ip = crate::mdns::host_responder::local_ipv4_strs()
        .into_iter()
        .next()
        .unwrap_or_else(|| "127.0.0.1".to_string());
    let device_name = ctx.device_name.read().unwrap().clone();
    let allowed = crate::prefs::dlna::senders(&ctx.prefs, "dlna_allowed_senders");
    let denied = crate::prefs::dlna::senders(&ctx.prefs, "dlna_denied_senders");
    let Some(command_tx) = ctx.dlna_engine.command_sender() else {
        return respond(404, Vec::new(), "text/plain");
    };
    let resp = dlna::http_router::route(
        &ctx.dlna_engine.state,
        method,
        path,
        &dlna_headers,
        &body,
        ctx.dlna_engine.device_uuid(),
        &device_name,
        &local_ip,
        &command_tx,
        &allowed,
        &denied,
    )
    .await;

    let status = StatusCode::from_u16(resp.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let mut builder = Response::builder().status(status);
    if let Some(ct) = &resp.content_type {
        builder = builder.header("content-type", ct.as_str());
    }
    for (k, v) in &resp.headers {
        builder = builder.header(k.as_str(), v.as_str());
    }
    builder
        .body(Body::from(resp.body))
        .unwrap_or_else(|_| respond(500, Vec::new(), "text/plain"))
}

fn header_string(headers: &axum::http::HeaderMap, name: &str) -> String {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string()
}

/// Strip the `"TIMESTAMP|NONCE|"` replay-protection prefix from the
/// decrypted payload (plain-desktop `wrapWithReplayProtection`). The
/// stricter plain-nas variant: only a numeric `TS|NONCE|` prefix is
/// stripped, timestamps >10 min off warn, anything else passes through
/// unchanged.
fn strip_replay_wrapper(body: &[u8]) -> String {
    let Ok(text) = std::str::from_utf8(body) else {
        return String::from_utf8_lossy(body).into_owned();
    };
    let Some((ts_str, rest)) = text.split_once('|') else {
        return text.to_string();
    };
    let Ok(ts) = ts_str.parse::<u64>() else {
        return text.to_string();
    };
    let Some((_, json)) = rest.split_once('|') else {
        return text.to_string();
    };
    let now: u64 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    if ts.abs_diff(now) > 600_000 {
        log::warn!("[graphql] stale request timestamp ({ts} vs {now})");
    }
    json.to_string()
}

#[cfg(test)]
#[path = "../../../tests/unit/api/server/handlers.rs"]
mod tests;

#[cfg(all(test, feature = "nas"))]
#[path = "../../../tests/unit/api/server/chat_peer.rs"]
mod chat_peer_nas_tests;
