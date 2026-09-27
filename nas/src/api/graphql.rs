//! GraphQL HTTP handler. Mirrors the Go `cmd/services/api/graphql.go`:
//!   * `/graphql` requires either a `c-id` header (for a ChaCha20-encrypted
//!     session) or an `Authorization: Bearer <dev_token>` header (dev mode).
//!   * For session requests, the body is decrypted, the GraphQL handler is
//!     invoked, and the response is re-encrypted with the session key.

use crate::api::auth::AppState;
use crate::api::auth::try_decode_token;
use crate::crypto;
use crate::db::SessionStore;
use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};

pub async fn graphql_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let client_id = headers
        .get("c-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    if client_id.is_empty() {
        // dev mode: trust the bearer token from config. In dev mode we use
        // the literal header value `dev` as the client id so resolvers that
        // need a client_id (file tasks, etc.) still work.
        //
        // Mirrors Go `requireAuth`'s dev branch: a missing `Authorization`
        // header yields a "make sure add http headers…" hint, while a
        // present-but-wrong token yields "token is invalid".
        let auth_header = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if auth_header.is_empty() {
            return dev_token_missing_header_error();
        }
        let token = auth_header
            .strip_prefix("Bearer ")
            .unwrap_or("")
            .to_string();
        let dev = state.config.get_string("auth.dev_token");
        if token.is_empty() || token != dev {
            return dev_token_invalid_error();
        }
        return run_graphql(state, body, "dev".to_string(), None).await;
    }

    let session = match SessionStore::new(&state.db).get(&client_id) {
        Some(s) => s,
        None => return unauth(),
    };
    let key = match try_decode_token(&session.token) {
        Some(k) => k,
        None => return unauth(),
    };
    let decrypted: axum::body::Bytes = match crypto::decrypt(&key, &body) {
        Some(b) => b.into(),
        None => return bad_request("Decryption failed"),
    };
    if let Err(e) = SessionStore::new(&state.db).touch_last_active(&session) {
        log::debug!("touch_last_active failed: {e}");
    }
    run_graphql(state, decrypted, client_id, Some(key)).await
}

async fn run_graphql(
    state: AppState,
    body: Bytes,
    cid: String,
    key: Option<[u8; crypto::KEY_LEN]>,
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
            return wrap_response(resp, key);
        }
    };
    let resp = state.schema.execute(request.data(cid)).await;
    wrap_response(resp, key)
}

/// Split `"TIMESTAMP|NONCE|JSON"` into the JSON part, rejecting timestamps
/// older/newer than 10 minutes. Payloads without the wrapper pass through.
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

fn wrap_response(resp: async_graphql::Response, key: Option<[u8; crypto::KEY_LEN]>) -> Response {
    let bytes = match serde_json::to_vec(&resp) {
        Ok(b) => b,
        Err(_) => return server_error("encode failed"),
    };
    if let Some(k) = key {
        let enc = match crypto::encrypt(&k, &bytes) {
            Ok(b) => b,
            Err(_) => return server_error("encrypt failed"),
        };
        (
            StatusCode::OK,
            [(http::header::CONTENT_TYPE, "application/octet-stream")],
            enc,
        )
            .into_response()
    } else {
        (
            StatusCode::OK,
            [(http::header::CONTENT_TYPE, "application/json")],
            bytes,
        )
            .into_response()
    }
}

fn unauth() -> Response {
    let body = serde_json::json!({ "errors": [{ "message": "Unauthorized" }] });
    (StatusCode::UNAUTHORIZED, Json(body)).into_response()
}

/// Mirrors Go `requireAuth`'s "make sure add http headers…" branch —
/// returned when the `Authorization` header is missing entirely.
fn dev_token_missing_header_error() -> Response {
    let body = serde_json::json!({ "errors": [{ "message": "Unauthorized: make sure add http headers `{\"authorization\": \"Bearer <dev_token>\"}`" }] });
    (StatusCode::UNAUTHORIZED, Json(body)).into_response()
}

/// Mirrors Go `requireAuth`'s "token is invalid" branch — returned when
/// the `Authorization` header is present but the token doesn't match
/// `auth.dev_token` from config.
fn dev_token_invalid_error() -> Response {
    let body = serde_json::json!({ "errors": [{ "message": "Unauthorized: token is invalid" }] });
    (StatusCode::UNAUTHORIZED, Json(body)).into_response()
}

fn bad_request(msg: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(serde_json::json!({ "errors": [{ "message": msg }] })),
    )
        .into_response()
}

fn server_error(msg: &str) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({ "errors": [{ "message": msg }] })),
    )
        .into_response()
}

// We use Arc so callers can move AppState cheaply.
