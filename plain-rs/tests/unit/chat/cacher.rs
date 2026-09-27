use super::*;
use crate::chat::db::{ChatDb, DChannel, DPeer};
use crate::chat::enums::{ChannelStatus, DeviceType, PeerStatus};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_tmp_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id();
    std::env::temp_dir().join(format!("plain-rs-chat-cacher-{label}-{pid}-{nanos}"))
}

fn seed_chat(db: &ChatDb, id: &str, from_id: &str, to_id: &str, channel_id: &str) {
    let mut chat = DChat::new(from_id, to_id, channel_id, "{}");
    chat.id = id.to_string();
    db.insert_chat(&chat);
}

fn seed_peer(db: &ChatDb, id: &str) {
    let now = crate::chat::db::now_iso();
    let peer = DPeer {
        id: id.to_string(),
        name: id.to_string(),
        ip: String::new(),
        key: String::new(),
        public_key: String::new(),
        status: PeerStatus::Paired,
        port: 0,
        device_type: DeviceType::Unknown,
        token: String::new(),
        created_at: now.clone(),
        updated_at: now,
    };
    db.upsert_peer(&peer);
}

fn seed_channel(db: &ChatDb, id: &str) {
    let mut ch = DChannel::new(id, "me");
    ch.id = id.to_string();
    ch.status = ChannelStatus::Joined;
    db.insert_channel(&ch);
}

#[test]
fn load_caches_local_chat() {
    let db = ChatDb::open(&unique_tmp_dir("local").join("local_chat.db")).expect("open db");
    seed_chat(&db, "a", "me", "local", "");

    let cacher = ChatCacher::new();
    cacher.load(&db);

    assert!(cacher.get_latest_chat("local").is_some());
}

#[test]
fn load_caches_peer_chat() {
    let db = ChatDb::open(&unique_tmp_dir("peer").join("local_chat.db")).expect("open db");
    seed_peer(&db, "peer1");
    seed_chat(&db, "a", "me", "peer1", "");

    let cacher = ChatCacher::new();
    cacher.load(&db);

    assert!(cacher.get_latest_chat("peer1").is_some());
}

#[test]
fn load_caches_channel_chat() {
    let db = ChatDb::open(&unique_tmp_dir("chan").join("local_chat.db")).expect("open db");
    seed_channel(&db, "ch1");
    seed_chat(&db, "a", "me", "", "ch1");

    let cacher = ChatCacher::new();
    cacher.load(&db);

    assert!(cacher.get_latest_chat("ch1").is_some());
}

#[test]
fn load_skips_chats_for_unknown_peer() {
    let db = ChatDb::open(&unique_tmp_dir("unknown").join("local_chat.db")).expect("open db");
    seed_chat(&db, "a", "me", "ghost", "");

    let cacher = ChatCacher::new();
    cacher.load(&db);

    assert!(cacher.get_latest_chat("ghost").is_none());
}

#[test]
fn load_keeps_most_recent_for_same_conversation() {
    let db = ChatDb::open(&unique_tmp_dir("latest").join("local_chat.db")).expect("open db");
    // now_iso() has seconds resolution, so manually set timestamps
    // to guarantee ordering.
    let mut old = DChat::new("me", "local", "", "{}");
    old.id = "old".to_string();
    old.created_at = "2026-01-01T00:00:01Z".to_string();
    old.updated_at = "2026-01-01T00:00:01Z".to_string();
    db.insert_chat(&old);

    let mut new = DChat::new("me", "local", "", "{}");
    new.id = "new".to_string();
    new.created_at = "2026-01-01T00:00:02Z".to_string();
    new.updated_at = "2026-01-01T00:00:02Z".to_string();
    db.insert_chat(&new);

    let cacher = ChatCacher::new();
    cacher.load(&db);

    let latest = cacher.get_latest_chat("local").unwrap();
    assert_eq!(latest.id, "new");
}
