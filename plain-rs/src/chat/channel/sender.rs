//! Sender-side helpers for `channelSystemMessage` payloads.
//!
//! Mirrors plain-app `ChannelSystemMessageSender`. Each function builds the
//! correct JSON payload and posts it to the peer's `/peer_graphql` endpoint
//! via the same transport used by `createChatItem`.
//!
//! `kp_bytes` is the local Ed25519 keypair (decoded from
//! `ChatIdentity.ed25519_keypair`) used to sign every outbound message so
//! the receiver can authenticate the sender.
//!
//! ## Encryption layer
//!
//! For channel system messages the wire is encrypted with the per-channel
//! ChaCha20 key (`channel.key` from `DChannel`). The `c-cid` HTTP header is
//! set to the channel id so the receiver can pick `channel_key_cache[c-cid]`
//! to decrypt, rather than the peer's shared key. This mirrors Kotlin's
//! `PeerGraphQLClient.sendChannelSystemMessage`, which switches key based
//! on `channelId.isNotEmpty()`.

use std::str::FromStr;

use crate::base64_encode;

use crate::chat::enums::{ChannelSystemMessageType, DeviceType};
use crate::chat::events::PeerKeyCache;
use crate::chat::transport::{PeerTransport, deliver_channel_system_message};
use crate::db::{DChannel, DPeer, Db};

use super::messages::*;

// ── Public API ─────────────────────────────────────────────────────────────

/// Send a `channel_invite` to a single peer. The per-channel ChaCha20 key
/// is embedded in the payload so the invitee can decrypt subsequent channel
/// traffic, and the wire itself is encrypted with that same key.
#[allow(clippy::too_many_arguments)]
pub async fn send_invite<T: PeerTransport>(
    transport: &T,
    channel: &DChannel,
    peer: &DPeer,
    client_id: &str,
    device_name: &str,
    self_device_type: DeviceType,
    kp_bytes: &[u8],
    db: &Db,
    key_cache: &PeerKeyCache,
    channel_key: &[u8],
) -> bool {
    let Ok(payload) = super::outgoing::build(
        Some(db),
        channel,
        client_id,
        device_name,
        self_device_type,
        kp_bytes,
        ChannelSystemMessageType::Invite,
        &peer.id,
    ) else {
        return false;
    };
    deliver_type(
        transport,
        peer,
        client_id,
        kp_bytes,
        ChannelSystemMessageType::Invite,
        &payload,
        Some(&channel.id),
        channel_key,
        key_cache,
    )
    .await
}

/// Send `channel_invite_accept` to the channel owner. Wire is encrypted
/// with the per-channel key (the owner knows the channel key by virtue of
/// having created the channel).
///
/// The local Ed25519 public key is extracted from `kp_bytes[32..]`
/// (mirrors plain-app `SignatureHelper.getRawPublicKeyBase64Async()`) so
/// the owner can store it for later signature verification on
/// `channel_update` / `channel_kick` traffic.
#[allow(clippy::too_many_arguments)]
pub async fn send_invite_accept<T: PeerTransport>(
    transport: &T,
    channel_id: &str,
    owner_peer: &DPeer,
    client_id: &str,
    kp_bytes: &[u8],
    name: &str,
    device_type: DeviceType,
    channel_key: &[u8],
    key_cache: &PeerKeyCache,
) -> bool {
    let mut channel = DChannel::new("", "");
    channel.id = channel_id.into();
    let Ok(payload) = super::outgoing::build(
        None,
        &channel,
        client_id,
        name,
        device_type,
        kp_bytes,
        ChannelSystemMessageType::InviteAccept,
        "",
    ) else {
        return false;
    };
    deliver_type(
        transport,
        owner_peer,
        client_id,
        kp_bytes,
        ChannelSystemMessageType::InviteAccept,
        &payload,
        Some(channel_id),
        channel_key,
        key_cache,
    )
    .await
}

/// Send `channel_invite_decline` to the channel owner.
#[allow(clippy::too_many_arguments)]
pub async fn send_invite_decline<T: PeerTransport>(
    transport: &T,
    channel_id: &str,
    owner_peer: &DPeer,
    client_id: &str,
    kp_bytes: &[u8],
    channel_key: &[u8],
    key_cache: &PeerKeyCache,
) -> bool {
    let mut channel = DChannel::new("", "");
    channel.id = channel_id.into();
    let Ok(payload) = super::outgoing::build(
        None,
        &channel,
        client_id,
        "",
        DeviceType::Phone,
        kp_bytes,
        ChannelSystemMessageType::InviteDecline,
        "",
    ) else {
        return false;
    };
    deliver_type(
        transport,
        owner_peer,
        client_id,
        kp_bytes,
        ChannelSystemMessageType::InviteDecline,
        &payload,
        Some(channel_id),
        channel_key,
        key_cache,
    )
    .await
}

/// Broadcast a `channel_update` to every member of the channel
/// (excluding self). Wire is encrypted with the channel key.
#[allow(clippy::too_many_arguments)]
pub async fn broadcast_update<T: PeerTransport>(
    transport: &T,
    channel: &DChannel,
    client_id: &str,
    device_name: &str,
    self_device_type: DeviceType,
    kp_bytes: &[u8],
    db: &Db,
    key_cache: &PeerKeyCache,
    channel_key: &[u8],
) {
    let Ok(payload) = super::outgoing::build(
        Some(db),
        channel,
        client_id,
        device_name,
        self_device_type,
        kp_bytes,
        ChannelSystemMessageType::Update,
        "",
    ) else {
        return;
    };
    let member_ids = member_ids_excluding(channel, client_id);
    for member_id in member_ids {
        if let Some(peer) = db.get_peer_by_id(&member_id) {
            let _ = deliver_type(
                transport,
                &peer,
                client_id,
                kp_bytes,
                ChannelSystemMessageType::Update,
                &payload,
                Some(&channel.id),
                channel_key,
                key_cache,
            )
            .await;
        }
    }
}

/// Send `channel_kick` to a single peer. Wire is encrypted with the
/// channel key.
#[allow(clippy::too_many_arguments)]
pub async fn send_kick<T: PeerTransport>(
    transport: &T,
    channel_id: &str,
    version: i64,
    peer: &DPeer,
    client_id: &str,
    kp_bytes: &[u8],
    channel_key: &[u8],
    key_cache: &PeerKeyCache,
) -> bool {
    let mut channel = DChannel::new("", client_id);
    channel.id = channel_id.into();
    channel.version = version;
    let Ok(payload) = super::outgoing::build(
        None,
        &channel,
        client_id,
        "",
        DeviceType::Phone,
        kp_bytes,
        ChannelSystemMessageType::Kick,
        &peer.id,
    ) else {
        return false;
    };
    deliver_type(
        transport,
        peer,
        client_id,
        kp_bytes,
        ChannelSystemMessageType::Kick,
        &payload,
        Some(channel_id),
        channel_key,
        key_cache,
    )
    .await
}

/// Broadcast a `channel_kick` to every member of the channel (excluding
/// self). Used when the owner deletes the channel entirely. Wire is
/// encrypted with the channel key.
#[allow(clippy::too_many_arguments)]
pub async fn broadcast_kick<T: PeerTransport>(
    transport: &T,
    channel: &DChannel,
    client_id: &str,
    kp_bytes: &[u8],
    db: &Db,
    key_cache: &PeerKeyCache,
    channel_key: &[u8],
) {
    let Ok(payload) = super::outgoing::build(
        Some(db),
        channel,
        client_id,
        "",
        DeviceType::Phone,
        kp_bytes,
        ChannelSystemMessageType::Kick,
        "",
    ) else {
        return;
    };
    let member_ids = member_ids_excluding(channel, client_id);
    for member_id in member_ids {
        if let Some(peer) = db.get_peer_by_id(&member_id) {
            let _ = deliver_type(
                transport,
                &peer,
                client_id,
                kp_bytes,
                ChannelSystemMessageType::Kick,
                &payload,
                Some(&channel.id),
                channel_key,
                key_cache,
            )
            .await;
        }
    }
}

/// Send `channel_leave` to the channel owner. Wire is encrypted with the
/// channel key.
#[allow(clippy::too_many_arguments)]
pub async fn send_leave<T: PeerTransport>(
    transport: &T,
    channel_id: &str,
    owner_peer: &DPeer,
    client_id: &str,
    kp_bytes: &[u8],
    channel_key: &[u8],
    key_cache: &PeerKeyCache,
) -> bool {
    let mut channel = DChannel::new("", "");
    channel.id = channel_id.into();
    let Ok(payload) = super::outgoing::build(
        None,
        &channel,
        client_id,
        "",
        DeviceType::Phone,
        kp_bytes,
        ChannelSystemMessageType::Leave,
        "",
    ) else {
        return false;
    };
    deliver_type(
        transport,
        owner_peer,
        client_id,
        kp_bytes,
        ChannelSystemMessageType::Leave,
        &payload,
        Some(channel_id),
        channel_key,
        key_cache,
    )
    .await
}

// ── Internals ──────────────────────────────────────────────────────────────

fn member_ids_excluding(channel: &DChannel, exclude_id: &str) -> Vec<String> {
    decode_members(&channel.members)
        .into_iter()
        .filter_map(|m| {
            if m.peer_id == exclude_id {
                None
            } else {
                Some(m.peer_id)
            }
        })
        .collect()
}

/// Send a `channelSystemMessage` GraphQL mutation to the peer.
///
/// Mirrors plain-app `PeerGraphQLClient.sendChannelSystemMessage`: when the
/// peer has its own shared key (a paired peer), the request is sent over
/// that shared key with no `c-cid` header; otherwise the per-channel key
/// (`channel_key`) is used and the `c-cid` header is set so the receiver
/// can pick the matching key from its `channel_key_cache`.
#[allow(clippy::too_many_arguments)]
async fn deliver_type<T: PeerTransport>(
    transport: &T,
    peer: &DPeer,
    client_id: &str,
    kp_bytes: &[u8],
    msg_type: ChannelSystemMessageType,
    payload: &str,
    channel_id_opt: Option<&str>,
    channel_key: &[u8],
    key_cache: &PeerKeyCache,
) -> bool {
    // Pick the transport key, mirroring plain-app's `if (peer.key.isNotEmpty())`
    // branch in `sendChannelSystemMessage`. A paired peer is always encrypted
    // with its own shared key; the channel key is only used for channel members
    // that were never directly paired (e.g. they joined via another invite).
    let (key, wire_cid): (Vec<u8>, Option<&str>) = if !peer.key.is_empty() {
        let cache = key_cache.read().unwrap();
        match cache.get(&peer.id).cloned() {
            Some(k) => (k, None),
            None => {
                log::debug!(
                    "[channel] no shared key for peer {}, skipping {msg_type:?}",
                    peer.id
                );
                return false;
            }
        }
    } else {
        (channel_key.to_vec(), channel_id_opt)
    };
    let msg_type_str = msg_type.as_str();

    deliver_channel_system_message(
        transport,
        peer,
        &key,
        client_id,
        kp_bytes,
        msg_type_str,
        payload,
        wire_cid,
    )
    .await
}

/// Kept for API parity with plain-desktop's sender module.
pub fn encode_channel_key(raw: &[u8]) -> String {
    base64_encode(raw)
}

/// Parse a wire device type string, defaulting to PHONE (mirrors the
/// Kotlin `runCatching { DeviceType.valueOf(it) }.getOrDefault(PHONE)`).
pub fn parse_device_type(s: &str) -> DeviceType {
    DeviceType::from_str(s).unwrap_or(DeviceType::Phone)
}

#[cfg(test)]
#[path = "../../../tests/unit/chat/channel/sender.rs"]
mod tests;
