//! Sender-side helpers for `channelSystemMessage` payloads.
//!
//! Mirrors plain-app `ChannelSystemMessageSender` (see
//! `plain-app/.../channel/ChannelSystemMessageSender.kt`). Each
//! function builds the correct JSON payload and posts it to the
//! peer's `/peer_graphql` endpoint via the same transport used by
//! `createChatItem`.
//!
//! `kp_bytes` is the local Ed25519 keypair (decoded from
//! `AppIdentity.ed25519_keypair`) used to sign every outbound
//! message so the receiver can authenticate the sender.
//!
//! ## Encryption layer
//!
//! For channel system messages the wire is encrypted with the
//! per-channel ChaCha20 key (`channel.key` from `DChannel`). The
//! `c-cid` HTTP header is set to the channel id so the receiver can
//! pick `channel_key_cache[c-cid]` to decrypt, rather than the
//! peer's shared key. This mirrors Kotlin's
//! `PeerGraphQLClient.sendChannelSystemMessage`, which switches key
//! based on `channelId.isNotEmpty()`.
//!
//! Callers pass the per-channel key (already base64-decoded) as
//! `channel_key`; for the rare cases where it is absent (e.g. the
//! very first `channel_invite` before the receiver has stored the
//! key), the function falls back to the peer's shared key so the
//! delivery can still complete — receivers then re-encrypt their
//! responses on the same channel key once they have it.

use std::str::FromStr;

use crate::local::db::{ChatDb, DChannel, DPeer};
use crate::local::enums::{ChannelSystemMessageAction, ChannelSystemMessageType, DeviceType};
use crate::local::graphql::context::PeerKeyCache;
use plain_rs::{base64_encode, ed25519_sign};

use super::messages::*;

// ── Public API ─────────────────────────────────────────────────────────────

/// Send a `channel_invite` to a single peer. The per-channel
/// ChaCha20 key is embedded in the payload so the invitee can
/// decrypt subsequent channel traffic, and the wire itself is
/// encrypted with that same key.
#[allow(dead_code, clippy::too_many_arguments)]
pub async fn send_invite(
    channel: &DChannel,
    peer: &DPeer,
    client_id: &str,
    device_name: &str,
    kp_bytes: &[u8],
    db: &ChatDb,
    key_cache: &PeerKeyCache,
    channel_key: &[u8],
) -> bool {
    let member_peers = build_member_peers(channel, db, client_id, device_name, kp_bytes);
    let sig_payload = channel_message_payload(
        &channel.id,
        channel.version,
        ChannelSystemMessageAction::Invite,
        &peer.id,
    );
    let signature = ed25519_sign(kp_bytes, sig_payload.as_bytes());
    let invite = ChannelInvite {
        channel_id: channel.id.clone(),
        channel_name: channel.name.clone(),
        key: channel.key.clone(),
        owner: client_id.to_string(),
        members: decode_members(&channel.members),
        member_peers,
        version: channel.version,
        signature,
    };
    let payload = serde_json::to_string(&invite).unwrap_or_default();
    deliver_type(
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

/// Send `channel_invite_accept` to the channel owner. Wire is
/// encrypted with the per-channel key (the owner knows the channel
/// key by virtue of having created the channel).
///
/// The local Ed25519 public key is extracted from `kp_bytes[32..]`
/// (mirrors plain-app `SignatureHelper.getRawPublicKeyBase64Async()`)
/// so the owner can store it for later signature verification on
/// `channel_update` / `channel_kick` traffic.
#[allow(dead_code, clippy::too_many_arguments)]
pub async fn send_invite_accept(
    channel_id: &str,
    owner_peer: &DPeer,
    client_id: &str,
    kp_bytes: &[u8],
    name: &str,
    device_type: &str,
    channel_key: &[u8],
    key_cache: &PeerKeyCache,
) -> bool {
    let public_key = if kp_bytes.len() == 64 {
        base64_encode(&kp_bytes[32..])
    } else {
        String::new()
    };
    let accept = ChannelInviteAccept {
        channel_id: channel_id.to_string(),
        public_key,
        name: name.to_string(),
        device_type: DeviceType::from_str(device_type).unwrap_or(DeviceType::Phone),
    };
    let payload = serde_json::to_string(&accept).unwrap_or_default();
    deliver_type(
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
#[allow(dead_code)]
pub async fn send_invite_decline(
    channel_id: &str,
    owner_peer: &DPeer,
    client_id: &str,
    kp_bytes: &[u8],
    channel_key: &[u8],
    key_cache: &PeerKeyCache,
) -> bool {
    let decline = ChannelInviteDecline {
        channel_id: channel_id.to_string(),
    };
    let payload = serde_json::to_string(&decline).unwrap_or_default();
    deliver_type(
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
#[allow(dead_code)]
pub async fn broadcast_update(
    channel: &DChannel,
    client_id: &str,
    device_name: &str,
    kp_bytes: &[u8],
    db: &ChatDb,
    key_cache: &PeerKeyCache,
    channel_key: &[u8],
) {
    let member_peers = build_member_peers(channel, db, client_id, device_name, kp_bytes);
    let sig_payload = channel_message_payload(
        &channel.id,
        channel.version,
        ChannelSystemMessageAction::Update,
        "",
    );
    let signature = ed25519_sign(kp_bytes, sig_payload.as_bytes());
    let update = ChannelUpdate {
        channel_id: channel.id.clone(),
        channel_name: channel.name.clone(),
        members: decode_members(&channel.members),
        member_peers,
        version: channel.version,
        signature,
    };
    let payload = serde_json::to_string(&update).unwrap_or_default();
    let member_ids = member_ids_excluding(channel, client_id);
    for member_id in member_ids {
        if let Some(peer) = db.get_peer_by_id(&member_id) {
            let _ = deliver_type(
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
#[allow(dead_code)]
pub async fn send_kick(
    channel_id: &str,
    version: i64,
    peer: &DPeer,
    client_id: &str,
    kp_bytes: &[u8],
    channel_key: &[u8],
    key_cache: &PeerKeyCache,
) -> bool {
    let sig_payload = channel_message_payload(
        channel_id,
        version,
        ChannelSystemMessageAction::Kick,
        &peer.id,
    );
    let signature = ed25519_sign(kp_bytes, sig_payload.as_bytes());
    let kick = ChannelKick {
        channel_id: channel_id.to_string(),
        version,
        signature,
    };
    let payload = serde_json::to_string(&kick).unwrap_or_default();
    deliver_type(
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

/// Broadcast a `channel_kick` to every member of the channel
/// (excluding self). Used when the owner deletes the channel
/// entirely. Wire is encrypted with the channel key.
#[allow(dead_code)]
pub async fn broadcast_kick(
    channel: &DChannel,
    client_id: &str,
    kp_bytes: &[u8],
    db: &ChatDb,
    key_cache: &PeerKeyCache,
    channel_key: &[u8],
) {
    let sig_payload = channel_message_payload(
        &channel.id,
        channel.version,
        ChannelSystemMessageAction::Kick,
        "",
    );
    let signature = ed25519_sign(kp_bytes, sig_payload.as_bytes());
    let kick = ChannelKick {
        channel_id: channel.id.clone(),
        version: channel.version,
        signature,
    };
    let payload = serde_json::to_string(&kick).unwrap_or_default();
    let member_ids = member_ids_excluding(channel, client_id);
    for member_id in member_ids {
        if let Some(peer) = db.get_peer_by_id(&member_id) {
            let _ = deliver_type(
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

/// Send `channel_leave` to the channel owner. Wire is encrypted with
/// the channel key.
#[allow(dead_code)]
pub async fn send_leave(
    channel_id: &str,
    owner_peer: &DPeer,
    client_id: &str,
    kp_bytes: &[u8],
    channel_key: &[u8],
    key_cache: &PeerKeyCache,
) -> bool {
    let leave = ChannelLeave {
        channel_id: channel_id.to_string(),
    };
    let payload = serde_json::to_string(&leave).unwrap_or_default();
    deliver_type(
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

/// Build the `MemberPeerInfo` array for the channel members.
///
/// Mirrors plain-app `DChatChannel.getPeersAsync()` — the owner is always
/// included first (synthesized from local device info, since the owner is
/// not in the `peers` table), then every other member. This ensures the
/// invitee can always find the owner's `publicKey` for signature
/// verification, even if the owner is not in the `members` list.
#[allow(dead_code)]
fn build_member_peers(
    channel: &DChannel,
    db: &ChatDb,
    client_id: &str,
    device_name: &str,
    kp_bytes: &[u8],
) -> Vec<MemberPeerInfo> {
    let self_pub_key = if kp_bytes.len() == 64 {
        base64_encode(&kp_bytes[32..])
    } else {
        String::new()
    };
    let mut peers = vec![MemberPeerInfo {
        id: client_id.to_string(),
        name: device_name.to_string(),
        public_key: self_pub_key,
        device_type: DeviceType::Computer,
        ip: String::new(),
        port: 0,
    }];
    for m in decode_members(&channel.members) {
        if m.peer_id == client_id {
            continue; // already added above
        }
        if let Some(p) = db.get_peer_by_id(&m.peer_id) {
            peers.push(MemberPeerInfo {
                id: p.id,
                name: p.name,
                public_key: p.public_key,
                device_type: p.device_type,
                ip: p.ip,
                port: p.port,
            });
        }
    }
    peers
}

#[allow(dead_code)]
fn member_ids_excluding(channel: &DChannel, exclude_id: &str) -> Vec<String> {
    decode_members(&channel.members)
        .into_iter()
        .filter_map(|m| if m.peer_id == exclude_id { None } else { Some(m.peer_id) })
        .collect()
}

/// Send a `channelSystemMessage` GraphQL mutation to the peer.
///
/// Mirrors plain-app `PeerGraphQLClient.sendChannelSystemMessage`:
/// when the peer has its own shared key (a paired peer), the request
/// is sent over that shared key with no `c-cid` header; otherwise the
/// per-channel key (`channel_key`) is used and the `c-cid` header is
/// set so the receiver can pick the matching key from its
/// `channel_key_cache`.
#[allow(clippy::too_many_arguments)]
async fn deliver_type(
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

    // Borrow the existing delivery helper from graphql/peer so we don't
    // duplicate the encrypt/sign/POST/verify pipeline.
    super::super::graphql::peer::deliver_channel_system_message(
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

#[allow(dead_code)]
pub fn encode_channel_key(raw: &[u8]) -> String {
    base64_encode(raw)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local::enums::{DeviceType, MemberStatus};
    use plain_rs::{base64_encode, ed25519_generate};
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_tmp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let pid = std::process::id();
        std::env::temp_dir().join(format!("plainapp-sender-{label}-{pid}-{nanos}"))
    }

    /// Regression test for the invite flow: `build_member_peers` must include
    /// the owner's own `MemberPeerInfo` whenever the owner is a member of the
    /// channel. Without it, the invitee's `handleInvite` rejects the invite
    /// with "no owner memberPeerInfo". Mirrors plain-app `getPeersAsync`.
    #[test]
    fn build_member_peers_includes_owner_when_owner_is_member() {
        let db =
            ChatDb::open(&unique_tmp_dir("owner-member").join("local_chat.db")).expect("open db");
        let (kp_bytes, _vk_bytes) = ed25519_generate();
        let client_id = "owner-1";
        let device_name = "Desktop";

        let mut channel = DChannel::new("Channel", client_id);
        channel.members = serde_json::json!([
            { "id": client_id, "status": MemberStatus::Joined.to_string() }
        ])
        .to_string();

        let member_peers = build_member_peers(&channel, &db, client_id, device_name, &kp_bytes);

        let owner_entry = member_peers.iter().find(|m| m.id == client_id);
        assert!(owner_entry.is_some(), "owner must appear in memberPeers");
        let owner_entry = owner_entry.unwrap();
        assert_eq!(owner_entry.public_key, base64_encode(&kp_bytes[32..]));
        assert_eq!(owner_entry.device_type, DeviceType::Computer);
    }

    /// The owner must appear exactly once, even alongside other members.
    #[test]
    fn build_member_peers_includes_owner_alongside_members() {
        let db = ChatDb::open(&unique_tmp_dir("owner-plus-member").join("local_chat.db"))
            .expect("open db");
        let (kp_bytes, _vk_bytes) = ed25519_generate();
        let client_id = "owner-1";
        let member_id = "member-1";
        db.upsert_peer(&DPeer::new(
            member_id,
            "Pixel",
            "192.168.1.5",
            8443,
            DeviceType::Phone,
        ));

        let mut channel = DChannel::new("Channel", client_id);
        channel.members = serde_json::json!([
            { "id": client_id, "status": MemberStatus::Joined.to_string() },
            { "id": member_id, "status": MemberStatus::Pending.to_string() }
        ])
        .to_string();

        let member_peers = build_member_peers(&channel, &db, client_id, "Desktop", &kp_bytes);
        let ids: Vec<&str> = member_peers.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids.len(), 2, "owner + member should both be present");
        assert_eq!(ids[0], client_id);
        assert_eq!(ids[1], member_id);
    }

    /// Regression: owner must be in `memberPeers` even when the owner is NOT
    /// in the `members` list (e.g. a channel created before the owner-as-member
    /// fix). Without this the invitee rejects the invite with
    /// "no owner memberPeerInfo".
    #[test]
    fn build_member_peers_includes_owner_even_when_not_in_members() {
        let db = ChatDb::open(&unique_tmp_dir("owner-not-in-members").join("local_chat.db"))
            .expect("open db");
        let (kp_bytes, _vk_bytes) = ed25519_generate();
        let client_id = "owner-1";
        let member_id = "member-1";
        db.upsert_peer(&DPeer::new(
            member_id,
            "Pixel",
            "192.168.1.5",
            8443,
            DeviceType::Phone,
        ));

        // Owner is deliberately NOT in members — only the invitee is.
        let mut channel = DChannel::new("Channel", client_id);
        channel.members = serde_json::json!([
            { "id": member_id, "status": MemberStatus::Pending.to_string() }
        ])
        .to_string();

        let member_peers = build_member_peers(&channel, &db, client_id, "Desktop", &kp_bytes);
        let ids: Vec<&str> = member_peers.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids.len(), 2, "owner + member should both be present");
        assert_eq!(ids[0], client_id, "owner must be first");
        assert_eq!(ids[1], member_id);
    }
}
