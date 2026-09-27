use super::*;
use crate::chat::db::{ChatDb, DPeer};
use crate::chat::enums::{DeviceType, MemberStatus};
use crate::ed25519_generate;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_tmp_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id();
    std::env::temp_dir().join(format!("plain-rs-chat-sender-{label}-{pid}-{nanos}"))
}

fn build(
    channel: &DChannel,
    db: &ChatDb,
    client_id: &str,
    name: &str,
    kp: &[u8],
) -> Vec<MemberPeerInfo> {
    build_member_peers(channel, db, client_id, name, DeviceType::Nas, kp)
}

/// Regression test for the invite flow: `build_member_peers` must include
/// the owner's own `MemberPeerInfo` whenever the owner is a member of the
/// channel. Without it, the invitee's `handleInvite` rejects the invite
/// with "no owner memberPeerInfo". Mirrors plain-app `getPeersAsync`.
#[test]
fn build_member_peers_includes_owner_when_owner_is_member() {
    let db = ChatDb::open(&unique_tmp_dir("owner-member").join("local_chat.db")).expect("open db");
    let (kp_bytes, _vk_bytes) = ed25519_generate();
    let client_id = "owner-1";
    let device_name = "NAS";

    let mut channel = DChannel::new("Channel", client_id);
    channel.members = serde_json::json!([
        { "id": client_id, "status": MemberStatus::Joined.to_string() }
    ])
    .to_string();

    let member_peers = build(&channel, &db, client_id, device_name, &kp_bytes);

    let owner_entry = member_peers.iter().find(|m| m.id == client_id);
    assert!(owner_entry.is_some(), "owner must appear in memberPeers");
    let owner_entry = owner_entry.unwrap();
    assert_eq!(
        owner_entry.public_key,
        crate::base64_encode(&kp_bytes[32..])
    );
    assert_eq!(owner_entry.device_type, DeviceType::Nas);
}

/// The owner must appear exactly once, even alongside other members.
#[test]
fn build_member_peers_includes_owner_alongside_members() {
    let db =
        ChatDb::open(&unique_tmp_dir("owner-plus-member").join("local_chat.db")).expect("open db");
    let (kp_bytes, _vk_bytes) = ed25519_generate();
    let client_id = "owner-1";
    let member_id = "member-1";
    db.upsert_peer(&DPeer::new(
        member_id,
        "Pixel",
        "203.0.113.5",
        8443,
        DeviceType::Phone,
    ));

    let mut channel = DChannel::new("Channel", client_id);
    channel.members = serde_json::json!([
        { "id": client_id, "status": MemberStatus::Joined.to_string() },
        { "id": member_id, "status": MemberStatus::Pending.to_string() }
    ])
    .to_string();

    let member_peers = build(&channel, &db, client_id, "NAS", &kp_bytes);
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
        "203.0.113.5",
        8443,
        DeviceType::Phone,
    ));

    // Owner is deliberately NOT in members — only the invitee is.
    let mut channel = DChannel::new("Channel", client_id);
    channel.members = serde_json::json!([
        { "id": member_id, "status": MemberStatus::Pending.to_string() }
    ])
    .to_string();

    let member_peers = build(&channel, &db, client_id, "NAS", &kp_bytes);
    let ids: Vec<&str> = member_peers.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids.len(), 2, "owner + member should both be present");
    assert_eq!(ids[0], client_id, "owner must be first");
    assert_eq!(ids[1], member_id);
}
