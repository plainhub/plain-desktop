//! Wire types for `channelSystemMessage` GraphQL payloads.
//!
//! Mirrors plain-app `ChannelSystemMessages` (see
//! `plain-app/.../channel/ChannelSystemMessage.kt`) with full
//! strong-typed data structures instead of weakly-typed JSON maps.
//! Every struct is `Serialize`/`Deserialize` with `camelCase` wire
//! names so the JSON we send/receive matches the Kotlin `@Serializable`
//! data classes exactly.

use serde::{Deserialize, Serialize};

use crate::local::enums::{ChannelSystemMessageAction, DeviceType, MemberStatus};

/// Build the canonical signature payload for a channel system message.
/// Mirrors plain-app `channelMessagePayload(channelId, version, action, target)`:
/// `"$channelId|$version|$action|$target"`.
///
/// `target` is the peer id being invited/kicked, or an empty string for
/// `update` and broadcast `kick`.
pub fn channel_message_payload(
    channel_id: &str,
    version: i64,
    action: ChannelSystemMessageAction,
    target: &str,
) -> String {
    format!("{channel_id}|{version}|{action}|{target}")
}

// ── ChannelMember ─────────────────────────────────────────────────────────

/// A channel member: peer id + membership status. Mirrors plain-app
/// `ChannelMember`. Carries only the id and status; all other peer
/// metadata (name, publicKey, IP, port) lives in the `peers` table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelMember {
    /// Wire/storage key is `peerId` since the 2026-09-24 naming cleanup; the
    /// legacy `id` key stays decodable via the serde alias (older app versions).
    #[serde(rename = "peerId", alias = "id")]
    pub peer_id: String,
    #[serde(default)]
    pub status: MemberStatus,
}

impl ChannelMember {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            peer_id: id.into(),
            status: MemberStatus::Joined,
        }
    }

    pub fn pending(id: impl Into<String>) -> Self {
        Self {
            peer_id: id.into(),
            status: MemberStatus::Pending,
        }
    }

    pub fn is_joined(&self) -> bool {
        self.status == MemberStatus::Joined
    }

    pub fn is_pending(&self) -> bool {
        self.status == MemberStatus::Pending
    }
}

// ── MemberPeerInfo ────────────────────────────────────────────────────────

/// Lightweight peer info for a channel member, embedded in invites and
/// updates so the other side can create peer records for members it
/// doesn't already know. For invites the owner's `publicKey` is taken
/// from the entry whose `id` matches the invite `owner`. Mirrors
/// plain-app `MemberPeerInfo`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemberPeerInfo {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub public_key: String,
    #[serde(default)]
    pub device_type: DeviceType,
    #[serde(default)]
    pub ip: String,
    #[serde(default)]
    pub port: u16,
}

// ── ChannelInvite ─────────────────────────────────────────────────────────

/// Owner → invitee. Mirrors plain-app `ChannelInvite`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelInvite {
    pub channel_id: String,
    pub channel_name: String,
    /// Base64-encoded symmetric ChaCha20 key for the channel.
    pub key: String,
    pub owner: String,
    pub members: Vec<ChannelMember>,
    #[serde(default)]
    pub member_peers: Vec<MemberPeerInfo>,
    pub version: i64,
    /// Ed25519 signature of `"$channelId|$version|invite|<invitee peer id>"`
    /// (Base64), signed by the owner at send time.
    #[serde(default)]
    pub signature: String,
}

// ── ChannelInviteAccept ───────────────────────────────────────────────────

/// Invitee → Owner. Mirrors plain-app `ChannelInviteAccept`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelInviteAccept {
    pub channel_id: String,
    #[serde(default)]
    pub public_key: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub device_type: DeviceType,
}

// ── ChannelInviteDecline ──────────────────────────────────────────────────

/// Invitee → Owner. Mirrors plain-app `ChannelInviteDecline`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelInviteDecline {
    pub channel_id: String,
}

// ── ChannelUpdate ─────────────────────────────────────────────────────────

/// Owner → all members. Mirrors plain-app `ChannelUpdate`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelUpdate {
    pub channel_id: String,
    pub channel_name: String,
    pub members: Vec<ChannelMember>,
    #[serde(default)]
    pub member_peers: Vec<MemberPeerInfo>,
    pub version: i64,
    /// Ed25519 signature of `"$channelId|$version|update|"` (Base64),
    /// signed by the owner at send time.
    #[serde(default)]
    pub signature: String,
}

// ── ChannelKick ───────────────────────────────────────────────────────────

/// Owner → a member being removed (or broadcast to all on channel
/// deletion). Mirrors plain-app `ChannelKick`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelKick {
    pub channel_id: String,
    /// Channel version at time of kick — included in the signature
    /// payload to bind the kick to a specific channel state.
    #[serde(default)]
    pub version: i64,
    /// Ed25519 signature of `"$channelId|$version|kick|<kicked peer id>"`
    /// (Base64), signed by the owner at send time.
    #[serde(default)]
    pub signature: String,
}

// ── ChannelLeave ──────────────────────────────────────────────────────────

/// Member → Owner: the sender is voluntarily leaving the channel.
/// Mirrors plain-app `ChannelLeave`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelLeave {
    pub channel_id: String,
}

// ── Members helpers ──────────────────────────────────────────────────────

/// Format a member roster back to the storage string
/// (`[{"id","status"}, ...]`, status as `JOINED`/`PENDING`).
pub fn encode_members(members: &[ChannelMember]) -> String {
    serde_json::to_string(members).unwrap_or_else(|_| "[]".to_string())
}

/// Parse the `members` storage string into a typed roster.
pub fn decode_members(raw: &str) -> Vec<ChannelMember> {
    serde_json::from_str(raw).unwrap_or_default()
}

/// Does the members list already contain `peer_id`?
pub fn has_member(members: &[ChannelMember], peer_id: &str) -> bool {
    members.iter().any(|m| m.peer_id == peer_id)
}

/// Find a member entry by peer id.
#[allow(dead_code)]
pub fn find_member<'a>(members: &'a [ChannelMember], peer_id: &str) -> Option<&'a ChannelMember> {
    members.iter().find(|m| m.peer_id == peer_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use plain_rs::{base64_encode, ed25519_generate, ed25519_sign, ed25519_verify};

    /// Verify the canonical payload format matches plain-app's
    /// `channelMessagePayload`: `"$channelId|$version|$action|$target"`.
    #[test]
    fn payload_format_matches_android() {
        assert_eq!(
            channel_message_payload("ch_abc", 3, ChannelSystemMessageAction::Invite, "peer_xyz"),
            "ch_abc|3|INVITE|peer_xyz"
        );
        assert_eq!(
            channel_message_payload("ch_abc", 5, ChannelSystemMessageAction::Update, ""),
            "ch_abc|5|UPDATE|"
        );
        assert_eq!(
            channel_message_payload("ch_abc", 9, ChannelSystemMessageAction::Kick, ""),
            "ch_abc|9|KICK|"
        );
    }

    /// Round-trip: sign an `invite` payload with the owner's keypair,
    /// then verify with the corresponding public key.
    #[test]
    fn signature_roundtrip_invite() {
        let (kp_bytes, vk_bytes) = ed25519_generate();
        let payload =
            channel_message_payload("ch_1", 1, ChannelSystemMessageAction::Invite, "peer_a");
        let sig = ed25519_sign(&kp_bytes, payload.as_bytes());
        assert!(!sig.is_empty(), "signature should not be empty");
        let pub_key_b64 = base64_encode(&vk_bytes);
        assert!(
            ed25519_verify(&pub_key_b64, payload.as_bytes(), &sig),
            "valid signature should verify"
        );
    }

    #[test]
    fn signature_roundtrip_update() {
        let (kp_bytes, vk_bytes) = ed25519_generate();
        let payload = channel_message_payload("ch_2", 7, ChannelSystemMessageAction::Update, "");
        let sig = ed25519_sign(&kp_bytes, payload.as_bytes());
        let pub_key_b64 = base64_encode(&vk_bytes);
        assert!(ed25519_verify(&pub_key_b64, payload.as_bytes(), &sig));
    }

    #[test]
    fn signature_roundtrip_kick() {
        let (kp_bytes, vk_bytes) = ed25519_generate();
        let payload =
            channel_message_payload("ch_3", 2, ChannelSystemMessageAction::Kick, "peer_b");
        let sig = ed25519_sign(&kp_bytes, payload.as_bytes());
        let pub_key_b64 = base64_encode(&vk_bytes);
        assert!(ed25519_verify(&pub_key_b64, payload.as_bytes(), &sig));
    }

    /// Tampering with the payload or signature must fail verification.
    #[test]
    fn signature_tamper_fails() {
        let (kp_bytes, vk_bytes) = ed25519_generate();
        let payload =
            channel_message_payload("ch_4", 1, ChannelSystemMessageAction::Invite, "peer_c");
        let sig = ed25519_sign(&kp_bytes, payload.as_bytes());
        let pub_key_b64 = base64_encode(&vk_bytes);

        // Tamper with version in the payload.
        let tampered =
            channel_message_payload("ch_4", 99, ChannelSystemMessageAction::Invite, "peer_c");
        assert!(
            !ed25519_verify(&pub_key_b64, tampered.as_bytes(), &sig),
            "tampered payload should fail verification"
        );

        // Tamper with the target peer id.
        let tampered_target =
            channel_message_payload("ch_4", 1, ChannelSystemMessageAction::Invite, "peer_evil");
        assert!(
            !ed25519_verify(&pub_key_b64, tampered_target.as_bytes(), &sig),
            "tampered target should fail verification"
        );

        // Garbage signature.
        let garbage_sig = base64_encode(&[0u8; 64]);
        assert!(
            !ed25519_verify(&pub_key_b64, payload.as_bytes(), &garbage_sig),
            "garbage signature should fail verification"
        );
    }

    /// A `ChannelInvite` round-trips through JSON with camelCase wire names
    /// matching the Kotlin `@Serializable` data class.
    #[test]
    fn channel_invite_serializes_to_camelcase_wire() {
        let invite = ChannelInvite {
            channel_id: "ch_x".to_string(),
            channel_name: "Channel".to_string(),
            key: "a2V5".to_string(),
            owner: "owner-1".to_string(),
            members: vec![
                ChannelMember::new("owner-1"),
                ChannelMember::pending("peer-1"),
            ],
            member_peers: vec![MemberPeerInfo {
                id: "owner-1".to_string(),
                name: "Desktop".to_string(),
                public_key: "PUB".to_string(),
                device_type: DeviceType::Computer,
                ip: "".to_string(),
                port: 0,
            }],
            version: 3,
            signature: "SIG".to_string(),
        };

        let json = serde_json::to_value(&invite).expect("serialize");

        // Wire uses camelCase for multi-word fields.
        assert_eq!(json["channelId"], "ch_x");
        assert_eq!(json["channelName"], "Channel");
        assert_eq!(json["memberPeers"][0]["publicKey"], "PUB");
        assert_eq!(json["memberPeers"][0]["deviceType"], "COMPUTER");
        assert_eq!(json["members"][1]["status"], "PENDING");

        // Round-trip back to the typed struct.
        let back: ChannelInvite = serde_json::from_value(json).expect("deserialize");
        assert_eq!(back, invite);
    }

    /// `encode_members`/`decode_members` preserve the exact storage
    /// format (`[{"peerId","status"}]` with `JOINED`/`PENDING`); legacy
    /// `"id"` keys (pre-rename rows/messages) still decode via the alias.
    #[test]
    fn members_encode_decode_roundtrip() {
        let roster = vec![ChannelMember::new("a"), ChannelMember::pending("b")];
        let json = encode_members(&roster);
        assert_eq!(
            json,
            r#"[{"peerId":"a","status":"JOINED"},{"peerId":"b","status":"PENDING"}]"#
        );
        assert_eq!(decode_members(&json), roster);
        // Legacy storage/wire keys written by pre-rename app versions.
        assert_eq!(
            decode_members(r#"[{"id":"a","status":"JOINED"},{"id":"b","status":"PENDING"}]"#),
            roster
        );
        assert!(has_member(&roster, "a"));
        assert!(!has_member(&roster, "c"));
        assert_eq!(find_member(&roster, "b").map(|m| m.peer_id.as_str()), Some("b"));
    }
}
