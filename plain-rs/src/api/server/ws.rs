//! WebSocket handlers for the local server.
//!
//! * `/` (any non-`/status` path) — the chat event bus socket: the first
//!   Binary frame must be XChaCha20-encrypted with the URL token (same
//!   handshake as `/graphql`), then broadcast events stream until
//!   disconnect. Mirrors Android's `WebSocket.kt` non-`auth=1` flow.
//! * `/status` — peer online/offline socket: the first Binary frame is
//!   ChaCha20-decrypted with the peer shared key and the plaintext must
//!   carry an Ed25519 signature over `{timestamp}{cid}`.

use axum::extract::ws::{Message, WebSocket};
use std::sync::Arc;
use tokio::sync::broadcast;

use crate::api::context::{AppCtx, encode_ws_event};
use crate::{base64_decode, ed25519_verify, xchacha_decrypt, xchacha_decrypt_raw};

pub async fn chat_socket(socket: WebSocket, path: String, ctx: Arc<AppCtx>) {
    let cid = query_param(&path, "cid").unwrap_or_default();
    if cid.is_empty() {
        log::debug!("local_server chat_ws: `cid` is missing");
        return;
    }

    let mut socket = socket;
    let token = ctx.token.clone();

    // Auth handshake: first Binary frame must decrypt successfully with
    // the URL token.
    loop {
        match socket.recv().await {
            Some(Ok(Message::Binary(bytes))) => {
                if xchacha_decrypt(&token, &bytes).is_some() {
                    break; // authenticated
                } else {
                    log::debug!("local_server chat_ws: invalid_request cid={cid}");
                    let _ = socket.send(Message::Close(None)).await;
                    return;
                }
            }
            Some(Ok(Message::Close(_))) | None => return,
            Some(Err(_)) => return,
            _ => continue,
        }
    }

    log::debug!("local_server chat_ws: session added cid={cid}");
    let mut event_rx = ctx.event_tx.subscribe();
    log::info!(
        "local_server chat_ws: subscribed to event_tx for cid={cid} (initial receivers = {})",
        event_rx.len()
    );

    // Forward broadcast events to the client.
    loop {
        tokio::select! {
            event = event_rx.recv() => {
                match event {
                    Ok(ev) => {
                        log::info!(
                            "local_server chat_ws: forwarding event type={} to cid={cid}",
                            ev.event_type
                        );
                        if let Some(bytes) = encode_ws_event(&ev, &token) {
                            log::info!(
                                "local_server chat_ws: encoded event type={} bytes={} to cid={cid}",
                                ev.event_type, bytes.len()
                            );
                            match socket.send(Message::Binary(bytes)).await {
                                Ok(_) => log::info!(
                                    "local_server chat_ws: sent event type={} to cid={cid}",
                                    ev.event_type
                                ),
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
