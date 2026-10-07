//! Web-login token issuance for the main web UI.
//!
//! The session token is the key to `/upload`, `/zip`, the token-mode
//! `/graphql` and the WS session frames, so it must have exactly one issuer.
//! Rust owns it: the `sessions` row is the single source of truth, and the
//! host only performs UI confirmation, notifications and socket delivery.

use super::server::ServerState;
use crate::db::SessionRow;
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256, Sha512};
use std::{
    collections::HashMap,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const LOGIN_ATTEMPT_LIMIT: usize = 5;
const LOGIN_ATTEMPT_WINDOW: Duration = Duration::from_secs(60);
const MAX_TRACKED_CLIENTS: usize = 256;

/// The key uses the first 32 ASCII bytes of the full SHA-512 hex digest.
fn hash_to_token(hash: &str) -> Vec<u8> {
    hash.as_bytes().iter().copied().take(32).collect()
}

/// The full SHA-512 hex digest — this is what the client sends and what
/// plain-app compares against. The 32-byte ChaCha20 key is a *separate*
/// truncation of the same hex string, so the two must never be conflated.
fn password_digest(prefs: &crate::prefs::Prefs) -> String {
    let password = prefs
        .get::<String>("password")
        .unwrap_or_default()
        .unwrap_or_default();
    crate::utils::hex::bytes_to_hex(&Sha512::digest(password.as_bytes()))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis() as u64)
        .unwrap_or_default()
}

#[derive(Default)]
pub(super) struct LoginAttempts {
    entries: HashMap<String, Vec<Instant>>,
    pending: HashMap<String, Pending>,
}

impl LoginAttempts {
    /// Sliding-window limiter so a wrong password cannot be brute forced from
    /// one address; keyed by client IP, falling back to the client id.
    fn acquire(&mut self, key: &str) -> bool {
        let now = Instant::now();
        if self.entries.len() > MAX_TRACKED_CLIENTS {
            self.entries.retain(|_, attempts| {
                attempts.retain(|at| now.duration_since(*at) < LOGIN_ATTEMPT_WINDOW);
                !attempts.is_empty()
            });
        }
        let attempts = self.entries.entry(key.to_owned()).or_default();
        attempts.retain(|at| now.duration_since(*at) < LOGIN_ATTEMPT_WINDOW);
        if attempts.len() >= LOGIN_ATTEMPT_LIMIT {
            return false;
        }
        attempts.push(now);
        true
    }
}

#[derive(Clone)]
struct Pending {
    client_id: String,
    client_ip: String,
    request: Value,
    at: Instant,
}

impl LoginAttempts {
    pub(super) fn cancel_client(&mut self, client_id: &str) {
        self.pending.retain(|_, value| value.client_id != client_id);
    }
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
pub(super) enum Request {
    Issue {
        #[serde(rename = "clientId")]
        client_id: String,
        #[serde(rename = "clientIp")]
        client_ip: String,
        frame: String,
    },
    Complete {
        #[serde(rename = "requestId")]
        request_id: String,
    },
    Cancel {
        #[serde(rename = "requestId")]
        request_id: String,
    },
    ResetPassword,
}

pub(super) fn reset_password(state: &ServerState) -> anyhow::Result<String> {
    use rand::Rng;
    let alphabet = b"23456789abcdefghijkmnpqrstuvwxyzABCDEFGHJKMNPQRSTUVWXYZ";
    let password: String = (0..6)
        .map(|_| alphabet[rand::thread_rng().gen_range(0..alphabet.len())] as char)
        .collect();
    state.prefs.set("password", &password)?;
    let _ = state
        .events
        .send(crate::ws_event::WsEvent::broadcast(47, "{}".to_owned()));
    Ok(password)
}

fn text(value: &Value, field: &str) -> String {
    value
        .get(field)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

/// A peer block is only accepted when it carries a usable port and a real
/// 32-byte signature key — the same guard plain-app applies before saving.
fn peer_chat_paired(request: &Value) -> bool {
    let Some(peer) = request.get("peer").filter(|value| !value.is_null()) else {
        return false;
    };
    let port = peer.get("port").and_then(Value::as_u64).unwrap_or(0);
    let key = text(peer, "signaturePublicKey");
    port >= 1 && port <= 65535 && crate::utils::base64::base64_decode(&key).len() == 32
}

fn signature_data(
    device_client_id: &str,
    status: &str,
    ecdh_public_key: &str,
    timestamp: u64,
    chat_paired: bool,
) -> String {
    let paired = if chat_paired { "|true" } else { "" };
    format!("{device_client_id}|{status}|{ecdh_public_key}|{timestamp}{paired}")
}

fn sign(prefs: &crate::prefs::Prefs, text: &str) -> anyhow::Result<String> {
    // `signature_key_pair` is plain-app's JSON keypair, not a bare base64
    // string. Handing the raw string bytes to ed25519_sign gave it something
    // that is not 64 bytes, so it returned an empty signature — the login
    // still reported COMPLETED and the client failed verification with
    // nothing wrong anywhere on the server.
    let keypair = super::peer_wire::signing_keypair(prefs)?;
    let signature = crate::crypto::ed25519_sign(&keypair, text.as_bytes());
    // An empty signature is a refusal, never a valid-looking answer.
    anyhow::ensure!(
        !signature.is_empty(),
        "ed25519 signing produced no signature"
    );
    Ok(signature)
}

async fn issue(state: &ServerState, request: &Pending) -> anyhow::Result<Value> {
    let device_client_id = state
        .prefs
        .get::<String>("client_id")?
        .ok_or_else(|| anyhow::anyhow!("device client id is missing"))?;
    let offered = text(&request.request, "password");
    anyhow::ensure!(offered == password_digest(&state.prefs), "invalid_password");
    let chat_paired = peer_chat_paired(&request.request);
    let keypair = crate::crypto::EcdhSession::generate();
    let peer_public = crate::utils::base64::base64_decode(&text(&request.request, "ecdhPublicKey"));
    let ecdh_public_key = crate::utils::base64::base64_encode(&keypair.public_key_bytes);
    let shared = keypair
        .compute_shared_key(&peer_public)
        .ok_or_else(|| anyhow::anyhow!("ECDH shared key computation failed"))?;
    let token = crate::utils::base64::base64_encode(&shared);
    let timestamp = now_ms();
    let signature = sign(
        &state.prefs,
        &signature_data(
            &device_client_id,
            "COMPLETED",
            &ecdh_public_key,
            timestamp,
            chat_paired,
        ),
    )?;
    let response = json!({
        "clientId": device_client_id,
        "status": "COMPLETED",
        "ecdhPublicKey": ecdh_public_key,
        "signature": signature,
        "timestamp": timestamp,
        "chatPaired": chat_paired,
    });
    persist(state, request, &token)?;
    if chat_paired {
        let peer = &request.request["peer"];
        let mut material = b"plain-chat-pairing-v1".to_vec();
        material.extend(&shared);
        let mut ips = vec![request.client_ip.clone()];
        if let Some(extra) = peer["ips"].as_array() {
            ips.extend(extra.iter().filter_map(Value::as_str).map(str::to_owned));
        }
        crate::chat::pairing::peer_store::save(
            &state.db,
            crate::chat::pairing::peer_store::Facts {
                id: request.client_id.clone(),
                name: text(peer, "deviceName"),
                ips,
                port: peer["port"].as_u64().unwrap() as u16,
                device_type: serde_json::from_value(peer["deviceType"].clone())?,
                key: crate::base64_encode(&Sha256::digest(&material)),
                public_key: text(peer, "signaturePublicKey"),
            },
        )?;
    }
    Ok(json!({"status": "COMPLETED", "token": token, "response": response}))
}

/// The browser's client id is the row key; the device's own client id and the
/// custom session name/type are preserved from any existing row.
fn persist(state: &ServerState, request: &Pending, token: &str) -> anyhow::Result<()> {
    let existing = state
        .db
        .session_get(&request.client_id)
        .map_err(|error| anyhow::Error::msg(error.to_string()))?;
    let now = now_ms().to_string();
    let row = SessionRow {
        client_id: request.client_id.clone(),
        name: existing
            .as_ref()
            .map(|row| row.name.clone())
            .unwrap_or_default(),
        r#type: existing
            .as_ref()
            .map(|row| row.r#type.clone())
            .unwrap_or_else(|| "WEB".into()),
        client_ip: request.client_ip.clone(),
        os_name: text(&request.request, "osName"),
        os_version: text(&request.request, "osVersion"),
        browser_name: text(&request.request, "browserName"),
        browser_version: text(&request.request, "browserVersion"),
        token: token.to_owned(),
        last_active_at: existing
            .as_ref()
            .and_then(|row| row.last_active_at.clone())
            .or_else(|| Some(now.clone())),
        created_at: existing
            .as_ref()
            .map(|row| row.created_at.clone())
            .unwrap_or_else(|| now.clone()),
        updated_at: now,
    };
    state
        .db
        .session_save(&row)
        .map_err(|error| anyhow::Error::msg(error.to_string()))?;
    Ok(())
}

pub(super) async fn execute(state: &ServerState, request: Request) -> anyhow::Result<Value> {
    if matches!(&request, Request::Issue { .. } | Request::Complete { .. }) {
        anyhow::ensure!(
            state.prefs.get_user_or("service", false)
                && state.prefs.get_user_or("desktop_access", true),
            "desktop_access_disabled"
        );
    }
    let (pending, request_id) = match request {
        Request::ResetPassword => return Ok(json!({"password":reset_password(state)?})),
        Request::Cancel { request_id } => {
            state
                .login_attempts
                .lock()
                .map_err(|_| anyhow::anyhow!("Login state unavailable"))?
                .pending
                .remove(&request_id);
            return Ok(json!({"cancelled":true}));
        }
        Request::Complete { request_id } => {
            let pending = state
                .login_attempts
                .lock()
                .map_err(|_| anyhow::anyhow!("Login state unavailable"))?
                .pending
                .remove(&request_id)
                .ok_or_else(|| anyhow::anyhow!("Login confirmation is no longer pending"))?;
            anyhow::ensure!(
                pending.at.elapsed() < Duration::from_secs(300),
                "Login confirmation expired"
            );
            (pending, request_id)
        }
        Request::Issue {
            client_id,
            client_ip,
            frame,
        } => {
            anyhow::ensure!(!client_id.is_empty(), "Client id is required");
            let allowed = state
                .login_attempts
                .lock()
                .map_err(|_| anyhow::anyhow!("Login state unavailable"))?
                .acquire(&if client_ip.is_empty() {
                    format!("cid:{client_id}")
                } else {
                    client_ip.clone()
                });
            anyhow::ensure!(allowed, "too_many_login_attempts");
            let key = hash_to_token(&password_digest(&state.prefs));
            let bytes = crate::crypto::xchacha_decrypt_raw(&key, &crate::base64_decode(&frame))
                .ok_or_else(|| anyhow::anyhow!("invalid_password"))?;
            let request: Value = serde_json::from_slice(&bytes)?;
            for field in ["browserName", "browserVersion", "osName", "osVersion"] {
                anyhow::ensure!(
                    request[field].is_string(),
                    "Invalid login metadata: {field}"
                );
            }
            anyhow::ensure!(
                request["isMobile"].is_boolean(),
                "Invalid login metadata: isMobile"
            );
            if let Some(peer) = request.get("peer").filter(|peer| !peer.is_null()) {
                anyhow::ensure!(
                    peer["deviceName"].is_string()
                        && peer["signaturePublicKey"].is_string()
                        && peer["port"]
                            .as_i64()
                            .is_some_and(|port| i32::try_from(port).is_ok()),
                    "Invalid peer metadata"
                );
                let _: crate::chat::enums::DeviceType =
                    serde_json::from_value(peer["deviceType"].clone())?;
                let _: Vec<String> = serde_json::from_value(peer["ips"].clone())?;
            }

            anyhow::ensure!(
                text(&request, "password") == password_digest(&state.prefs),
                "invalid_password"
            );
            let pending = Pending {
                client_id,
                client_ip,
                request,
                at: Instant::now(),
            };
            let id = uuid::Uuid::new_v4().to_string();
            if peer_chat_paired(&pending.request) || state.prefs.get_or("auth_two_factor", true) {
                let response = json!({"clientId":state.prefs.get_or("client_id",String::new()), "status":"PENDING"});
                let encrypted =
                    crate::crypto::xchacha_encrypt_raw(&key, &serde_json::to_vec(&response)?)
                        .ok_or_else(|| anyhow::anyhow!("Response encryption failed"))?;
                let mut attempts = state
                    .login_attempts
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Login state unavailable"))?;
                attempts
                    .pending
                    .retain(|_, item| item.at.elapsed() < Duration::from_secs(300));
                attempts.cancel_client(&pending.client_id);
                attempts.pending.insert(id.clone(), pending.clone());
                return Ok(
                    json!({"status":"PENDING","requestId":id,"request":pending.request,"frame":crate::base64_encode(&encrypted)}),
                );
            }
            (pending, id)
        }
    };
    let mut result = issue(state, &pending).await?;
    let key = hash_to_token(&password_digest(&state.prefs));
    let bytes = crate::crypto::xchacha_encrypt_raw(&key, &serde_json::to_vec(&result["response"])?)
        .ok_or_else(|| anyhow::anyhow!("Response encryption failed"))?;
    result["frame"] = json!(crate::base64_encode(&bytes));
    result["requestId"] = json!(request_id);
    result["request"] = pending.request;
    Ok(result)
}

pub(super) async fn call(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(request): Json<Request>,
) -> Response {
    if !state.authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match execute(&state, request).await {
        Ok(value) => Json(value).into_response(),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":error.to_string()})),
        )
            .into_response(),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/content_api/ws_login.rs"]
mod tests;
