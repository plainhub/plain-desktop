//! Request-key resolution — the single auth path shared by `/graphql`,
//! `/upload`, `/upload_chunk`, `/fs` and the WebSocket handshake.
//!
//! * Desktop host: every request is encrypted with the local server's
//!   URL token (base64 of 32 bytes) — [`RequestKey::Token`]. The `c-id`
//!   header is advisory (the upload handlers additionally enforce
//!   `cid == ctx.identity.client_id`).
//! * Nas host: the `c-id` header selects a session from the fjall
//!   `SessionStore`; the session token (base64 of 32 bytes) is the
//!   per-client key — [`RequestKey::Raw`]. With no `c-id`,
//!   `/graphql` additionally accepts the config `auth.dev_token`
//!   bearer (dev mode, plaintext JSON responses) —
//!   [`RequestKey::DevBearer`].

use axum::http::HeaderMap;

use super::{AuthPolicy, ServerState};
use crate::base64_decode;

/// The encrypt/decrypt key a request (or socket) is bound to.
#[derive(Clone, Debug)]
pub enum RequestKey {
    /// Base64-encoded URL token (desktop).
    Token(String),
    /// Raw 32-byte session key (nas).
    Raw([u8; 32]),
    /// Dev bearer token (nas dev mode): no encryption at all.
    DevBearer,
}

impl RequestKey {
    /// The raw 32-byte XChaCha20 key, when this key encrypts bodies.
    pub fn raw_key(&self) -> Option<[u8; 32]> {
        match self {
            RequestKey::Token(token) => decode_token_key(token),
            RequestKey::Raw(key) => Some(*key),
            RequestKey::DevBearer => None,
        }
    }
}

/// Why key resolution failed; every handler renders its own
/// host-appropriate response from this.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RequestKeyError {
    /// Nas: the `c-id` header is absent/empty.
    MissingCid,
    /// Nas: no session row for the `c-id`.
    SessionNotFound,
    /// Nas dev branch: no `Authorization` header at all.
    DevHeaderMissing,
    /// Nas dev branch: bearer token != config `auth.dev_token`.
    DevTokenInvalid,
    /// Nas: the session token is not a decodable 32-byte key.
    BadSessionToken,
}

fn decode_token_key(token: &str) -> Option<[u8; 32]> {
    let bytes = base64_decode(token);
    if bytes.len() != 32 {
        return None;
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&bytes);
    Some(key)
}

/// Resolve the per-request key + cid. `allow_dev_bearer` gates the nas
/// config-token branch (only `/graphql` accepts it; uploads and the
/// WebSocket require a real session).
#[cfg_attr(not(feature = "nas"), allow(unused_variables))]
pub fn resolve_request_key(
    state: &ServerState,
    headers: &HeaderMap,
    allow_dev_bearer: bool,
) -> Result<(RequestKey, String), RequestKeyError> {
    let cid = headers
        .get("c-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    if let AuthPolicy::Session { dev_token, .. } = &state.settings.auth {
        if cid.is_empty() {
            // Dev mode: trust the bearer token from config. In dev mode
            // we use the literal header value `dev` as the client id so
            // resolvers that need a client_id (file tasks, etc.) still
            // work. Mirrors Go `requireAuth`'s dev branch.
            if !allow_dev_bearer {
                return Err(RequestKeyError::MissingCid);
            }
            let auth_header = headers
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .unwrap_or("");
            if auth_header.is_empty() {
                return Err(RequestKeyError::DevHeaderMissing);
            }
            let token = auth_header.strip_prefix("Bearer ").unwrap_or("");
            if token.is_empty() || token != dev_token {
                return Err(RequestKeyError::DevTokenInvalid);
            }
            return Ok((RequestKey::DevBearer, "dev".to_string()));
        }

        let session = crate::media::kv::SessionStore::new(&state.ctx.media.db)
            .get(&cid)
            .ok_or(RequestKeyError::SessionNotFound)?;
        // Best-effort activity tracking (graphql used to do this after
        // decrypt; the position is not observable).
        let _ =
            crate::media::kv::SessionStore::new(&state.ctx.media.db).touch_last_active(&session);
        let key = crate::media::kv::token_key(&session.token)
            .map_err(|_| RequestKeyError::BadSessionToken)?;
        return Ok((RequestKey::Raw(key), cid));
    }

    Ok((RequestKey::Token(state.ctx.token.clone()), cid))
}

/// Decrypt a body with a resolved key. `None` = wrong key / corrupt
/// body.
pub fn decrypt_body(key: &RequestKey, body: &[u8]) -> Option<Vec<u8>> {
    match key {
        RequestKey::Token(token) => crate::xchacha_decrypt(token, body),
        RequestKey::Raw(raw) => crate::xchacha_decrypt_raw(raw, body),
        RequestKey::DevBearer => Some(body.to_vec()),
    }
}

/// Encrypt a response body with a resolved key. `DevBearer` passes the
/// plaintext through.
pub fn encrypt_body(key: &RequestKey, body: &[u8]) -> Option<Vec<u8>> {
    match key {
        RequestKey::Token(token) => crate::xchacha_encrypt(token, body),
        RequestKey::Raw(raw) => crate::xchacha_encrypt_raw(raw, body),
        RequestKey::DevBearer => Some(body.to_vec()),
    }
}
