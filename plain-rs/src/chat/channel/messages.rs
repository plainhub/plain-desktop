//! Wire types for `channelSystemMessage` GraphQL payloads.
//!
//! Mirrors plain-app `ChannelSystemMessages` (see
//! `plain-app/.../channel/ChannelSystemMessage.kt`) with full
//! strong-typed data structures instead of weakly-typed JSON maps.
//! Every struct is `Serialize`/`Deserialize` with `camelCase` wire
//! names so the JSON we send/receive matches the Kotlin `@Serializable`
//! data classes exactly.

use serde::{Deserialize, Serialize};

use crate::chat::enums::{ChannelSystemMessageAction, DeviceType, MemberStatus};

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
/// (`[{"peerId","status"}, ...]`, status as `JOINED`/`PENDING`).
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
pub fn find_member<'a>(members: &'a [ChannelMember], peer_id: &str) -> Option<&'a ChannelMember> {
    members.iter().find(|m| m.peer_id == peer_id)
}

#[cfg(test)]
#[path = "../../../tests/unit/chat/channel/messages.rs"]
mod tests;
