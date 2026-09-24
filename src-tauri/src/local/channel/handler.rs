//! Receiver-side handlers for `channelSystemMessage` payloads.
//!
//! Mirrors plain-app `ChannelSystemMessageHandler` (see
//! `plain-app/.../channel/ChannelSystemMessageHandler.kt`). Each
//! handler mutates the local DB and broadcasts `WS_CHANNELS_UPDATED`
//! when the channel set changes.
//!
//! `handle_leave` and `handle_invite_accept` additionally call
//! `broadcast_update` to propagate the new member roster / version
//! to every other member, matching Kotlin's behaviour.

use tokio::sync::broadcast;

use serde_json::json;

use crate::local::db::{ChatDb, DChannel, DPeer, now_iso};
use crate::local::enums::{
    ChannelStatus, ChannelSystemMessageAction, ChannelSystemMessageType, MemberStatus, PeerStatus,
};
use crate::local::graphql::context::{
    ChannelKeyCache, PeerKeyCache, WS_CHANNEL_INVITE_RECEIVED, WS_CHANNELS_UPDATED, WsEvent,
    channels_updated_payload, load_key_cache,
};
use plain_rs::{base64_decode, ed25519_verify};

use super::messages::*;

/// Verify an Ed25519 signature on a channel system message payload.
///
/// Mirrors plain-app `DChatChannelExtensions.verifyEd25519Signature`:
/// an empty `public_key_b64` or `signature_b64` is accepted (permissive
/// for backward compatibility with older peers that did not sign). When
/// both are present, the signature is verified against `payload` using
/// the raw 32-byte Ed25519 public key.
fn verify_channel_signature(public_key_b64: &str, payload: &str, signature_b64: &str) -> bool {
    if public_key_b64.is_empty() || signature_b64.is_empty() {
        return true;
    }
    ed25519_verify(public_key_b64, payload.as_bytes(), signature_b64)
}

/// Dispatch a decoded `channelSystemMessage` from `from_id` to the correct
/// sub-handler based on `msg_type`. Returns `true` on a recognised message
/// (regardless of internal outcome), `false` for an unknown type.
///
/// `client_id` is the local device's own peer id (used to compare against
/// `owner == "me"` and to filter self out of the members list).
///
/// `kp_bytes` is the local Ed25519 keypair, used to sign outbound
/// `broadcast_update` traffic for `handle_leave` / `handle_invite_accept`.
///
/// `peer_key_cache` and `channel_key_cache` are used to encrypt the
/// outbound broadcast payload and to refresh the local channel key
/// cache after `handle_invite_accept` (mirrors Kotlin's
/// `ChatCacheManager.loadKeyCacheAsync`).
#[allow(clippy::too_many_arguments)]
pub fn handle(
    db: &ChatDb,
    client_id: &str,
    device_name: &str,
    from_id: &str,
    msg_type: ChannelSystemMessageType,
    payload: &str,
    event_tx: &broadcast::Sender<WsEvent>,
    kp_bytes: &[u8],
    peer_key_cache: &PeerKeyCache,
    channel_key_cache: &ChannelKeyCache,
) -> bool {
    let result = match msg_type {
        ChannelSystemMessageType::Invite => handle_invite(
            db,
            client_id,
            from_id,
            payload,
            event_tx,
            peer_key_cache,
            channel_key_cache,
        ),
        ChannelSystemMessageType::InviteAccept => handle_invite_accept(
            db,
            client_id,
            device_name,
            from_id,
            payload,
            event_tx,
            kp_bytes,
            peer_key_cache,
            channel_key_cache,
        ),
        ChannelSystemMessageType::InviteDecline => {
            handle_invite_decline(db, client_id, from_id, payload)
        }
        ChannelSystemMessageType::Update => handle_update(db, client_id, from_id, payload),
        ChannelSystemMessageType::Kick => handle_kick(db, client_id, from_id, payload),
        ChannelSystemMessageType::Leave => handle_leave(
            db,
            client_id,
            device_name,
            from_id,
            payload,
            event_tx,
            kp_bytes,
            peer_key_cache,
        ),
    };
    if result {
        let _ = event_tx.send(WsEvent {
            event_type: WS_CHANNELS_UPDATED,
            payload: channels_updated_payload(db),
        });
    }
    result
}

// ── ChannelInvite ───────────────────────────────────────────────────────────

fn handle_invite(
    db: &ChatDb,
    client_id: &str,
    from_id: &str,
    payload: &str,
    event_tx: &broadcast::Sender<WsEvent>,
    peer_key_cache: &PeerKeyCache,
    channel_key_cache: &ChannelKeyCache,
) -> bool {
    let msg: ChannelInvite = match serde_json::from_str(payload) {
        Ok(v) => v,
        Err(e) => {
            log::warn!("[channel] invite payload parse error: {e}");
            return false;
        }
    };
    let channel_id = &msg.channel_id;
    let channel_name = &msg.channel_name;

    if channel_id.is_empty() {
        log::warn!("[channel] invite missing channelId");
        return false;
    }

    // Reject invites from non-owners (the wire says `owner` but the actual
    // sender must equal it — the from_id check below acts as a sanity gate
    // since PeerGraphQL already authenticates the sender's signature).
    if msg.owner != from_id {
        log::warn!(
            "[channel] invite owner ({}) != fromId ({from_id}) — rejected",
            msg.owner
        );
        return false;
    }

    // Look up the owner's publicKey from the embedded `memberPeers` array
    // (mirrors plain-app `handleInvite`). Reject if the owner entry is
    // missing entirely — the signature cannot be authenticated without it.
    let owner_pub_key = match msg
        .member_peers
        .iter()
        .find(|m| m.id == msg.owner)
        .map(|m| &m.public_key)
    {
        Some(k) if !k.is_empty() => k.clone(),
        _ => {
            log::warn!("[channel] invite for {channel_id} has no owner memberPeerInfo — rejected");
            return false;
        }
    };

    let sig_payload = channel_message_payload(
        channel_id,
        msg.version,
        ChannelSystemMessageAction::Invite,
        client_id,
    );
    if !verify_channel_signature(&owner_pub_key, &sig_payload, &msg.signature) {
        log::warn!("[channel] invite signature failed for {channel_id} from {from_id} — rejected");
        return false;
    }

    // Reject invites from unknown / unpaired peers.
    if db.get_peer_by_id(from_id).is_none() {
        log::warn!("[channel] invite from unknown peer {from_id} — ignored");
        return false;
    }

    let existing = db.get_channel_by_id(channel_id);
    let is_reinvite = existing
        .as_ref()
        .map(|ch| ch.status == ChannelStatus::Left || ch.status == ChannelStatus::Kicked)
        .unwrap_or(false);

    if existing.is_some() && !is_reinvite {
        log::debug!("[channel] {channel_id} already exists locally, ignoring invite");
        return true;
    }

    // Auto-create peer records for members we don't already know about.
    for member in &msg.member_peers {
        if member.id.is_empty() || member.id == from_id || db.get_peer_by_id(&member.id).is_some() {
            continue;
        }
        let now = now_iso();
        let mut p = DPeer::new(
            &member.id,
            &member.name,
            &member.ip,
            member.port,
            member.device_type,
        );
        p.public_key = member.public_key.clone();
        p.status = PeerStatus::Channel;
        p.created_at = now.clone();
        p.updated_at = now;
        db.upsert_peer(&p);
    }

    let owner_name = db
        .get_peer_by_id(from_id)
        .map(|p| p.name)
        .unwrap_or_default();

    if let Some(mut ch) = existing {
        ch.name = channel_name.clone();
        ch.owner_id = from_id.to_string();
        ch.members = encode_members(&msg.members);
        if !msg.key.is_empty() {
            ch.key = msg.key.clone();
        }
        ch.version = msg.version;
        ch.status = ChannelStatus::Joined;
        ch.updated_at = now_iso();
        db.update_channel(&ch);
    } else {
        let now = now_iso();
        let ch = DChannel {
            id: channel_id.clone(),
            name: channel_name.clone(),
            owner_id: from_id.to_string(),
            members: encode_members(&msg.members),
            key: msg.key,
            version: msg.version,
            status: ChannelStatus::Joined,
            created_at: now.clone(),
            updated_at: now,
        };
        db.insert_channel(&ch);
    }
    load_key_cache(db, peer_key_cache, channel_key_cache);
    log::info!("[channel] invite accepted: {channel_name} ({channel_id}) from {from_id}");

    // Mirror plain-app's `ChannelInviteReceivedEvent`: notify the UI so it
    // can prompt the user to allow or deny the invite. The channel is
    // already persisted locally — a `deny` will remove it again via
    // `respond_channel_invite`; an `allow` simply keeps it.
    log::info!(
        "[channel] firing WS_CHANNEL_INVITE_RECEIVED for {channel_id} (subscriber count = {})",
        event_tx.receiver_count()
    );
    let send_result = event_tx.send(WsEvent {
        event_type: WS_CHANNEL_INVITE_RECEIVED,
        payload: json!({
            "channelId": channel_id,
            "channelName": channel_name,
            "fromId": from_id,
            "fromName": owner_name,
        })
        .to_string(),
    });
    log::info!(
        "[channel] WS_CHANNEL_INVITE_RECEIVED send result: {:?}",
        send_result.as_ref().map(|n| *n).map_err(|e| e.to_string())
    );
    true
}

// ── ChannelInviteAccept ─────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn handle_invite_accept(
    db: &ChatDb,
    client_id: &str,
    device_name: &str,
    from_id: &str,
    payload: &str,
    event_tx: &broadcast::Sender<WsEvent>,
    kp_bytes: &[u8],
    peer_key_cache: &PeerKeyCache,
    channel_key_cache: &ChannelKeyCache,
) -> bool {
    let msg: ChannelInviteAccept = match serde_json::from_str(payload) {
        Ok(v) => v,
        Err(e) => {
            log::warn!("[channel] invite_accept payload parse error: {e}");
            return false;
        }
    };
    let channel_id = &msg.channel_id;
    if channel_id.is_empty() {
        return false;
    }
    let Some(mut ch) = db.get_channel_by_id(channel_id) else {
        log::warn!("[channel] invite_accept for unknown channel {channel_id}");
        return false;
    };
    // Only the owner should process accept responses.
    if ch.owner_id != client_id {
        log::warn!("[channel] invite_accept received but we are not the owner of {channel_id}");
        return false;
    }

    // Make sure a peer record exists for the accepter.
    let pub_key = msg.public_key.clone();
    let name = msg.name.clone();
    match db.get_peer_by_id(from_id) {
        None => {
            let now = now_iso();
            let mut p = DPeer::new(from_id, &name, "", 0, msg.device_type);
            p.public_key = pub_key;
            p.status = PeerStatus::Channel;
            p.created_at = now.clone();
            p.updated_at = now;
            db.upsert_peer(&p);
        }
        Some(existing) => {
            let mut updated = false;
            let mut to_update = existing;
            if to_update.public_key.is_empty() && !pub_key.is_empty() {
                to_update.public_key = pub_key;
                updated = true;
            }
            if to_update.name.is_empty() && !name.is_empty() {
                to_update.name = name;
                updated = true;
            }
            if updated {
                to_update.updated_at = now_iso();
                db.upsert_peer(&to_update);
            }
        }
    }

    // pending → joined (or append as joined if member not found).
    let mut members = decode_members(&ch.members);
    if let Some(idx) = members.iter().position(|m| m.peer_id == from_id) {
        if members[idx].is_pending() {
            members[idx].status = MemberStatus::Joined;
        }
    } else {
        members.push(ChannelMember::new(from_id));
    }
    ch.members = encode_members(&members);
    ch.version += 1;
    ch.updated_at = now_iso();
    db.update_channel(&ch);

    // Mirror Kotlin: refresh key cache + broadcast the new roster so
    // existing members see the accepter as `joined` on next sync.
    load_key_cache(db, peer_key_cache, channel_key_cache);
    let channel_key = base64_decode(&ch.key);
    if !channel_key.is_empty() {
        let broadcast_ch = ch.clone();
        let broadcast_db = db.clone();
        let broadcast_peer_key_cache = peer_key_cache.clone();
        let broadcast_event_tx = event_tx.clone();
        let broadcast_kp_bytes = kp_bytes.to_vec();
        let client_id_owned = client_id.to_string();
        let device_name_owned = device_name.to_string();
        tokio::spawn(async move {
            crate::local::channel::sender::broadcast_update(
                &broadcast_ch,
                &client_id_owned,
                &device_name_owned,
                &broadcast_kp_bytes,
                &broadcast_db,
                &broadcast_peer_key_cache,
                &channel_key,
            )
            .await;
            let _ = broadcast_event_tx.send(WsEvent {
                event_type: WS_CHANNELS_UPDATED,
                payload: channels_updated_payload(&broadcast_db),
            });
        });
    }

    log::info!("[channel] peer {from_id} accepted invite for {channel_id}");
    true
}

// ── ChannelInviteDecline ────────────────────────────────────────────────────

fn handle_invite_decline(db: &ChatDb, client_id: &str, from_id: &str, payload: &str) -> bool {
    let msg: ChannelInviteDecline = match serde_json::from_str(payload) {
        Ok(v) => v,
        Err(e) => {
            log::warn!("[channel] invite_decline payload parse error: {e}");
            return false;
        }
    };
    let channel_id = &msg.channel_id;
    let Some(mut ch) = db.get_channel_by_id(channel_id) else {
        return false;
    };
    if ch.owner_id != client_id {
        return false;
    }
    let members = decode_members(&ch.members);
    if !has_member(&members, from_id) {
        return false;
    }
    let members: Vec<_> = members.into_iter().filter(|m| m.peer_id != from_id).collect();
    ch.members = encode_members(&members);
    ch.version += 1;
    ch.updated_at = now_iso();
    db.update_channel(&ch);
    log::info!("[channel] peer {from_id} declined invite for {channel_id}");
    true
}

// ── ChannelUpdate ───────────────────────────────────────────────────────────

fn handle_update(db: &ChatDb, _client_id: &str, from_id: &str, payload: &str) -> bool {
    let msg: ChannelUpdate = match serde_json::from_str(payload) {
        Ok(v) => v,
        Err(e) => {
            log::warn!("[channel] update payload parse error: {e}");
            return false;
        }
    };
    let channel_id = &msg.channel_id;
    let Some(mut ch) = db.get_channel_by_id(channel_id) else {
        log::warn!("[channel] update for unknown channel {channel_id}");
        return false;
    };
    // Only the owner may broadcast updates.
    if ch.owner_id != from_id {
        log::warn!(
            "[channel] update from non-owner {from_id} (owner={}) — rejected",
            ch.owner_id
        );
        return false;
    }
    let version = msg.version;

    // Look up the owner's publicKey from the local peers table (we
    // already know the owner since we have the channel). Mirrors
    // plain-app `handleUpdate`.
    let owner_pub_key = match db.get_peer_by_id(&ch.owner_id) {
        Some(p) => p.public_key,
        None => {
            log::warn!(
                "[channel] update: owner peer {} not found locally — rejected",
                ch.owner_id
            );
            return false;
        }
    };
    let sig_payload =
        channel_message_payload(channel_id, version, ChannelSystemMessageAction::Update, "");
    if !verify_channel_signature(&owner_pub_key, &sig_payload, &msg.signature) {
        log::warn!("[channel] update signature failed for {channel_id} from {from_id} — rejected");
        return false;
    }

    // Optimistic concurrency: stale updates are ignored.
    if version <= ch.version {
        log::debug!(
            "[channel] stale update (local={}, remote={version})",
            ch.version
        );
        return false;
    }

    // Auto-create peers for any new members we don't know.
    for member in &msg.member_peers {
        if member.id.is_empty() || member.id == from_id || db.get_peer_by_id(&member.id).is_some() {
            continue;
        }
        let now = now_iso();
        let mut p = DPeer::new(
            &member.id,
            &member.name,
            &member.ip,
            member.port,
            member.device_type,
        );
        p.public_key = member.public_key.clone();
        p.status = PeerStatus::Channel;
        p.created_at = now.clone();
        p.updated_at = now;
        db.upsert_peer(&p);
    }

    ch.name = msg.channel_name.clone();
    ch.members = encode_members(&msg.members);
    ch.version = version;
    ch.updated_at = now_iso();
    db.update_channel(&ch);
    log::info!("[channel] {channel_id} updated to version {version}");
    true
}

// ── ChannelKick ─────────────────────────────────────────────────────────────

fn handle_kick(db: &ChatDb, client_id: &str, from_id: &str, payload: &str) -> bool {
    let msg: ChannelKick = match serde_json::from_str(payload) {
        Ok(v) => v,
        Err(e) => {
            log::warn!("[channel] kick payload parse error: {e}");
            return false;
        }
    };
    let channel_id = &msg.channel_id;
    let Some(mut ch) = db.get_channel_by_id(channel_id) else {
        return false;
    };
    if ch.owner_id != from_id {
        log::warn!(
            "[channel] kick from non-owner {from_id} (owner={}) — rejected",
            ch.owner_id
        );
        return false;
    }
    let version = msg.version;

    // Look up the owner's publicKey from the local peers table. Mirrors
    // plain-app `handleKick`.
    let owner_pub_key = match db.get_peer_by_id(&ch.owner_id) {
        Some(p) => p.public_key,
        None => {
            log::warn!(
                "[channel] kick: owner peer {} not found locally — rejected",
                ch.owner_id
            );
            return false;
        }
    };
    let sig_payload = channel_message_payload(
        channel_id,
        version,
        ChannelSystemMessageAction::Kick,
        client_id,
    );
    if !verify_channel_signature(&owner_pub_key, &sig_payload, &msg.signature) {
        log::warn!("[channel] kick signature failed for {channel_id} from {from_id} — rejected");
        return false;
    }

    ch.status = ChannelStatus::Kicked;
    let members: Vec<_> = decode_members(&ch.members)
        .into_iter()
        .filter(|m| m.peer_id != client_id)
        .collect();
    ch.members = encode_members(&members);
    ch.updated_at = now_iso();
    db.update_channel(&ch);
    log::info!("[channel] kicked from {channel_id} by {from_id}");
    true
}

// ── ChannelLeave ────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn handle_leave(
    db: &ChatDb,
    client_id: &str,
    device_name: &str,
    from_id: &str,
    payload: &str,
    event_tx: &broadcast::Sender<WsEvent>,
    kp_bytes: &[u8],
    peer_key_cache: &PeerKeyCache,
) -> bool {
    let msg: ChannelLeave = match serde_json::from_str(payload) {
        Ok(v) => v,
        Err(e) => {
            log::warn!("[channel] leave payload parse error: {e}");
            return false;
        }
    };
    let channel_id = &msg.channel_id;
    let Some(mut ch) = db.get_channel_by_id(channel_id) else {
        return false;
    };
    if ch.owner_id != client_id {
        log::warn!("[channel] leave received but we are not the owner of {channel_id}");
        return false;
    }
    let members: Vec<_> = decode_members(&ch.members)
        .into_iter()
        .filter(|m| m.peer_id != from_id)
        .collect();
    ch.members = encode_members(&members);
    ch.version += 1;
    ch.updated_at = now_iso();
    db.update_channel(&ch);

    // Mirror Kotlin: tell every other member that the leaver is gone
    // and the roster version has moved.
    let channel_key = base64_decode(&ch.key);
    if !channel_key.is_empty() {
        let broadcast_ch = ch.clone();
        let broadcast_db = db.clone();
        let broadcast_peer_key_cache = peer_key_cache.clone();
        let broadcast_event_tx = event_tx.clone();
        let broadcast_kp_bytes = kp_bytes.to_vec();
        let client_id_owned = client_id.to_string();
        let device_name_owned = device_name.to_string();
        tokio::spawn(async move {
            crate::local::channel::sender::broadcast_update(
                &broadcast_ch,
                &client_id_owned,
                &device_name_owned,
                &broadcast_kp_bytes,
                &broadcast_db,
                &broadcast_peer_key_cache,
                &channel_key,
            )
            .await;
            let _ = broadcast_event_tx.send(WsEvent {
                event_type: WS_CHANNELS_UPDATED,
                payload: channels_updated_payload(&broadcast_db),
            });
        });
    }
    log::info!("[channel] peer {from_id} left {channel_id}");
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use plain_rs::{base64_encode, ed25519_generate, ed25519_sign};

    /// Empty `public_key` or `signature` is accepted (permissive for
    /// backward compatibility with older peers that did not sign).
    /// Mirrors plain-app `DChatChannelExtensions.verifyEd25519Signature`.
    #[test]
    fn verify_channel_signature_permissive_on_empty() {
        let payload = "ch_x|1|invite|peer_y";
        // Both empty → accept.
        assert!(verify_channel_signature("", payload, ""));
        // Empty public key, non-empty signature → accept.
        assert!(verify_channel_signature("", payload, "AAAA"));
        // Non-empty public key, empty signature → accept.
        assert!(verify_channel_signature("AAAA", payload, ""));
    }

    /// A real signature round-trip should verify, and tampering should fail.
    #[test]
    fn verify_channel_signature_roundtrip_and_tamper() {
        let (kp_bytes, vk_bytes) = ed25519_generate();
        let payload =
            channel_message_payload("ch_5", 4, ChannelSystemMessageAction::Kick, "peer_d");
        let sig = ed25519_sign(&kp_bytes, payload.as_bytes());
        let pub_key_b64 = base64_encode(&vk_bytes);

        assert!(
            verify_channel_signature(&pub_key_b64, &payload, &sig),
            "valid signature should verify"
        );

        let tampered =
            channel_message_payload("ch_5", 99, ChannelSystemMessageAction::Kick, "peer_d");
        assert!(
            !verify_channel_signature(&pub_key_b64, &tampered, &sig),
            "tampered payload should fail verification"
        );
    }
}
