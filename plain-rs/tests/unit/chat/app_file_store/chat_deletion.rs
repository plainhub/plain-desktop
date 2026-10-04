use super::*;
use crate::{
    chat::app_file_store::import_bytes,
    db::{
        DChat,
        chat_store::{SaveMode, messages},
    },
};

#[test]
fn deletion_releases_exact_references_and_preserves_other_conversations() {
    let root = std::env::temp_dir().join(format!(
        "plain-chat-delete-{}",
        crate::utils::short_uuid::short_uuid()
    ));
    fs::create_dir_all(&root).unwrap();
    let db = Db::open(&root.join("plain.db")).unwrap();
    let file = import_bytes(&db, &root, b"owned fixture", "text/plain").unwrap();
    import_bytes(&db, &root, b"owned fixture", "text/plain").unwrap();
    import_bytes(&db, &root, b"owned fixture", "text/plain").unwrap();
    let content = serde_json::json!({"type":"FILES","value":{"items":[{"uri":format!("fid:{}",file.fid_suffix)},{"uri":"fsid:foreign"}]}}).to_string();
    let mut direct = DChat::new("me", "peer", "", &content);
    direct.id = "direct".into();
    let mut group = DChat::new("peer", "me", "group", &content);
    group.id = "group-message".into();
    messages::save(&db, &[direct, group], SaveMode::Insert).unwrap();
    assert_eq!(
        delete(
            &db,
            &root,
            Selection::Ids(&["direct".into(), "direct".into(), "absent".into()])
        )
        .unwrap(),
        1
    );
    assert_eq!(db.app_file_get(&file.id).unwrap().unwrap().ref_count, 2);
    assert!(root.join(&file.real_path).exists());
    assert_eq!(delete(&db, &root, Selection::Peer("peer")).unwrap(), 0);
    assert!(messages::get(&db, "group-message").unwrap().is_some());
    assert_eq!(delete(&db, &root, Selection::Channel("group")).unwrap(), 1);
    assert_eq!(db.app_file_get(&file.id).unwrap().unwrap().ref_count, 1);
    assert!(root.join(&file.real_path).exists());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn failed_sql_restores_attachment_and_message_then_retry_deletes_once() {
    let root = std::env::temp_dir().join(format!(
        "plain-chat-rollback-{}",
        crate::utils::short_uuid::short_uuid()
    ));
    fs::create_dir_all(&root).unwrap();
    let db = Db::open(&root.join("plain.db")).unwrap();
    let file = import_bytes(&db, &root, b"rollback fixture", "image/png").unwrap();
    let content = serde_json::json!({"type":"IMAGES","value":{"items":[{"uri":format!("fid:{}",file.fid_suffix)}]}}).to_string();
    let mut chat = DChat::new("me", "peer", "", &content);
    chat.id = "message".into();
    messages::save(&db, &[chat], SaveMode::Insert).unwrap();
    db.with_conn(|c| c.execute_batch("CREATE TRIGGER reject_delete BEFORE DELETE ON chats BEGIN SELECT RAISE(ABORT,'test rollback'); END;")).unwrap();
    assert!(delete(&db, &root, Selection::Peer("peer")).is_err());
    assert!(messages::get(&db, "message").unwrap().is_some());
    assert_eq!(db.app_file_get(&file.id).unwrap().unwrap().ref_count, 1);
    assert_eq!(
        fs::read(root.join(&file.real_path)).unwrap(),
        b"rollback fixture"
    );
    db.with_conn(|c| c.execute_batch("DROP TRIGGER reject_delete"))
        .unwrap();
    assert_eq!(delete(&db, &root, Selection::Peer("peer")).unwrap(), 1);
    assert!(db.app_file_get(&file.id).unwrap().is_none());
    assert!(!root.join(&file.real_path).exists());
    assert_eq!(
        delete(&db, &root, Selection::Ids(&["message".into()])).unwrap(),
        0
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn entity_removal_is_atomic_retains_channel_peer_and_returns_removed_channel_snapshot() {
    use crate::{
        chat::enums::{DeviceType, PeerStatus},
        db::{
            DChannel, DPeer,
            chat_store::{channels, peers},
        },
    };
    let root = std::env::temp_dir().join(format!(
        "plain-entity-delete-{}",
        crate::utils::short_uuid::short_uuid()
    ));
    fs::create_dir_all(&root).unwrap();
    let db = Db::open(&root.join("plain.db")).unwrap();
    let file = import_bytes(&db, &root, b"entity fixture", "text/plain").unwrap();
    import_bytes(&db, &root, b"entity fixture", "text/plain").unwrap();
    let mut peer = DPeer::new("peer", "peer", "", 1, DeviceType::Phone);
    peer.status = PeerStatus::Paired;
    peer.key = "pair-key".into();
    peer.token = "login-token".into();
    peer.public_key = "public".into();
    peers::save(&db, &[peer], SaveMode::Insert).unwrap();
    let mut channel = DChannel::new("group", "owner");
    channel.id = "group".into();
    channel.members = r#"[{"peerId":"peer","status":"PENDING"}]"#.into();
    channel.key = "channel-secret".into();
    channels::save(&db, &[channel], SaveMode::Insert).unwrap();
    let content=serde_json::json!({"type":"FILES","value":{"items":[{"uri":format!("fid:{}",file.fid_suffix)}]}}).to_string();
    let mut direct = DChat::new("me", "peer", "", &content);
    direct.id = "direct".into();
    let mut group = DChat::new("peer", "me", "group", &content);
    group.id = "group-message".into();
    messages::save(&db, &[direct, group], SaveMode::Insert).unwrap();
    assert_eq!(
        delete(&db, &root, Selection::PeerRecord("peer")).unwrap(),
        1
    );
    let peer = peers::get(&db, "peer").unwrap().unwrap();
    assert_eq!(peer.status, PeerStatus::Channel);
    assert!(peer.key.is_empty());
    assert_eq!(peer.token, "login-token");
    assert_eq!(peer.public_key, "public");
    assert!(messages::get(&db, "direct").unwrap().is_none());
    assert!(messages::get(&db, "group-message").unwrap().is_some());
    assert_eq!(db.app_file_get(&file.id).unwrap().unwrap().ref_count, 1);
    db.with_conn(|c| c.execute_batch("CREATE TRIGGER reject_channel BEFORE DELETE ON chat_channels BEGIN SELECT RAISE(ABORT,'entity rollback'); END;")).unwrap();
    assert!(remove_channel(&db, &root, "group").is_err());
    assert!(channels::get(&db, "group").unwrap().is_some());
    assert!(messages::get(&db, "group-message").unwrap().is_some());
    assert!(db.app_file_get(&file.id).unwrap().is_some());
    assert_eq!(
        fs::read(root.join(&file.real_path)).unwrap(),
        b"entity fixture"
    );
    db.with_conn(|c| c.execute_batch("DROP TRIGGER reject_channel"))
        .unwrap();
    assert_eq!(
        remove_channel(&db, &root, "group").unwrap().unwrap().key,
        "channel-secret"
    );
    assert!(messages::get(&db, "group-message").unwrap().is_none());
    assert!(db.app_file_get(&file.id).unwrap().is_none());
    assert!(!root.join(&file.real_path).exists());
    assert_eq!(
        delete(&db, &root, Selection::PeerRecord("peer")).unwrap(),
        1
    );
    assert!(peers::get(&db, "peer").unwrap().is_none());
    assert_eq!(
        delete(&db, &root, Selection::PeerRecord("peer")).unwrap(),
        0
    );
    assert!(remove_channel(&db, &root, "group").unwrap().is_none());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn discovery_and_unpair_update_only_owned_fields_and_require_paired_peer() {
    use crate::{
        chat::enums::{DeviceType, PeerStatus},
        db::{DPeer, chat_store::peers},
    };
    let db = Db::open(Path::new(":memory:")).unwrap();
    let mut peer = DPeer::new("peer", "old", "old-ip", 1, DeviceType::Phone);
    peer.status = PeerStatus::Paired;
    peer.key = "current-key".into();
    peer.token = "session".into();
    peers::save(&db, &[peer], SaveMode::Insert).unwrap();
    let refreshed = peers::discovered(
        &db,
        "peer",
        &["127.0.0.1".into(), "::1".into()],
        443,
        "new",
        DeviceType::Computer,
    )
    .unwrap()
    .unwrap();
    assert_eq!(refreshed.ip, "127.0.0.1,::1");
    assert_eq!(refreshed.name, "new");
    assert_eq!(refreshed.key, "current-key");
    assert_eq!(refreshed.token, "session");
    assert!(peers::unpair(&db, "peer").unwrap());
    let unpaired = peers::get(&db, "peer").unwrap().unwrap();
    assert_eq!(unpaired.status, PeerStatus::Unpaired);
    assert_eq!(unpaired.key, "current-key");
    assert_eq!(unpaired.token, "session");
    assert!(
        peers::discovered(&db, "peer", &[], 1, "stale", DeviceType::Phone)
            .unwrap()
            .is_none()
    );
    assert_eq!(peers::get(&db, "peer").unwrap().unwrap().name, "new");
    assert!(!peers::unpair(&db, "missing").unwrap());
}

#[test]
fn checked_channel_delete_preserves_newer_row_message_and_attachment() {
    use crate::{
        chat::channel::state::{self, Action},
        db::{DChannel, chat_store::channels},
    };
    let root = std::env::temp_dir().join(format!(
        "plain-checked-channel-{}",
        crate::utils::short_uuid::short_uuid()
    ));
    fs::create_dir_all(&root).unwrap();
    let db = Db::open(&root.join("plain.db")).unwrap();
    let channel = state::create(&db, "actor", "fixture").unwrap();
    let file = import_bytes(&db, &root, b"channel attachment", "text/plain").unwrap();
    let content=serde_json::json!({"type":"FILES","value":{"items":[{"uri":format!("fid:{}",file.fid_suffix)}]}}).to_string();
    let chat = DChat::new("actor", "", &channel.id, &content);
    messages::save(&db, &[chat.clone()], SaveMode::Insert).unwrap();
    let current: DChannel = state::apply(
        &db,
        "actor",
        &channel.id,
        Action::Rename {
            name: "newer".into(),
        },
    )
    .unwrap();
    assert!(remove_channel_if_matches(&db, &root, &channel).is_err());
    assert_eq!(channels::get(&db, &channel.id).unwrap().unwrap(), current);
    assert!(messages::get(&db, &chat.id).unwrap().is_some());
    assert_eq!(db.app_file_get(&file.id).unwrap().unwrap().ref_count, 1);
    assert_eq!(
        fs::read(root.join(&file.real_path)).unwrap(),
        b"channel attachment"
    );
    assert_eq!(
        remove_channel_if_matches(&db, &root, &current)
            .unwrap()
            .unwrap(),
        current
    );
    assert!(channels::get(&db, &channel.id).unwrap().is_none());
    assert!(messages::get(&db, &chat.id).unwrap().is_none());
    assert!(!root.join(&file.real_path).exists());
    fs::remove_dir_all(root).unwrap();
}
