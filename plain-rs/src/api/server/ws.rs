//! WebSocket handlers for the shared router.
//!
//! * The chat event socket (nas `/`, desktop any non-`/status` path):
//!   cid from `?cid=`; the key is the nas session token or the desktop
//!   URL token; the first Binary frame must XChaCha20-decrypt with it
//!   (failure closes with 1013 `invalid_request`, the plain-app
//!   contract close code). Afterwards broadcast events stream until
//!   disconnect — `WsEvent`s with a `target_cid` reach only the
//!   matching socket. Mirrors Android's `WebSocket.kt` non-`auth=1`
//!   flow plus the nas event channel.
//! * `/status` (desktop) — peer online/offline socket: the first Binary
//!   frame is XChaCha20-decrypted with the peer shared key and the
//!   plaintext must carry an Ed25519 signature over
//!   `{timestamp}{cid}`.
//! * `auth=1` (nas `/`) — the login/setup handshake from plain-desktop
//!   (`login-handshake.ts`): the first frame is a XChaCha20-Poly1305
//!   blob encrypted with the password-hash key containing the auth
//!   request plus the client's ECDH public key. We verify the password,
//!   derive the session token via ECDH P-256, sign the response with
//!   the server's Ed25519 key and send it back encrypted. Mirrors
//!   plain-app's signed login protocol.

use std::sync::Arc;
use tokio::sync::broadcast;

use axum::extract::ws::Message;
use axum::extract::ws::WebSocket;

use crate::api::context::AppCtx;
use crate::api::server::ServerState;
use crate::{base64_decode, ed25519_verify, xchacha_decrypt_raw};

/// WebSocket close code 1013 ("Try Again Later"). Mirrors Go's
/// `websocket.CloseTryAgainLater` and plain-app's
/// `ws.close(WsCloseCode.TRY_AGAIN_LATER, …)`. Sent with the reason
/// "invalid_request" when the session lookup or the handshake
/// decryption fails — matching the plain-app contract close codes on
/// both hosts.
const CLOSE_TRY_AGAIN_LATER: u16 = 1013;
const INVALID_REQUEST_REASON: &str = "invalid_request";
/// Wrong password during the login handshake — mirrors plain-app's
/// `ws.close(WsCloseCode.TRY_AGAIN_LATER, "invalid_password")`. The
/// client surfaces the close reason as the i18n key
/// `login.invalid_password`.
#[cfg(feature = "nas")]
const INVALID_PASSWORD_REASON: &str = "invalid_password";

pub async fn chat_socket(socket: WebSocket, path: String, state: ServerState) {
    let cid = query_param(&path, "cid").unwrap_or_default();
    if cid.is_empty() {
        log::debug!("local_server chat_ws: `cid` is missing");
        return;
    }
    chat_socket_cid(socket, cid, state).await;
}

/// The chat event socket once the cid is known (the nas `/` handler has
/// it from the upgrade query, the desktop fallback extracts it from the
/// raw path).
pub async fn chat_socket_cid(socket: WebSocket, cid: String, state: ServerState) {
    // Resolve the connection key per host: the nas session token for
    // this cid, the desktop URL token.
    let key = if matches!(
        state.settings.auth,
        crate::api::server::AuthPolicy::Session { .. }
    ) {
        let session = crate::media::kv::SessionStore::new(&state.ctx.media.db).get(&cid);
        match session.and_then(|s| crate::media::kv::token_key(&s.token).ok()) {
            Some(k) => k,
            None => {
                let mut socket = socket;
                send_close(&mut socket, CLOSE_TRY_AGAIN_LATER, INVALID_REQUEST_REASON).await;
                return;
            }
        }
    } else {
        match token_raw_key(&state.ctx.token) {
            Some(k) => k,
            None => {
                log::warn!("local_server chat_ws: bad token for cid={cid}");
                return;
            }
        }
    };

    let mut socket = socket;

    // Auth handshake: first Binary frame must decrypt successfully with
    // the connection key.
    loop {
        match socket.recv().await {
            Some(Ok(Message::Binary(bytes))) => {
                if xchacha_decrypt_raw(&key, &bytes).is_some() {
                    break; // authenticated
                } else {
                    log::debug!("local_server chat_ws: invalid_request cid={cid}");
                    send_close(&mut socket, CLOSE_TRY_AGAIN_LATER, INVALID_REQUEST_REASON).await;
                    return;
                }
            }
            Some(Ok(Message::Close(_))) | None => return,
            Some(Err(_)) => return,
            _ => continue,
        }
    }

    log::debug!("local_server chat_ws: session added cid={cid}");
    let ctx = state.ctx.clone();
    let mut event_rx = ctx.event_tx.subscribe();
    log::info!(
        "local_server chat_ws: subscribed to event_tx for cid={cid} (initial receivers = {})",
        event_rx.len()
    );

    // Forward broadcast events to the client: broadcast events to every
    // socket, targeted events only to the owning cid.
    loop {
        tokio::select! {
            event = event_rx.recv() => {
                match event {
                    Ok(ev) => {
                        if let Some(target) = ev.target_cid.as_ref() {
                            if target != &cid {
                                continue;
                            }
                        }
                        log::info!(
                            "local_server chat_ws: forwarding event type={} to cid={cid}",
                            ev.event_type
                        );
                        if let Some(bytes) =
                            crate::ws_frame::encode(ev.event_type, ev.payload.as_bytes(), &key)
                        {
                            match socket.send(Message::Binary(bytes)).await {
                                Ok(_) => {}
                                Err(e) => {
                                    log::warn!(
                                        "local_server chat_ws: send failed type={} cid={cid} err={e}",
                                        ev.event_type
                                    );
                                    break;
                                }
                            }
                        } else {
                            log::warn!(
                                "local_server chat_ws: encode failed type={} cid={cid}",
                                ev.event_type
                            );
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        log::warn!("local_server chat_ws: lagged by {n} events for cid={cid}");
                        continue;
                    }
                    Err(_) => break,
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(_)) => break,
                    _ => {}
                }
            }
        }
    }

    log::debug!("local_server chat_ws: session removed cid={cid}");
}

/// The desktop URL token as a raw 32-byte key.
fn token_raw_key(token: &str) -> Option<[u8; 32]> {
    let bytes = base64_decode(token);
    if bytes.len() != 32 {
        return None;
    }
    let mut key = [0u8; 32];
    key.copy_from_slice(&bytes);
    Some(key)
}

async fn send_close(socket: &mut WebSocket, code: u16, reason: &str) {
    use axum::extract::ws::CloseFrame;
    let _ = socket
        .send(Message::Close(Some(CloseFrame {
            code,
            reason: reason.to_owned().into(),
        })))
        .await;
}

pub async fn status_socket(socket: WebSocket, path: String, ctx: Arc<AppCtx>) {
    let Some(peer_id) = query_param(&path, "cid").filter(|v| !v.is_empty()) else {
        log::debug!("local_server status_ws: `cid` is missing");
        return;
    };

    log::debug!("local_server status_ws: new connection peer_id={peer_id}");
    let mut socket = socket;
    let mut authenticated = false;

    while let Some(message) = socket.recv().await {
        match message {
            Ok(Message::Binary(bytes)) if !authenticated => {
                if authenticate_peer(&peer_id, &bytes, &ctx) {
                    authenticated = true;
                    ctx.peer_status.set_online(&peer_id, true);
                    if socket.send(Message::Text("ok".into())).await.is_err() {
                        break;
                    }
                } else {
                    log::debug!("local_server status_ws: auth failed peer_id={peer_id}");
                    let _ = socket.send(Message::Close(None)).await;
                    break;
                }
            }
            Ok(Message::Close(_)) => break,
            Err(_) => break,
            _ => {}
        }
    }

    if authenticated {
        ctx.peer_status.disconnected(&peer_id);
    }
}

/// Extract a single query parameter value from a path string like `/foo?a=1&b=2`.
/// Value is percent-decoded (so `cid=hello%20world` becomes `hello world`).
pub fn query_param(path: &str, key: &str) -> Option<String> {
    crate::query::query_get(path, key)
}

/// Verify the auth payload sent by the peer on connect.
///
/// Expected plaintext after ChaCha20 decryption: `{sig}|{timestamp_ms}|{cid}`
/// where `sig` is an Ed25519 signature over `{timestamp_ms}{cid}`.
fn authenticate_peer(peer_id: &str, payload: &[u8], ctx: &AppCtx) -> bool {
    log::debug!(
        "status_ws auth: peer_id={peer_id} payload_len={}",
        payload.len()
    );
    let Some(peer) = ctx.db.get_peer_by_id(peer_id) else {
        log::debug!("status_ws auth: peer not found peer_id={peer_id}");
        return false;
    };
    log::debug!(
        "status_ws auth: peer found is_paired={} key_len={} pubkey_len={}",
        peer.is_paired(),
        peer.key.len(),
        peer.public_key.len()
    );
    if !peer.is_paired() || peer.key.is_empty() || peer.public_key.is_empty() {
        log::debug!("status_ws auth: peer not ready peer_id={peer_id}");
        return false;
    }
    let key = base64_decode(&peer.key);
    log::debug!("status_ws auth: decoded key_len={}", key.len());
    if key.len() != 32 {
        log::debug!("status_ws auth: bad key length {} (expected 32)", key.len());
        return false;
    }
    let Some(plaintext) = xchacha_decrypt_raw(&key, payload) else {
        log::debug!("status_ws auth: xchacha decrypt failed peer_id={peer_id}");
        return false;
    };
    let Ok(text) = std::str::from_utf8(&plaintext) else {
        log::debug!("status_ws auth: plaintext is not valid utf8 peer_id={peer_id}");
        return false;
    };
    log::debug!("status_ws auth: plaintext={text:?}");
    let mut parts = text.splitn(3, '|');
    let signature = parts.next().unwrap_or_default();
    let timestamp = parts.next().unwrap_or_default();
    let client_id = parts.next().unwrap_or_default();
    log::debug!(
        "status_ws auth: sig_len={} timestamp={timestamp} client_id={client_id}",
        signature.len()
    );
    if client_id != peer_id {
        log::debug!("status_ws auth: client_id mismatch: got={client_id} expected={peer_id}");
        return false;
    }
    let Ok(timestamp_ms) = timestamp.parse::<i64>() else {
        log::debug!("status_ws auth: timestamp parse failed: {timestamp:?}");
        return false;
    };
    let diff = (now_ms() - timestamp_ms).abs();
    log::debug!("status_ws auth: timestamp_diff_ms={diff}");
    if diff > 5 * 60 * 1000 {
        log::debug!("status_ws auth: timestamp expired diff_ms={diff}");
        return false;
    }
    let sig_input = format!("{timestamp}{client_id}");
    let ok = ed25519_verify(&peer.public_key, sig_input.as_bytes(), signature);
    log::debug!("status_ws auth: ed25519_verify={ok} sig_input={sig_input:?}");
    ok
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// ── Nas `/` root: WebSocket upgrade + SPA index split, and the
/// `auth=1` login handshake ─────────────────────────────────────────

#[cfg(feature = "nas")]
mod login {
    use super::*;

    use axum::extract::ws::rejection::WebSocketUpgradeRejection;
    use axum::extract::ws::{CloseFrame, WebSocketUpgrade};
    use axum::extract::{Query, State};
    use axum::http::StatusCode;
    use axum::response::{IntoResponse, Response};
    use base64::Engine;
    use futures_util::{SinkExt, StreamExt};
    use serde::Deserialize;

    #[derive(Deserialize)]
    pub struct WsParams {
        #[serde(default)]
        pub cid: String,
        #[serde(default)]
        pub auth: Option<String>,
    }

    /// `/` handler: WebSocket upgrade requests run the login/event
    /// channel; every other GET serves the SPA index (a plain browser
    /// navigation of `/` carries no upgrade headers and no `cid`).
    pub async fn root_handler(
        State(state): State<ServerState>,
        Query(p): Query<WsParams>,
        ws: Result<WebSocketUpgrade, WebSocketUpgradeRejection>,
    ) -> Response {
        let ws = match ws {
            Ok(ws) => ws,
            Err(_) => return super::super::static_files::index().await,
        };
        if p.cid.trim().is_empty() {
            return (StatusCode::BAD_REQUEST, "").into_response();
        }
        if p.auth.as_deref() == Some("1") {
            ws.on_upgrade(move |socket| auth_handshake(socket, state, p.cid))
                .into_response()
        } else {
            ws.on_upgrade(move |socket| super::chat_socket_cid(socket, p.cid, state))
                .into_response()
        }
    }

    async fn send_close(
        tx: &mut futures_util::stream::SplitSink<WebSocket, Message>,
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

    /// Decrypted login-handshake request from the client
    /// (`login-handshake.ts` `ws.onopen`), plus the fields the server
    /// needs for the ECDH token swap.
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
    /// 3. derive the session token via ECDH P-256 (SHA-256 of the
    ///    shared secret)
    /// 4. sign `clientId|status|ecdhPublicKey|timestamp` with the
    ///    Ed25519 key
    /// 5. send the encrypted response; the client closes the socket
    ///    afterwards
    async fn auth_handshake(socket: WebSocket, state: ServerState, cid: String) {
        let (mut tx, mut rx) = socket.split();

        // The key the client used is the first half of the stored sha512
        // hex (ASCII bytes), mirroring the REST `/auth` handler.
        let hash = crate::media::kv::PasswordStore::new(&state.ctx.prefs)
            .get()
            .unwrap_or_default();
        if hash.len() < 32 {
            log::warn!("[ws/auth] cid={cid} rejected: server has no password set");
            // Uninitialized server — the client must call `/auth/setup`
            // first.
            send_close(&mut tx, CLOSE_TRY_AGAIN_LATER, INVALID_REQUEST_REASON).await;
            return;
        }
        let key: [u8; 32] = hash.as_bytes()[..32]
            .try_into()
            .expect("key length checked above");

        let frame = match rx.next().await {
            Some(Ok(Message::Binary(b))) => b,
            other => {
                log::warn!("[ws/auth] cid={cid} no binary frame: {other:?}");
                return;
            }
        };
        let plaintext = match xchacha_decrypt_raw(&key, &frame) {
            Some(p) => p,
            None => {
                log::warn!("[ws/auth] cid={cid} frame decrypt failed (wrong password?)");
                // Decryption failure means the password does not match.
                send_close(&mut tx, CLOSE_TRY_AGAIN_LATER, INVALID_PASSWORD_REASON).await;
                return;
            }
        };
        let req: AuthRequest = match serde_json::from_slice(&plaintext) {
            Ok(r) => r,
            Err(e) => {
                log::warn!("[ws/auth] cid={cid} bad auth request JSON: {e}");
                send_close(&mut tx, CLOSE_TRY_AGAIN_LATER, INVALID_PASSWORD_REASON).await;
                return;
            }
        };
        if req.password != hash {
            log::warn!("[ws/auth] cid={cid} password mismatch");
            send_close(&mut tx, CLOSE_TRY_AGAIN_LATER, INVALID_PASSWORD_REASON).await;
            return;
        }

        let client_pub =
            match base64::engine::general_purpose::STANDARD.decode(&req.ecdh_public_key) {
                Ok(v) => v,
                Err(e) => {
                    log::warn!("[ws/auth] cid={cid} bad ecdh public key: {e}");
                    send_close(&mut tx, CLOSE_TRY_AGAIN_LATER, INVALID_REQUEST_REASON).await;
                    return;
                }
            };
        let ecdh = crate::crypto::EcdhSession::generate();
        let ecdh_public_b64 =
            base64::engine::general_purpose::STANDARD.encode(&ecdh.public_key_bytes);
        // Same derivation the client performs; both sides end up with
        // this 32-byte token, stored base64 in the session for later
        // request bodies.
        let token = match ecdh.compute_shared_key(&client_pub) {
            Some(t) => t,
            None => {
                log::warn!("[ws/auth] cid={cid} ecdh shared key failed");
                send_close(&mut tx, CLOSE_TRY_AGAIN_LATER, INVALID_REQUEST_REASON).await;
                return;
            }
        };
        let token_b64 = base64::engine::general_purpose::STANDARD.encode(token);

        let sessions = crate::media::kv::SessionStore::new(&state.ctx.media.db);
        let info = match sessions.get(&cid) {
            Some(s) => crate::media::kv::SessionInfo {
                token: token_b64.clone(),
                ..s
            },
            None => crate::media::kv::SessionInfo {
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
            log::error!("[ws] failed to store login session: {e}");
            send_close(&mut tx, CLOSE_TRY_AGAIN_LATER, INVALID_REQUEST_REASON).await;
            return;
        }

        let timestamp: u64 = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        // plain-app sends AuthStatus.COMPLETED and signs
        // `clientId|status|ecdhPublicKey|timestamp`
        // (AuthResponse.toSignatureData).
        let status = "COMPLETED";
        let resp_client_id = crate::media::kv::server_client_id(&state.ctx.prefs);
        // The client verifies `clientId|status|ecdhPublicKey|timestamp`
        // using the exact fields from this response — sign the same
        // values we send.
        let signature = crate::crypto::ed25519_sign(
            &crate::media::kv::SignatureKey::new(&state.ctx.prefs)
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
        let encrypted = match crate::xchacha_encrypt_raw(&key, resp.to_string().as_bytes()) {
            Some(v) => v,
            None => {
                send_close(&mut tx, CLOSE_TRY_AGAIN_LATER, INVALID_REQUEST_REASON).await;
                return;
            }
        };
        if tx.send(Message::Binary(encrypted)).await.is_err() {
            return;
        }

        // The client closes as soon as it derived the token; stay quiet
        // until it does (never register this socket on the event hub).
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
}

#[cfg(feature = "nas")]
pub use login::root_handler;

#[cfg(all(test, feature = "nas"))]
#[path = "../../../tests/unit/api/server/ws_nas.rs"]
mod nas_tests;
