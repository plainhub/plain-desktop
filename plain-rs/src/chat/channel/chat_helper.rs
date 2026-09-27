//! Faithful translation of plain-app `ChannelChatSender.kt`.
//!
//! ```kotlin
//! object ChannelChatSender {
//!     sealed class Result {
//!         data class Status(val data: DMessageStatusData) : Result()
//!         data object NoLeader : Result()
//!         data class LeaderPeerMissing(val leaderId: String) : Result()
//!     }
//!     suspend fun send(channel, content, onlinePeerIds = emptySet()): Result
//!     private suspend fun broadcastAsLeader(channel, content): DMessageStatusData
//!     suspend fun sendToRecipients(channel, recipientIds, content): DMessageStatusData
//!     private suspend fun sendToLeader(channel, leaderId, content): Result
//!     suspend fun sendToMember(channel, peer, content): DMessageDeliveryResult
//! }
//! ```
//!
//! Key behavioural notes (verified against the Kotlin source):
//! * `send()` is called with `onlinePeerIds = emptySet()` (default) — only
//!   self is considered online. Self is always the leader when joined.
//! * `sendToMember()` calls `PeerGraphQLClient.createChannelChatItem()` which
//!   REQUIRES the channel key — no fallback to peer key.
//! * The `c-cid` header is always set for channel chat items, so the receiver
//!   uses the channel key for decryption (no paired check).

use std::collections::HashSet;

use serde_json::{Value, json};

use crate::chat::db::{ChatDb, DChannel, DPeer};
use crate::chat::enums::ChatStatus;
use crate::chat::events::ChannelKeyCache;
use crate::chat::transport::{PeerTransport, deliver_to_peer, peer_graphql_urls};

// ── DMessageDeliveryResult ──────────────────────────────────────────────────

/// One delivery attempt result, matching Kotlin's `DMessageDeliveryResult`.
/// `error` is `None` when the message was delivered successfully.
#[derive(Debug, Clone)]
pub struct ChannelDeliveryResult {
    pub peer_id: String,
    pub peer_name: String,
    pub error: Option<String>,
}

impl ChannelDeliveryResult {
    pub fn to_json(&self) -> Value {
        json!({
            "peerId": self.peer_id,
            "peerName": self.peer_name,
            "error": self.error.as_deref().map_or(Value::Null, |e| json!(e)),
        })
    }
}

// ── ChannelChatSender.Result ────────────────────────────────────────────────

/// Direct translation of `ChannelChatSender.Result` sealed class.
pub enum SendResult {
    /// `Result.Status(data)` — delivery attempted, carries per-member results.
    Status(Vec<ChannelDeliveryResult>),
    /// `Result.NoLeader` — no online joined member to relay through.
    NoLeader,
    /// `Result.LeaderPeerMissing(leaderId)` — leader was elected but not in DB.
    LeaderPeerMissing(()),
}

// ── ChannelChatSender.send ──────────────────────────────────────────────────

/// Send a chat item to the channel.
///
/// Direct translation of `ChannelChatSender.send`; see the Kotlin quote in
/// the module docs. In practice `onlinePeerIds` is always `emptySet()` (the
/// sole caller `ChatSender.sendToChannel` does not pass it), so only self is
/// considered online. This means self is always the leader when it is a
/// joined member.
#[allow(clippy::too_many_arguments)]
pub async fn send<T: PeerTransport>(
    transport: &T,
    channel: &DChannel,
    client_id: &str,
    content: &str,
    db: &ChatDb,
    channel_key_cache: &ChannelKeyCache,
    kp_bytes: &[u8],
) -> SendResult {
    // onlinePeerIds defaults to emptySet() — only self is online.
    let online_ids: HashSet<String> = channel
        .joined_member_ids()
        .into_iter()
        .filter(|id| id == client_id)
        .collect();

    let leader_id = channel.elect_leader(&online_ids, client_id);

    match leader_id {
        None => SendResult::NoLeader,
        Some(lid) if lid == client_id => SendResult::Status(
            broadcast_as_leader(
                transport,
                channel,
                client_id,
                content,
                db,
                channel_key_cache,
                kp_bytes,
            )
            .await,
        ),
        Some(lid) => {
            send_to_leader(
                transport,
                channel,
                &lid,
                client_id,
                content,
                db,
                channel_key_cache,
                kp_bytes,
            )
            .await
        }
    }
}

// ── ChannelChatSender.broadcastAsLeader ─────────────────────────────────────

/// Broadcast to every joined member except self.
async fn broadcast_as_leader<T: PeerTransport>(
    transport: &T,
    channel: &DChannel,
    client_id: &str,
    content: &str,
    db: &ChatDb,
    channel_key_cache: &ChannelKeyCache,
    kp_bytes: &[u8],
) -> Vec<ChannelDeliveryResult> {
    // getRecipientIds(): joined members excluding self
    let recipient_ids: Vec<String> = channel
        .joined_member_ids()
        .into_iter()
        .filter(|id| id != client_id)
        .collect();

    send_to_recipients(
        transport,
        channel,
        &recipient_ids,
        content,
        db,
        channel_key_cache,
        client_id,
        kp_bytes,
    )
    .await
}

// ── ChannelChatSender.sendToRecipients ──────────────────────────────────────

/// Send to a specific list of recipient IDs.
#[allow(clippy::too_many_arguments)]
async fn send_to_recipients<T: PeerTransport>(
    transport: &T,
    channel: &DChannel,
    recipient_ids: &[String],
    content: &str,
    db: &ChatDb,
    channel_key_cache: &ChannelKeyCache,
    client_id: &str,
    kp_bytes: &[u8],
) -> Vec<ChannelDeliveryResult> {
    if recipient_ids.is_empty() {
        return Vec::new();
    }

    let mut results = Vec::with_capacity(recipient_ids.len());
    for member_id in recipient_ids {
        let member_peer = match db.get_peer_by_id(member_id) {
            Some(p) => p,
            None => {
                results.push(ChannelDeliveryResult {
                    peer_id: member_id.clone(),
                    peer_name: member_id.clone(),
                    error: Some("Peer not found in database".to_string()),
                });
                continue;
            }
        };
        results.push(
            send_to_member(
                transport,
                channel,
                &member_peer,
                content,
                channel_key_cache,
                client_id,
                kp_bytes,
            )
            .await,
        );
    }
    results
}

// ── ChannelChatSender.sendToLeader ──────────────────────────────────────────

/// Forward the message to the elected leader.
#[allow(clippy::too_many_arguments)]
async fn send_to_leader<T: PeerTransport>(
    transport: &T,
    channel: &DChannel,
    leader_id: &str,
    client_id: &str,
    content: &str,
    db: &ChatDb,
    channel_key_cache: &ChannelKeyCache,
    kp_bytes: &[u8],
) -> SendResult {
    let leader_peer = match db.get_peer_by_id(leader_id) {
        Some(p) => p,
        None => return SendResult::LeaderPeerMissing(()),
    };
    let result = send_to_member(
        transport,
        channel,
        &leader_peer,
        content,
        channel_key_cache,
        client_id,
        kp_bytes,
    )
    .await;
    SendResult::Status(vec![result])
}

// ── ChannelChatSender.sendToMember ──────────────────────────────────────────

/// Send to a single member peer.
///
/// `PeerGraphQLClient.createChannelChatItem` requires the channel key
/// (`requireNotNull(ChannelCacher.getKeyBytes(channelId))` — no fallback to
/// the peer key). The `c-cid` header is set to `channelId`, and the body is
/// encrypted with the channel key.
#[allow(clippy::too_many_arguments)]
async fn send_to_member<T: PeerTransport>(
    transport: &T,
    channel: &DChannel,
    peer: &DPeer,
    content: &str,
    channel_key_cache: &ChannelKeyCache,
    client_id: &str,
    kp_bytes: &[u8],
) -> ChannelDeliveryResult {
    // requireNotNull(ChannelCacher.getKeyBytes(channelId))
    let key = {
        let cache = channel_key_cache.read().unwrap();
        cache.get(&channel.id).cloned()
    };
    let Some(key) = key else {
        return ChannelDeliveryResult {
            peer_id: peer.id.clone(),
            peer_name: peer.name.clone(),
            error: Some(format!(
                "ChannelCacher has no key bytes for channel {}",
                channel.id
            )),
        };
    };

    let peer_urls = peer_graphql_urls(peer);
    let result = deliver_to_peer(
        transport,
        &peer_urls,
        &key,
        client_id,
        kp_bytes,
        content,
        Some(&channel.id),
    )
    .await;

    match result {
        Ok(()) => ChannelDeliveryResult {
            peer_id: peer.id.clone(),
            peer_name: peer.name.clone(),
            error: None,
        },
        Err(e) => ChannelDeliveryResult {
            peer_id: peer.id.clone(),
            peer_name: peer.name.clone(),
            error: Some(e),
        },
    }
}

// ── DMessageStatusData helpers ──────────────────────────────────────────────

/// Aggregate per-peer results into the coarse-grained `ChatStatus`.
/// Direct translation of `DMessageStatusData.aggregateStatus()`.
pub fn compute_status(results: &[ChannelDeliveryResult]) -> ChatStatus {
    if results.is_empty() {
        return ChatStatus::Sent;
    }
    let failed = results.iter().filter(|r| r.error.is_some()).count();
    if failed == 0 {
        ChatStatus::Sent
    } else if failed == results.len() {
        ChatStatus::Failed
    } else {
        ChatStatus::Partial
    }
}

/// Build the JSON-encoded `DMessageStatusData` payload.
/// Mirrors `DMessageStatusData(results)` serialization.
pub fn build_status_data_json(results: &[ChannelDeliveryResult]) -> String {
    if results.is_empty() {
        return String::new();
    }
    let arr: Vec<Value> = results.iter().map(|r| r.to_json()).collect();
    json!({ "results": arr }).to_string()
}

/// `DMessageStatusData(results=null)` — used for NoLeader / LeaderPeerMissing.
/// Mirrors `ChatDbHelper.updateChannelChatItemStatus(item, null)`.
pub fn build_no_leader_status_data() -> String {
    json!({ "results": Value::Null }).to_string()
}

#[cfg(test)]
#[path = "../../../tests/unit/chat/channel/chat_helper.rs"]
mod tests;
