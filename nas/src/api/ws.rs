//! `/` endpoint — WebSocket upgrade or SPA index, mirroring plain-app's
//! contract (plain-app serves its main WS channel at `/`). Two modes,
//! selected by the `auth` query parameter:
//!
//! * `auth=1` — the login/setup handshake from plain-desktop
//!   (`login-handshake.ts`): the first frame is a XChaCha20-Poly1305 blob
//!   encrypted with the password-hash key containing the auth request plus
//!   the client's ECDH public key. We verify the password, derive the session
//!   token via ECDH P-256, sign the response with the server's Ed25519 key
//!   and send it back encrypted. Mirrors plain-app's signed login protocol.
//! * no `auth` — event-bus mode: the first frame is a token-encrypted
//!   handshake; afterwards the socket receives typed event broadcasts.

use crate::api::auth::AppState;
use crate::api::auth::try_decode_token;
use crate::db::{PasswordStore, SessionInfo, SessionStore, SignatureKey};
use axum::extract::ws::rejection::WebSocketUpgradeRejection;
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use base64::Engine;
use futures::{SinkExt, StreamExt};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct WsParams {
    #[serde(default)]
    cid: String,
    #[serde(default)]
    auth: Option<String>,
}

/// `/` handler: WebSocket upgrade requests run the login/event channel;
/// every other GET serves the SPA index (a plain browser navigation of `/`
/// carries no upgrade headers and no `cid`).
pub async fn root_handler(
    State(state): State<AppState>,
    Query(p): Query<WsParams>,
    ws: Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
) -> Response {
    let ws = match ws {
        Ok(ws) => ws,
        Err(_) => return super::static_files::index().await,
    };
    if p.cid.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, "").into_response();
    }
    if p.auth.as_deref() == Some("1") {
        ws.on_upgrade(move |socket| auth_handshake(socket, state, p.cid))
            .into_response()
    } else {
        ws.on_upgrade(move |socket| handle_socket(socket, state, p.cid))
            .into_response()
    }
}

/// WebSocket close code 1013 ("Try Again Later"). Mirrors Go's
/// `websocket.CloseTryAgainLater`. Sent with the reason "invalid_request"
/// when the session lookup or the handshake decryption fails — matching
/// Go's `wsHandler` exactly.
const CLOSE_TRY_AGAIN_LATER: u16 = 1013;
const INVALID_REQUEST_REASON: &str = "invalid_request";
/// Wrong password during the login handshake — mirrors plain-app's
/// `ws.close(WsCloseCode.TRY_AGAIN_LATER, "invalid_password")`. The client
/// surfaces the close reason as the i18n key `login.invalid_password`.
const INVALID_PASSWORD_REASON: &str = "invalid_password";

async fn send_close(
    tx: &mut futures::stream::SplitSink<WebSocket, Message>,
    code: u16,
    reason: &str,
) {
    let _ = tx
        .send(Message::Close(Some(CloseFrame {
            code,
            reason: reason.to_owned().into(),
        })))
        .await;
}

async fn handle_socket(socket: WebSocket, state: AppState, cid: String) {
    let (mut tx, mut rx) = socket.split();
    let session = match SessionStore::new(&state.db).get(&cid) {
        Some(s) => s,
        None => {
            send_close(&mut tx, CLOSE_TRY_AGAIN_LATER, INVALID_REQUEST_REASON).await;
            return;
        }
    };
    let key = match try_decode_token(&session.token) {
        Some(k) => k,
        None => {
            send_close(&mut tx, CLOSE_TRY_AGAIN_LATER, INVALID_REQUEST_REASON).await;
            return;
        }
    };

    let handshake = match rx.next().await {
        Some(Ok(Message::Binary(b))) => b,
        _ => return,
    };
    if crate::crypto::decrypt(&key, &handshake).is_none() {
        send_close(&mut tx, CLOSE_TRY_AGAIN_LATER, INVALID_REQUEST_REASON).await;
        return;
    }

    // Register and run until either side closes.
    state.ws_hub.register(cid.clone(), key, tx).await;

    while let Some(msg) = rx.next().await {
        if msg.is_err() {
            break;
        }
    }
    state.ws_hub.unregister(&cid);
}

/// Decrypted login-handshake request from the client (`login-handshake.ts`
/// `ws.onopen`), plus the fields the server needs for the ECDH token swap.
#[derive(Debug, Deserialize)]
struct AuthRequest {
    #[serde(default)]
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
    #[serde(default, rename = "ecdhPublicKey")]
    ecdh_public_key: String,
}

/// The `auth=1` login handshake:
/// 1. decrypt the first frame with the stored password-hash key
/// 2. verify the password
/// 3. derive the session token via ECDH P-256 (SHA-256 of the shared secret)
/// 4. sign `clientId|status|ecdhPublicKey|timestamp` with the Ed25519 key
/// 5. send the encrypted response; the client closes the socket afterwards
async fn auth_handshake(socket: WebSocket, state: AppState, cid: String) {
    let (mut tx, mut rx) = socket.split();

    // The key the client used is the first half of the stored sha512 hex
    // (ASCII bytes), mirroring the REST `/auth` handler.
    let hash = PasswordStore::new(&state.prefs).get().unwrap_or_default();
    if hash.len() < crate::crypto::KEY_LEN {
        crate::log::warn!("[ws/auth] cid={cid} rejected: server has no password set");
        // Uninitialized server — the client must call `/auth/setup` first.
        send_close(&mut tx, CLOSE_TRY_AGAIN_LATER, INVALID_REQUEST_REASON).await;
        return;
    }
    let key: [u8; crate::crypto::KEY_LEN] = hash.as_bytes()[..crate::crypto::KEY_LEN]
        .try_into()
        .expect("key length checked above");

    let frame = match rx.next().await {
        Some(Ok(Message::Binary(b))) => b,
        other => {
            crate::log::warn!("[ws/auth] cid={cid} no binary frame: {other:?}");
            return;
        }
    };
    let plaintext = match crate::crypto::decrypt(&key, &frame) {
        Some(p) => p,
        None => {
            crate::log::warn!("[ws/auth] cid={cid} frame decrypt failed (wrong password?)");
            // Decryption failure means the password does not match.
            send_close(&mut tx, CLOSE_TRY_AGAIN_LATER, INVALID_PASSWORD_REASON).await;
            return;
        }
    };
    let req: AuthRequest = match serde_json::from_slice(&plaintext) {
        Ok(r) => r,
        Err(e) => {
            crate::log::warn!("[ws/auth] cid={cid} bad auth request JSON: {e}");
            send_close(&mut tx, CLOSE_TRY_AGAIN_LATER, INVALID_PASSWORD_REASON).await;
            return;
        }
    };
    if req.password != hash {
        crate::log::warn!("[ws/auth] cid={cid} password mismatch");
        send_close(&mut tx, CLOSE_TRY_AGAIN_LATER, INVALID_PASSWORD_REASON).await;
        return;
    }

    let client_pub = match base64::engine::general_purpose::STANDARD.decode(&req.ecdh_public_key) {
        Ok(v) => v,
        Err(e) => {
            crate::log::warn!("[ws/auth] cid={cid} bad ecdh public key: {e}");
            send_close(&mut tx, CLOSE_TRY_AGAIN_LATER, INVALID_REQUEST_REASON).await;
            return;
        }
    };
    let ecdh = plain_rs::crypto::EcdhSession::generate();
    let ecdh_public_b64 = base64::engine::general_purpose::STANDARD.encode(&ecdh.public_key_bytes);
    // Same derivation the client performs; both sides end up with this
    // 32-byte token, stored base64 in the session for later request bodies.
    let token = match ecdh.compute_shared_key(&client_pub) {
        Some(t) => t,
        None => {
            crate::log::warn!("[ws/auth] cid={cid} ecdh shared key failed");
            send_close(&mut tx, CLOSE_TRY_AGAIN_LATER, INVALID_REQUEST_REASON).await;
            return;
        }
    };
    let token_b64 = base64::engine::general_purpose::STANDARD.encode(token);

    let sessions = SessionStore::new(&state.db);
    let info = match sessions.get(&cid) {
        Some(s) => SessionInfo {
            token: token_b64.clone(),
            ..s
        },
        None => SessionInfo {
            client_id: cid.clone(),
            token: token_b64.clone(),
            client_name: client_name(
                &req.browser_name,
                &req.browser_version,
                &req.os_name,
                &req.os_version,
                req.is_mobile,
            ),
            browser_name: req.browser_name,
            browser_version: req.browser_version,
            os_name: req.os_name,
            os_version: req.os_version,
            is_mobile: req.is_mobile,
            ..Default::default()
        },
    };
    if let Err(e) = sessions.upsert(info) {
        crate::log::error!("[ws] failed to store login session: {e}");
        send_close(&mut tx, CLOSE_TRY_AGAIN_LATER, INVALID_REQUEST_REASON).await;
        return;
    }

    let timestamp: u64 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    // plain-app sends AuthStatus.COMPLETED and signs
    // `clientId|status|ecdhPublicKey|timestamp` (AuthResponse.toSignatureData).
    let status = "COMPLETED";
    let resp_client_id = crate::db::server_client_id(&state.prefs);
    // The client verifies `clientId|status|ecdhPublicKey|timestamp` using the
    // exact fields from this response — sign the same values we send.
    let signature = plain_rs::crypto::ed25519_sign(
        &SignatureKey::new(&state.prefs)
            .ensure_keypair()
            .unwrap_or([0u8; 64]),
        format!("{resp_client_id}|{status}|{ecdh_public_b64}|{timestamp}").as_bytes(),
    );
    let resp = serde_json::json!({
        "clientId": resp_client_id,
        "status": status,
        "ecdhPublicKey": ecdh_public_b64,
        "timestamp": timestamp,
        "signature": signature,
    });
    let encrypted = match crate::crypto::encrypt(&key, resp.to_string().as_bytes()) {
        Ok(v) => v,
        Err(_) => {
            send_close(&mut tx, CLOSE_TRY_AGAIN_LATER, INVALID_REQUEST_REASON).await;
            return;
        }
    };
    if tx.send(Message::Binary(encrypted)).await.is_err() {
        return;
    }

    // The client closes as soon as it derived the token; stay quiet until it
    // does (never register this socket on the event hub).
    while let Some(msg) = rx.next().await {
        if msg.is_err() {
            break;
        }
    }
}

fn client_name(
    browser_name: &str,
    browser_version: &str,
    os_name: &str,
    os_version: &str,
    is_mobile: bool,
) -> String {
    if browser_name.is_empty() {
        return String::new();
    }
    let mut name = browser_name.to_string();
    if !browser_version.is_empty() {
        name.push(' ');
        name.push_str(browser_version);
    }
    if !os_name.is_empty() {
        name.push_str(" / ");
        name.push_str(os_name);
        if !os_version.is_empty() {
            name.push(' ');
            name.push_str(os_version);
        }
    }
    if is_mobile {
        name.push_str(" (Mobile)");
    }
    name
}

#[cfg(test)]
#[path = "../../tests/unit/api/ws.rs"]
mod tests;
