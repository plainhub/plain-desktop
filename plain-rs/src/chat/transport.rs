//! Outgoing peer-to-peer delivery over HTTPS `/peer_graphql`.
//!
//! The wire protocol (Ed25519-signed GraphQL JSON, XChaCha20-Poly1305
//! body, `c-id`/`c-cid` headers, encrypted response) lives here; the
//! actual HTTP POST is the app's job via the [`PeerTransport`] trait —
//! plain-desktop uses an async reqwest client, plain-nas uses ureq.

use serde_json::json;

use crate::ed25519_sign;
use crate::utils::http_url::build_url;
use crate::xchacha_decrypt_raw;
use crate::xchacha_encrypt_raw;

use crate::chat::db::DPeer;

/// The one transport seam of the chat stack: POST an encrypted body to a
/// peer URL with the `c-id` / `c-cid` headers and return the raw response
/// bytes. Implementations own their HTTP client, TLS policy (peers use
/// self-signed certs — certificate verification must be disabled) and
/// timeouts.
pub trait PeerTransport: Send + Sync {
    fn post<'a>(
        &'a self,
        url: &'a str,
        client_id: &'a str,
        channel_id: Option<&'a str>,
        body: &'a [u8],
    ) -> impl std::future::Future<Output = Result<Vec<u8>, String>> + Send;
}

/// Blanket forwarding impl so callers can pass `&Arc<T>` where `&T` is
/// expected (the service holds the transport behind an Arc).
impl<T: PeerTransport> PeerTransport for std::sync::Arc<T> {
    fn post<'a>(
        &'a self,
        url: &'a str,
        client_id: &'a str,
        channel_id: Option<&'a str>,
        body: &'a [u8],
    ) -> impl std::future::Future<Output = Result<Vec<u8>, String>> + Send {
        (**self).post(url, client_id, channel_id, body)
    }
}

/// Build all candidate URLs for a peer's /peer_graphql endpoint,
/// ordered with the most-recently-seen IP first.
pub fn peer_graphql_urls(peer: &DPeer) -> Vec<String> {
    peer.ip
        .split(',')
        .map(str::trim)
        .filter(|ip| !ip.is_empty())
        .map(|ip| build_url("https", ip, peer.port, "/peer_graphql"))
        .collect()
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// POST a `createChatItem` GraphQL request to `peer_graphql_urls`, trying
/// each URL in order and returning `Ok(())` on the first successful
/// delivery. Payload format: XChaCha20-Poly1305 over
/// `signature|timestamp|GraphQL_JSON`.
pub async fn deliver_to_peer<T: PeerTransport>(
    transport: &T,
    peer_graphql_urls: &[String],
    key: &[u8],
    client_id: &str,
    kp_bytes: &[u8],
    content: &str,
    channel_id: Option<&str>,
) -> Result<(), String> {
    let graphql_json = serde_json::to_string(&json!({
        "query": "mutation CreateChatItem($content: String!) { createChatItem(content: $content) { id fromId toId createdAt } }",
        "variables": { "content": content }
    }))
    .unwrap_or_default();

    let ts = now_ms();
    let sig_data = format!("{ts}{graphql_json}");
    let signature = ed25519_sign(kp_bytes, sig_data.as_bytes());
    let payload = format!("{signature}|{ts}|{graphql_json}");

    let Some(encrypted) = xchacha_encrypt_raw(key, payload.as_bytes()) else {
        return Err("encrypt failed".to_string());
    };

    let mut errors = Vec::new();

    for peer_graphql_url in peer_graphql_urls {
        let response = match transport
            .post(peer_graphql_url, client_id, channel_id, &encrypted)
            .await
        {
            Ok(bytes) => bytes,
            Err(e) => {
                log::warn!("deliver_to_peer request failed url={peer_graphql_url} error={e}");
                errors.push(format!("{peer_graphql_url}: {e}"));
                continue;
            }
        };
        let Some(decrypted) = xchacha_decrypt_raw(key, &response) else {
            log::warn!("deliver_to_peer failed decrypting response from {peer_graphql_url}");
            errors.push(format!("{peer_graphql_url}: failed decrypting response"));
            continue;
        };
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(&decrypted) else {
            log::warn!("deliver_to_peer invalid JSON from {peer_graphql_url}");
            errors.push(format!("{peer_graphql_url}: invalid JSON response"));
            continue;
        };
        if value.get("errors").is_some() {
            log::warn!("deliver_to_peer GraphQL errors from {peer_graphql_url}: {value}");
            errors.push(format!("{peer_graphql_url}: GraphQL errors {value}"));
            continue;
        }
        return Ok(());
    }

    Err(if errors.is_empty() {
        "delivery failed".to_string()
    } else {
        errors.join("; ")
    })
}

/// Send a `channelSystemMessage` GraphQL mutation to a peer over the same
/// transport used by `deliver_to_peer`. Returns `true` if the peer
/// acknowledged the request, `false` otherwise.
///
/// When `channel_id_opt` is `Some`, the request carries a `c-cid`
/// header so the receiver picks the channel key from its cache
/// instead of the peer's shared key. The wire body is still
/// encrypted with `key` — callers are responsible for passing the
/// correct key (channel key when `channel_id_opt.is_some()`,
/// otherwise the peer's shared key).
#[allow(clippy::too_many_arguments)]
pub async fn deliver_channel_system_message<T: PeerTransport>(
    transport: &T,
    peer: &DPeer,
    key: &[u8],
    client_id: &str,
    kp_bytes: &[u8],
    msg_type: &str,
    payload: &str,
    channel_id_opt: Option<&str>,
) -> bool {
    let graphql_json = serde_json::to_string(&json!({
        "query": "mutation ChannelSystemMessage($type: ChannelSystemMessageType!, $payload: String!) { channelSystemMessage(type: $type, payload: $payload) }",
        "variables": { "type": msg_type, "payload": payload }
    }))
    .unwrap_or_default();

    let ts = now_ms();
    let sig_data = format!("{ts}{graphql_json}");
    let signature = ed25519_sign(kp_bytes, sig_data.as_bytes());
    let wire = format!("{signature}|{ts}|{graphql_json}");

    let Some(encrypted) = xchacha_encrypt_raw(key, wire.as_bytes()) else {
        log::warn!("[channel] encrypt failed for {} ({msg_type})", peer.id);
        return false;
    };

    for url in peer_graphql_urls(peer) {
        let resp = transport
            .post(&url, client_id, channel_id_opt, &encrypted)
            .await;
        let Ok(bytes) = resp else {
            continue;
        };
        let Some(decrypted) = xchacha_decrypt_raw(key, &bytes) else {
            log::warn!("[channel] {} response decrypt failed from {url}", msg_type);
            continue;
        };
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(&decrypted) else {
            continue;
        };
        if value.get("errors").is_some() {
            log::warn!("[channel] {} GraphQL errors from {url}: {value}", msg_type);
            continue;
        }
        log::debug!("[channel] {} sent to {} via {url}", msg_type, peer.id);
        return true;
    }
    log::debug!(
        "[channel] {} delivery failed to {} (no reachable URL)",
        msg_type,
        peer.id
    );
    false
}

/// Verify an incoming peer message's Ed25519 signature. Currently a thin
/// wrapper over `ed25519_verify`; exposed so HTTP handlers can validate
/// the signature before dispatching to a sub-handler.
pub fn verify_peer_signature(public_key_b64: &str, sig_data: &[u8], sig_b64: &str) -> bool {
    crate::ed25519_verify(public_key_b64, sig_data, sig_b64)
}
