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

use super::request_key::{RequestKey, RequestKeyError, resolve_request_key};
use super::response::{APP_ID, respond};
use crate::http_server::websocket as ws;
use crate::api::executor::execute_graphql;
use crate::dlna_receiver;
use crate::http_server::{AuthPolicy, ServerState};

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
pub async fn init(
    State(state): State<ServerState>,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> Response {
    if matches!(state.settings.auth, AuthPolicy::Session { .. }) {
        return super::auth::init_session(&state, &headers).await;
    }
    let _ = headers;
    let ctx = &state.ctx;
    let authenticated = !body.is_empty()
        && !ctx.token.is_empty()
        && crate::xchacha_decrypt(&ctx.token, &body).is_some();

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

pub async fn graphql(
    State(state): State<ServerState>,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> Response {
    let (key, cid) = match resolve_request_key(&state, &headers, true) {
        Ok(v) => v,
        Err(e) => return request_key_error(&e),
    };
    let Some(plaintext) = super::request_key::decrypt_body(&key, &body) else {
        return respond(401, Vec::new(), "text/plain");
    };
    let json_bytes = strip_replay_wrapper(&plaintext).into_bytes();
    let request: Value = serde_json::from_slice(&json_bytes).unwrap_or_else(|_| json!({}));
    let response_json =
        execute_graphql(state.schema.as_ref(), request, state.ctx.clone(), cid).await;
    let response_text = response_json.to_string();
    let content_type = if matches!(key, RequestKey::DevBearer) {
        "application/json"
    } else {
        "application/octet-stream"
    };
    match super::request_key::encrypt_body(&key, response_text.as_bytes()) {
        Some(encrypted) => respond(200, encrypted, content_type),
        None => respond(500, Vec::new(), "text/plain"),
    }
}

fn request_key_error(e: &RequestKeyError) -> Response {
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

pub async fn peer_graphql_handler(
    State(state): State<ServerState>,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> Response {
    let header_client_id = header_string(&headers, "c-id");
    let header_channel_id = header_string(&headers, "c-cid");

    crate::http_server::peer_schemas::handle(
        &body,
        &header_client_id,
        &header_channel_id,
        &state.ctx,
        &state.peer_schema,
    )
    .await
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
pub async fn fallback(
    State(state): State<ServerState>,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
    req: Request,
) -> Response {
    let method = req.method().as_str().to_owned();
    let path = req.uri().path().to_owned();
    if dlna_receiver::is_receiver_path(&method, &path) {
        return dlna_route(&state, req, &method, &path, remote.ip().to_string()).await;
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
                    .on_upgrade(move |socket| {
                        ws::status_socket(socket, raw_path, state.ctx.clone())
                    })
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
async fn dlna_route(
    state: &ServerState,
    req: Request,
    method: &str,
    path: &str,
    sender_ip: String,
) -> Response {
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
    let resp = dlna_receiver::http_router::route(
        &ctx.dlna_engine.state,
        method,
        path,
        &dlna_headers,
        &body,
        ctx.dlna_engine.device_uuid(),
        &device_name,
        &local_ip,
        &sender_ip,
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

#[cfg(all(test, feature = "system"))]
#[path = "../../../tests/unit/api/server/chat_peer.rs"]
mod chat_peer_nas_tests;
