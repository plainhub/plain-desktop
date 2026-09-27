//! Peer authentication for `/peer_graphql` requests.
//!
//! Faithful translation of plain-app `PeerGraphQLService.handle` +
//! `PeerChatParser.decrypt`.
//!
//! Trust chain:
//!   1. Look up the peer by the `c-id` header (must exist — needed for
//!      the Ed25519 public key).
//!   2. Pick the decryption key:
//!      * `c-cid` present → use channel key from `channel_key_cache`.
//!        Supports non-paired channel members (they have `PeerStatus::Channel`,
//!        not `Paired`). No paired check.
//!      * `c-cid` absent → require paired peer, use peer's shared key.
//!   3. Decrypt the body with XChaCha20-Poly1305.
//!   4. Split `signature|timestamp|{graphql_json}`.
//!   5. Verify timestamp is within ±5 minutes.
//!   6. Verify Ed25519 signature over `{timestamp}{graphql_json}`.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::base64_decode;
use crate::ed25519_verify;
use crate::xchacha_decrypt_raw;

use crate::chat::db::{ChatDb, DPeer};
use crate::chat::events::ChannelKeyCache;

/// Maximum allowed clock skew (forward or backward) for a peer request.
/// Mirrors `PeerChatParser.MAX_TIMESTAMP_DIFF_MS`.
const TIMESTAMP_WINDOW_MS: i64 = 5 * 60 * 1000;

/// Outcome of the auth chain. On success, carries the peer's key and the
/// parsed plaintext payload (already split into `signature`, `timestamp`,
/// and the GraphQL JSON body).
pub struct AuthenticatedPeer {
    pub peer: DPeer,
    pub key: Vec<u8>,
    pub signature_b64: String,
    pub timestamp: i64,
    pub graphql_json: String,
}

/// Failure modes the caller may want to map to different HTTP responses.
#[derive(Debug)]
pub enum AuthError {
    UnknownPeer,
    NotPaired,
    DecryptFailed,
    NotUtf8,
    TimestampExpired,
    BadSignature,
    MissingFields,
    NoChannelKey,
}

impl AuthError {
    /// Short, machine-readable reason string for logging / response bodies.
    pub fn reason(&self) -> &'static str {
        match self {
            AuthError::UnknownPeer => "unknown peer",
            AuthError::NotPaired => "not paired",
            AuthError::DecryptFailed => "decrypt failed",
            AuthError::NotUtf8 => "not utf-8",
            AuthError::TimestampExpired => "timestamp expired",
            AuthError::BadSignature => "bad signature",
            AuthError::MissingFields => "missing fields",
            AuthError::NoChannelKey => "no channel key",
        }
    }
}

/// Run the full auth chain for an incoming peer request.
///
/// Direct translation of `PeerGraphQLService.handle` (key selection) +
/// `PeerChatParser.decrypt` (decryption + signature/timestamp verification).
///
/// `header_client_id` is the peer id from the `c-id` header;
/// `header_channel_id` is the channel id from the `c-cid` header
/// (used to look up a per-channel key when present);
/// `channel_key_cache` carries the local node's known per-channel
/// keys; `body` is the raw (encrypted) request body.
pub fn authenticate(
    db: &ChatDb,
    header_client_id: &str,
    header_channel_id: &str,
    body: &[u8],
    channel_key_cache: &ChannelKeyCache,
) -> Result<AuthenticatedPeer, AuthError> {
    // The peer must exist — we need its publicKey for signature verification.
    // Mirrors plain-app: `PeerCacher.getPublicKeyBytes(clientId)` returns null
    // if the peer isn't in the cache, which triggers UNAUTHORIZED.
    let peer = db
        .get_peer_by_id(header_client_id)
        .ok_or(AuthError::UnknownPeer)?;

    // ── Key selection — mirrors `PeerGraphQLService.handle` ──
    //
    // val token = if (channelId.isNotEmpty()) {
    //     ChannelCacher.getKeyBytes(channelId)        // NO paired check
    // } else {
    //     val peer = PeerCacher.getPeer(clientId)
    //     if (peer == null || !peer.isPaired()) {     // paired check HERE
    //         call.respondNoBody(HttpStatus.FORBIDDEN)
    //         return
    //     }
    //     PeerCacher.getKeyBytes(clientId)
    // }
    let key = if !header_channel_id.is_empty() {
        // Channel message: use channel key, no paired check.
        // Non-paired channel members (PeerStatus::Channel) are allowed.
        let cache = channel_key_cache.read().unwrap();
        cache
            .get(header_channel_id)
            .cloned()
            .ok_or(AuthError::NoChannelKey)?
    } else {
        // Direct peer-to-peer message: require paired peer.
        if !peer.is_paired() {
            return Err(AuthError::NotPaired);
        }
        base64_decode(&peer.key)
    };

    // ── Decrypt — mirrors `PeerChatParser.decrypt` ──
    let plaintext_bytes = xchacha_decrypt_raw(&key, body).ok_or(AuthError::DecryptFailed)?;
    let plaintext = std::str::from_utf8(&plaintext_bytes)
        .map_err(|_| AuthError::NotUtf8)?
        .to_string();

    // Wire format: `signature|timestamp|{graphql_json}`
    let mut parts = plaintext.splitn(3, '|');
    let signature_b64 = parts.next().unwrap_or_default().to_string();
    let ts_str = parts.next().unwrap_or_default();
    let graphql_json = parts.next().unwrap_or_default().to_string();

    if signature_b64.is_empty() || graphql_json.is_empty() {
        return Err(AuthError::MissingFields);
    }

    let timestamp: i64 = ts_str.parse().unwrap_or(0);

    // ── Timestamp freshness — mirrors `PeerChatParser.decrypt` ──
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    if (now_ms - timestamp).abs() > TIMESTAMP_WINDOW_MS {
        return Err(AuthError::TimestampExpired);
    }

    // ── Signature verification — mirrors `PeerChatParser.decrypt` ──
    // Signature is computed over `{timestamp}{graphql_json}` (no separator).
    let sig_data = format!("{timestamp}{graphql_json}");
    if !ed25519_verify(&peer.public_key, sig_data.as_bytes(), &signature_b64) {
        return Err(AuthError::BadSignature);
    }

    Ok(AuthenticatedPeer {
        peer,
        key,
        signature_b64,
        timestamp,
        graphql_json,
    })
}

#[cfg(test)]
#[path = "../../tests/unit/chat/peer_auth.rs"]
mod tests;
