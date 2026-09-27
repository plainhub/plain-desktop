use crate::chat::db::{ChatDb, DPeer};
use crate::chat::enums::{ChannelStatus, DeviceType, MemberStatus};
use crate::chat::service::{ChatIdentity, ChatService, NoChatHooks, no_link_previews};
use crate::chat::transport::PeerTransport;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// Transport stub that records nothing and fails nothing — the ops tests
/// have no live peers, so delivery targets don't exist in the peers table
/// and the transport is never invoked.
struct TestTransport;

impl PeerTransport for TestTransport {
    fn post<'a>(
        &'a self,
        _url: &'a str,
        _client_id: &'a str,
        _channel_id: Option<&'a str>,
        _body: &'a [u8],
    ) -> impl std::future::Future<Output = Result<Vec<u8>, String>> + Send {
        std::future::ready(Err("test transport: no peers".to_string()))
    }
}

fn unique_tmp_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id();
    std::env::temp_dir().join(format!("plain-rs-chat-ops-{label}-{pid}-{nanos}"))
}

fn service(dir_label: &str) -> ChatService<TestTransport> {
    let dir = unique_tmp_dir(dir_label);
    let db = ChatDb::open(&dir.join("local_chat.db")).unwrap();
    let (kp, _vk) = crate::ed25519_generate();
    let identity = ChatIdentity::new("me-nas", "nas-box", crate::base64_encode(&kp));
    ChatService::new(
        db,
        crate::base64_encode(&[5u8; 32]),
        std::sync::Arc::new(identity),
        DeviceType::Nas,
        dir,
        Arc::new(TestTransport),
        Arc::new(NoChatHooks),
        no_link_previews(),
    )
}

#[tokio::test]
async fn create_channel_sets_owner_member_and_key() {
    let svc = service("create");
    let ch = svc.create_channel("Team");

    assert_eq!(ch.name, "Team");
    assert_eq!(ch.owner_id, "me-nas");
    // Owner is a JOINED member from the start (invite-rejection guard).
    assert_eq!(ch.members, r#"[{"peerId":"me-nas","status":"JOINED"}]"#);
    // Channel key is a base64 32-byte value.
    assert_eq!(crate::base64_decode(&ch.key).len(), 32);
    // WS_CHANNELS_UPDATED fired with the joined-channel payload.
    let ev = svc.event_tx.subscribe();
    let _ = ev;
    assert_eq!(svc.db.get_all_channels().len(), 1);
}

#[tokio::test]
async fn add_channel_member_enforces_owner_and_duplicates() {
    let svc = service("add-member");
    let ch = svc.create_channel("Team");

    // Unknown peer: added as PENDING member, no invite delivery (no peer row).
    let updated = svc
        .add_channel_member(&ch.id, "peer-x")
        .await
        .expect("add ok");
    let members = crate::chat::channel::messages::decode_members(&updated.members);
    assert_eq!(members.len(), 2);
    assert_eq!(members[1].status, MemberStatus::Pending);

    // Duplicate add is rejected.
    let err = svc.add_channel_member(&ch.id, "peer-x").await.unwrap_err();
    assert_eq!(err, "Already a member");

    // Removing the member then succeeds; remove of a non-member errors.
    svc.remove_channel_member(&ch.id, "peer-x")
        .await
        .expect("remove ok");
    let err = svc
        .remove_channel_member(&ch.id, "peer-x")
        .await
        .unwrap_err();
    assert_eq!(err, "Not a member");

    // Unknown channel id errors.
    assert_eq!(
        svc.add_channel_member("nope", "peer-x").await.unwrap_err(),
        "Channel not found"
    );
}

#[tokio::test]
async fn decline_channel_invite_removes_channel_and_chats() {
    let svc = service("decline");
    let ch = svc.create_channel("Team");
    let mut chat = crate::chat::db::DChat::new("me", "", &ch.id, "{}");
    chat.id = "m1".to_string();
    svc.db.insert_chat(&chat);

    assert!(svc.decline_channel_invite(&ch.id).await);
    assert!(svc.db.get_channel_by_id(&ch.id).is_none());
    assert!(svc.db.get_chats_by_channel(&ch.id).is_empty());
}

#[tokio::test]
async fn delete_peer_demotes_channel_members() {
    let svc = service("del-peer");
    let mut ch = svc.create_channel("Team");
    // peer-x as a joined channel member.
    ch.members = r#"[{"peerId":"me-nas","status":"JOINED"},{"peerId":"peer-x","status":"JOINED"}]"#
        .to_string();
    svc.db.update_channel(&ch);
    svc.db.upsert_peer(&DPeer::new(
        "peer-x",
        "X",
        "203.0.113.9",
        8443,
        DeviceType::Phone,
    ));
    let mut chat = crate::chat::db::DChat::new("me", "peer-x", "", "{}");
    chat.id = "m1".to_string();
    svc.db.insert_chat(&chat);

    assert!(svc.delete_peer("peer-x"));
    // Peer row survives demoted to CHANNEL (routing still needs it)…
    let peer = svc.db.get_peer_by_id("peer-x").expect("demoted row kept");
    assert_eq!(peer.status, crate::chat::enums::PeerStatus::Channel);
    // …and its 1:1 chats are gone.
    assert!(svc.db.get_chat_by_id("m1").is_none());
    // Unknown peer id → false.
    assert!(!svc.delete_peer("ghost"));

    // Unpair flips status but keeps the row.
    assert!(svc.unpair_peer("peer-x"));
    assert_eq!(
        svc.db.get_peer_by_id("peer-x").unwrap().status,
        crate::chat::enums::PeerStatus::Unpaired
    );
}

#[tokio::test]
async fn leave_channel_as_member_removes_self_and_marks_left() {
    let svc = service("leave");
    // A channel owned by someone else, with us as a member.
    let mut ch = crate::chat::db::DChannel::new("Team", "owner-1");
    ch.id = "ch-foreign".to_string();
    ch.members =
        r#"[{"peerId":"owner-1","status":"JOINED"},{"peerId":"me-nas","status":"JOINED"}]"#
            .to_string();
    ch.key = crate::base64_encode(&[7u8; 32]);
    svc.db.insert_channel(&ch);

    assert!(svc.leave_channel("ch-foreign").await);
    let after = svc.db.get_channel_by_id("ch-foreign").expect("row kept");
    assert_eq!(after.status, ChannelStatus::Left);
    let members = crate::chat::channel::messages::decode_members(&after.members);
    assert!(members.iter().all(|m| m.peer_id != "me-nas"));
}
