//! Nas auth handlers: `/auth` (login), `/auth/status`, `/auth/setup`
//! and the nas branch of `/init` (`needsSetup` triage + signature key).
//!
//! Mirrors Go `cmd/services/api/auth.go` / `init.go`, aligned with
//! plain-app (`SystemRoutes.kt`): `/init` never requires authentication
//! and never answers 401. The handlers read the password/session/event
//! stores from the fjall kv and the identity keys from prefs.

use axum::{
    Json,
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Deserialize;

use super::{AuthPolicy, ServerState};
use crate::media::kv::{self, EventLog, PasswordStore, SessionInfo, SessionStore};

fn status_error(status: StatusCode, msg: &str) -> Response {
    let body = serde_json::json!({ "errors": [{ "message": msg }] });
    (status, Json(body)).into_response()
}

#[derive(Debug, Deserialize)]
struct AuthRequest {
    password: String,
    #[serde(default)]
    browser_name: String,
    #[serde(default)]
    browser_version: String,
    #[serde(default)]
    os_name: String,
    #[serde(default)]
    os_version: String,
    #[serde(default)]
    is_mobile: bool,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AuthResponse {
    nas_id: String,
    token: String,
}

pub async fn auth_handler(
    State(state): State<ServerState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let client_id = headers
        .get("c-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let store = PasswordStore::new(&state.ctx.prefs);
    if !store.has() {
        return status_error(StatusCode::CONFLICT, "Password not configured");
    }
    if body.is_empty() {
        return status_error(StatusCode::BAD_REQUEST, "Bad request");
    }
    let hash = store.get().unwrap_or_default();
    let mut key = [0u8; 32];
    if hash.as_bytes().len() < 32 {
        return status_error(StatusCode::INTERNAL_SERVER_ERROR, "Server misconfigured");
    }
    key.copy_from_slice(&hash.as_bytes()[..32]);
    let decrypted = match crate::xchacha_decrypt_raw(&key, &body) {
        Some(b) => b,
        None => {
            let _ = EventLog::new(&state.ctx.media.db).add(
                "login_failed",
                "decrypt_failed",
                &client_id,
            );
            return status_error(StatusCode::UNAUTHORIZED, "Unauthorized");
        }
    };
    let req: AuthRequest = match serde_json::from_slice(&decrypted) {
        Ok(v) => v,
        Err(_) => return status_error(StatusCode::BAD_REQUEST, "Bad request"),
    };
    if hash != req.password {
        let _ = EventLog::new(&state.ctx.media.db).add("login_failed", "bad_password", &client_id);
        return status_error(StatusCode::UNAUTHORIZED, "Unauthorized");
    }
    if client_id.is_empty() {
        let _ = EventLog::new(&state.ctx.media.db).add("login_failed", "missing_client_id", "");
        return status_error(StatusCode::BAD_REQUEST, "Missing client id");
    }
    let mut client_name = String::new();
    if !req.browser_name.is_empty() {
        client_name = req.browser_name.clone();
        if !req.browser_version.is_empty() {
            client_name.push(' ');
            client_name.push_str(&req.browser_version);
        }
        if !req.os_name.is_empty() {
            client_name.push_str(" / ");
            client_name.push_str(&req.os_name);
            if !req.os_version.is_empty() {
                client_name.push(' ');
                client_name.push_str(&req.os_version);
            }
        }
        if req.is_mobile {
            client_name.push_str(" (Mobile)");
        }
    }
    let sessions = SessionStore::new(&state.ctx.media.db);
    let session = match sessions.get(&client_id) {
        Some(s) => sessions.upsert(SessionInfo {
            client_name: client_name.clone(),
            ..s
        }),
        None => sessions.upsert(SessionInfo {
            client_id: client_id.clone(),
            token: String::new(),
            client_name: client_name.clone(),
            browser_name: req.browser_name.clone(),
            browser_version: req.browser_version.clone(),
            os_name: req.os_name.clone(),
            os_version: req.os_version.clone(),
            is_mobile: req.is_mobile,
            last_active: chrono::Utc::now(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }),
    };
    let session = match session {
        Ok(s) => s,
        Err(e) => return status_error(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };
    let _ = EventLog::new(&state.ctx.media.db).add("login", &client_name, &client_id);

    let nas_id = match &state.settings.auth {
        AuthPolicy::Session { device_id, .. } => device_id.clone(),
        AuthPolicy::LocalToken => String::new(),
    };
    let resp = AuthResponse {
        nas_id,
        token: session.token,
    };
    let plaintext = match serde_json::to_vec(&resp) {
        Ok(b) => b,
        Err(_) => return status_error(StatusCode::INTERNAL_SERVER_ERROR, "Encode failed"),
    };
    let mut resp_key = [0u8; 32];
    if req.password.as_bytes().len() < 32 {
        return status_error(StatusCode::BAD_REQUEST, "Bad request");
    }
    resp_key.copy_from_slice(&req.password.as_bytes()[..32]);
    match crate::xchacha_encrypt_raw(&resp_key, &plaintext) {
        Some(encrypted) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/octet-stream")],
            encrypted,
        )
            .into_response(),
        None => status_error(StatusCode::INTERNAL_SERVER_ERROR, "Encryption failed"),
    }
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AuthStatusResponse {
    authenticated: bool,
    needs_setup: bool,
}

pub async fn auth_status_handler(
    State(state): State<ServerState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let client_id = headers
        .get("c-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if client_id.is_empty() {
        return status_error(StatusCode::BAD_REQUEST, "`c-id` is missing in the headers");
    }
    let pw = PasswordStore::new(&state.ctx.prefs);
    let needs_setup = !pw.has();
    let mut authed = false;
    if !body.is_empty() {
        if let Some(session) = SessionStore::new(&state.ctx.media.db).get(client_id) {
            if let Ok(key) = kv::token_key(&session.token) {
                if crate::xchacha_decrypt_raw(&key, &body).is_some() {
                    authed = true;
                }
            }
        }
    }
    if needs_setup {
        authed = false;
    }
    let resp = AuthStatusResponse {
        authenticated: authed,
        needs_setup,
    };
    (StatusCode::OK, Json(resp)).into_response()
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct InitResponse {
    needs_setup: bool,
    signature_public_key: String,
}

/// `POST /init` (nas branch) — the only pre-login API. Aligned with
/// plain-app (`SystemRoutes.kt` `post("/init")`): it never requires
/// authentication and never answers 401 — an initialized server answers
/// `needsSetup: false` and the client triages between auto-login
/// (stored token) and the login form. `needsSetup: true` is the
/// plain-nas extension for first-run setup (plain-app instead hands out
/// a generated password). `signaturePublicKey` is the server's stable
/// Ed25519 public key (base64), used by clients for TOFU verification
/// of signed login responses. Like plain-app, a missing `c-id` header
/// is a 400.
pub async fn init_session(state: &ServerState, headers: &HeaderMap) -> Response {
    let client_id = headers
        .get("c-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if client_id.is_empty() {
        return status_error(StatusCode::BAD_REQUEST, "`c-id` is missing in the headers");
    }
    // A missing key (KV error) degrades to an empty value, which clients
    // treat as "server does not sign" rather than failing the flow.
    let signature_public_key = kv::SignatureKey::new(&state.ctx.prefs)
        .ensure()
        .unwrap_or_default();
    let needs_setup = !PasswordStore::new(&state.ctx.prefs).has();
    (
        StatusCode::OK,
        Json(InitResponse {
            needs_setup,
            signature_public_key,
        }),
    )
        .into_response()
}

pub async fn auth_setup_handler(State(state): State<ServerState>, body: Bytes) -> Response {
    let pw = PasswordStore::new(&state.ctx.prefs);
    if pw.has() {
        return status_error(StatusCode::CONFLICT, "Password already configured");
    }
    // Mirrors Go: read raw body, reject empty / non-JSON with "Bad request".
    if body.is_empty() {
        return status_error(StatusCode::BAD_REQUEST, "Bad request");
    }
    #[derive(serde::Deserialize)]
    struct AuthSetupRequest {
        #[serde(default)]
        password: String,
    }
    let req: AuthSetupRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(_) => return status_error(StatusCode::BAD_REQUEST, "Bad request"),
    };
    let h = req.password.trim().to_string();
    if h.len() != 128 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return status_error(StatusCode::BAD_REQUEST, "Invalid password hash");
    }
    if let Err(_) = pw.set(&h) {
        return status_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to save password");
    }
    (StatusCode::OK, "").into_response()
}

#[cfg(all(test, feature = "nas"))]
#[path = "../../../tests/unit/api/server/auth.rs"]
mod tests;
