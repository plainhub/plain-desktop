//! Auth handlers and shared AppState.

use crate::config::Config;
use crate::crypto;
use crate::db::{self, EventLog, PasswordStore, SessionInfo, SessionStore};
use axum::{
    Json,
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use base64::Engine;
use serde::Deserialize;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db: Arc<crate::db::Db>,
    pub prefs: Arc<crate::prefs::Prefs>,
    pub ws_hub: Arc<crate::ws_hub::WsHub>,
    pub cors: crate::api::cors::CorsPolicy,
    pub schema: crate::gql::AppSchema,
    pub chat: Arc<crate::chat::ChatState>,
    pub peer_schema: crate::gql::peer_schema::PeerSchema,
}

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

api_response! {
    struct AuthResponse { nas_id: String, token: String }
}

pub async fn auth_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let client_id = headers
        .get("c-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let store = PasswordStore::new(&state.prefs);
    if !store.has() {
        return status_error(StatusCode::CONFLICT, "Password not configured");
    }
    if body.is_empty() {
        return status_error(StatusCode::BAD_REQUEST, "Bad request");
    }
    let hash = store.get().unwrap_or_default();
    let mut key = [0u8; crypto::KEY_LEN];
    if hash.as_bytes().len() < crypto::KEY_LEN {
        return status_error(StatusCode::INTERNAL_SERVER_ERROR, "Server misconfigured");
    }
    key.copy_from_slice(&hash.as_bytes()[..crypto::KEY_LEN]);
    let decrypted = match crypto::decrypt(&key, &body) {
        Some(b) => b,
        None => {
            let _ = EventLog::new(&state.db).add("login_failed", "decrypt_failed", &client_id);
            return status_error(StatusCode::UNAUTHORIZED, "Unauthorized");
        }
    };
    let req: AuthRequest = match serde_json::from_slice(&decrypted) {
        Ok(v) => v,
        Err(_) => return status_error(StatusCode::BAD_REQUEST, "Bad request"),
    };
    if hash != req.password {
        let _ = EventLog::new(&state.db).add("login_failed", "bad_password", &client_id);
        return status_error(StatusCode::UNAUTHORIZED, "Unauthorized");
    }
    if client_id.is_empty() {
        let _ = EventLog::new(&state.db).add("login_failed", "missing_client_id", "");
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
    let sessions = SessionStore::new(&state.db);
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
    let _ = EventLog::new(&state.db).add("login", &client_name, &client_id);

    let nas_id = state.config.get_string("nas.id");
    let resp = AuthResponse {
        nas_id,
        token: session.token,
    };
    let plaintext = match serde_json::to_vec(&resp) {
        Ok(b) => b,
        Err(_) => return status_error(StatusCode::INTERNAL_SERVER_ERROR, "Encode failed"),
    };
    let mut resp_key = [0u8; crypto::KEY_LEN];
    if req.password.as_bytes().len() < crypto::KEY_LEN {
        return status_error(StatusCode::BAD_REQUEST, "Bad request");
    }
    resp_key.copy_from_slice(&req.password.as_bytes()[..crypto::KEY_LEN]);
    let encrypted = match crypto::encrypt(&resp_key, &plaintext) {
        Ok(b) => b,
        Err(_) => return status_error(StatusCode::INTERNAL_SERVER_ERROR, "Encryption failed"),
    };
    (
        StatusCode::OK,
        [(http::header::CONTENT_TYPE, "application/octet-stream")],
        encrypted,
    )
        .into_response()
}

api_response! {
    struct AuthStatusResponse {
        authenticated: bool,
        needs_setup: bool,
    }
}

pub async fn auth_status_handler(
    State(state): State<AppState>,
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
    let pw = PasswordStore::new(&state.prefs);
    let needs_setup = !pw.has();
    let mut authed = false;
    if !body.is_empty() {
        if let Some(session) = SessionStore::new(&state.db).get(client_id) {
            if let Ok(key) = db::token_key(&session.token) {
                if crypto::decrypt(&key, &body).is_some() {
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

api_response! {
    struct InitResponse {
        needs_setup: bool,
        signature_public_key: String,
    }
}

/// `POST /init` — the only pre-login API. Aligned with plain-app
/// (`SystemRoutes.kt` `post("/init")`): it never requires authentication and
/// never answers 401 — an initialized server answers `needsSetup: false` and
/// the client triages between auto-login (stored token) and the login form.
/// `needsSetup: true` is the plain-nas extension for first-run setup (plain-app
/// instead hands out a generated password). `signaturePublicKey` is the
/// server's stable Ed25519 public key (base64), used by clients for TOFU
/// verification of signed login responses. Like plain-app, a missing `c-id`
/// header is a 400.
pub async fn init_handler(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let client_id = headers
        .get("c-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if client_id.is_empty() {
        return status_error(StatusCode::BAD_REQUEST, "`c-id` is missing in the headers");
    }
    // A missing key (KV error) degrades to an empty value, which clients
    // treat as "server does not sign" rather than failing the flow.
    let signature_public_key = db::SignatureKey::new(&state.prefs)
        .ensure()
        .unwrap_or_default();
    let needs_setup = !PasswordStore::new(&state.prefs).has();
    (
        StatusCode::OK,
        Json(InitResponse {
            needs_setup,
            signature_public_key,
        }),
    )
        .into_response()
}

#[cfg(test)]
#[path = "../../tests/unit/api/auth.rs"]
mod tests;

pub async fn auth_setup_handler(State(state): State<AppState>, body: Bytes) -> Response {
    let pw = PasswordStore::new(&state.prefs);
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

pub fn try_decode_token(token: &str) -> Option<[u8; crypto::KEY_LEN]> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(token)
        .ok()?;
    if bytes.len() != crypto::KEY_LEN {
        return None;
    }
    let mut k = [0u8; crypto::KEY_LEN];
    k.copy_from_slice(&bytes);
    Some(k)
}
