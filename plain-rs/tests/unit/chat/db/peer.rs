use super::*;
use crate::chat::db::tests::unique_tmp_dir;
use crate::chat::enums::{DeviceType, PeerStatus};

fn seed_peer(db: &ChatDb, id: &str, status: PeerStatus, key: &str) {
    let mut peer = DPeer::new(id, id, "198.51.100.1", 12345, DeviceType::Phone);
    peer.status = status;
    peer.key = key.to_string();
    db.upsert_peer(&peer);
}

#[test]
fn update_peer_status_flips_status_and_bumps_updated_at() {
    let db = ChatDb::open(&unique_tmp_dir("status-flip").join("local_chat.db")).expect("open db");
    seed_peer(&db, "p1", PeerStatus::Paired, "k");
    let before = db.get_peer_by_id("p1").expect("peer exists");
    assert_eq!(before.status, PeerStatus::Paired);

    db.update_peer_status("p1", PeerStatus::Unpaired);

    let after = db.get_peer_by_id("p1").expect("peer still exists");
    assert_eq!(after.status, PeerStatus::Unpaired);
    assert_eq!(after.key, "k");
    assert!(after.updated_at >= before.updated_at);
}

#[test]
fn update_peer_status_and_key_clears_key_for_channel_demotion() {
    let db = ChatDb::open(&unique_tmp_dir("key-clear").join("local_chat.db")).expect("open db");
    seed_peer(&db, "p1", PeerStatus::Paired, "secret-key");

    db.update_peer_status_and_key("p1", PeerStatus::Channel, "");

    let after = db.get_peer_by_id("p1").expect("peer demoted, not deleted");
    assert_eq!(after.status, PeerStatus::Channel);
    assert_eq!(after.key, "");
}

#[test]
fn delete_peer_removes_row() {
    let db = ChatDb::open(&unique_tmp_dir("delete").join("local_chat.db")).expect("open db");
    seed_peer(&db, "p1", PeerStatus::Paired, "k");
    assert!(db.get_peer_by_id("p1").is_some());

    db.delete_peer("p1");
    assert!(db.get_peer_by_id("p1").is_none());
}

#[test]
fn login_peer_creates_unpaired_peer_with_token() {
    let db = ChatDb::open(&unique_tmp_dir("login-new").join("local_chat.db")).expect("open db");

    db.login_peer(
        "p1",
        "Pixel 9",
        "203.0.113.10",
        8443,
        DeviceType::Phone,
        "tok1",
        "sig1",
        "",
    )
    .expect("login peer");

    let peer = db.get_peer_by_id("p1").expect("login creates peer");
    assert_eq!(peer.status, PeerStatus::Unpaired);
    assert_eq!(peer.token, "tok1");
    assert_eq!(peer.public_key, "sig1");
    assert_eq!(peer.ip, "203.0.113.10");
    assert_eq!(peer.port, 8443);
    assert_eq!(db.get_login_peers().len(), 1);
}

#[test]
fn login_peer_refreshes_existing_row_and_keeps_pairing_state() {
    let db = ChatDb::open(&unique_tmp_dir("login-paired").join("local_chat.db")).expect("open db");
    seed_peer(&db, "p1", PeerStatus::Paired, "chat-key");

    db.login_peer(
        "p1",
        "Pixel 9",
        "203.0.113.20",
        8443,
        DeviceType::Phone,
        "tok2",
        "",
        "",
    )
    .expect("refresh login peer");

    let peer = db.get_peer_by_id("p1").expect("peer still exists");
    assert_eq!(peer.status, PeerStatus::Paired);
    assert_eq!(peer.key, "chat-key");
    assert_eq!(peer.token, "tok2");
    assert_eq!(peer.ip, "203.0.113.20");
}

#[test]
fn login_peer_with_chat_key_creates_paired_peer_with_token() {
    let db = ChatDb::open(&unique_tmp_dir("login-chat").join("local_chat.db")).expect("open db");
    db.login_peer(
        "p1",
        "Pixel 9",
        "203.0.113.10",
        8443,
        DeviceType::Phone,
        "token",
        "phone-signature-key",
        "chat-key",
    )
    .expect("login and pair peer");

    let peer = db.get_peer_by_id("p1").expect("paired peer");
    assert_eq!(peer.status, PeerStatus::Paired);
    assert_eq!(peer.key, "chat-key");
    assert_eq!(peer.token, "token");
    assert_eq!(peer.public_key, "phone-signature-key");
}

#[test]
fn logout_peer_clears_token_and_drops_from_login_list() {
    let db = ChatDb::open(&unique_tmp_dir("logout").join("local_chat.db")).expect("open db");
    db.login_peer(
        "p1",
        "Pixel 9",
        "203.0.113.10",
        8443,
        DeviceType::Phone,
        "tok1",
        "sig1",
        "",
    )
    .expect("login peer");
    assert_eq!(db.get_login_peers().len(), 1);

    db.logout_peer("p1");

    let peer = db
        .get_peer_by_id("p1")
        .expect("row kept, only token cleared");
    assert_eq!(peer.token, "");
    assert!(db.get_login_peers().is_empty());
}

#[test]
fn update_peer_name_renames_only() {
    let db = ChatDb::open(&unique_tmp_dir("rename").join("local_chat.db")).expect("open db");
    db.login_peer(
        "p1",
        "old",
        "203.0.113.10",
        8443,
        DeviceType::Phone,
        "tok1",
        "sig1",
        "",
    )
    .expect("login peer");

    db.update_peer_name("p1", "new-name");

    let peer = db.get_peer_by_id("p1").expect("peer exists");
    assert_eq!(peer.name, "new-name");
    assert_eq!(peer.token, "tok1");
    assert_eq!(peer.ip, "203.0.113.10");
}

#[test]
fn peer_url_helpers_use_best_ip_and_port() {
    let mut peer = DPeer::new("p1", "p1", "203.0.113.7,203.0.113.8", 8443, DeviceType::Nas);
    assert_eq!(peer.best_ip(), "203.0.113.7");
    assert_eq!(peer.base_url(), "https://203.0.113.7:8443");
    assert_eq!(
        peer.peer_graphql_url(),
        "https://203.0.113.7:8443/peer_graphql"
    );
    assert_eq!(peer.file_url("abc"), "https://203.0.113.7:8443/fs?id=abc");
    // Default HTTPS port is omitted.
    peer.port = 443;
    peer.ip = "203.0.113.7".to_string();
    assert_eq!(peer.base_url(), "https://203.0.113.7");
}
