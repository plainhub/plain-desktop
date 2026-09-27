use super::*;
use crate::chat::db::{ChatDb, DPeer};
use crate::chat::enums::{DeviceType, PeerStatus};
use crate::chat::events::{ChannelKeyCache, new_channel_key_cache};
use crate::{base64_encode, ed25519_generate, ed25519_sign, xchacha_encrypt_raw};
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_db() -> (ChatDb, std::path::PathBuf) {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "plain-rs-chat-peerauth-{}-{seq}-{}.db",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    ));
    let db = ChatDb::open(&path).unwrap();
    (db, path)
}

/// Seed a paired peer with the given shared key and return (peer_id,
/// key, kp_bytes).
fn seed_paired_peer(db: &ChatDb) -> (String, [u8; 32], [u8; 64]) {
    let (kp, vk) = ed25519_generate();
    let key = [77u8; 32];
    let peer = DPeer {
        id: "peer-a".into(),
        name: "Phone".into(),
        ip: "203.0.113.4".into(),
        key: base64_encode(&key),
        public_key: base64_encode(&vk),
        status: PeerStatus::Paired,
        port: 2443,
        device_type: DeviceType::Phone,
        token: String::new(),
        created_at: "2026-01-01T00:00:00Z".into(),
        updated_at: "2026-01-01T00:00:00Z".into(),
    };
    db.upsert_peer(&peer);
    ("peer-a".into(), key, kp)
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

/// Build an encrypted+signed request body exactly like the sender side
/// (`deliver_to_peer` wire format).
fn build_body(kp: &[u8], key: &[u8; 32], graphql_json: &str, ts: i64) -> Vec<u8> {
    let sig_data = format!("{ts}{graphql_json}");
    let signature = ed25519_sign(kp, sig_data.as_bytes());
    let payload = format!("{signature}|{ts}|{graphql_json}");
    xchacha_encrypt_raw(key, payload.as_bytes()).expect("encrypt")
}

#[test]
fn authenticate_roundtrip_paired_peer() {
    let (db, _path) = unique_db();
    let (peer_id, key, kp) = seed_paired_peer(&db);
    let graphql_json = r#"{"query":"mutation { createChatItem(content: \"x\") { id } }"}"#;
    let body = build_body(&kp, &key, graphql_json, now_ms());

    let cache: ChannelKeyCache = new_channel_key_cache();
    let authed = authenticate(&db, &peer_id, "", &body, &cache).expect("auth ok");
    assert_eq!(authed.peer.id, "peer-a");
    assert_eq!(authed.graphql_json, graphql_json);
    assert_eq!(authed.key, key.to_vec());
}

#[test]
fn authenticate_rejects_unknown_peer_and_unpaired() {
    let (db, _path) = unique_db();
    let (_, key, kp) = seed_paired_peer(&db);
    let cache = new_channel_key_cache();
    let body = build_body(&kp, &key, "{}", now_ms());

    assert!(matches!(
        authenticate(&db, "ghost", "", &body, &cache),
        Err(AuthError::UnknownPeer)
    ));

    // Demote to CHANNEL → direct message rejected, channel message OK.
    db.update_peer_status("peer-a", PeerStatus::Channel);
    assert!(matches!(
        authenticate(&db, "peer-a", "", &body, &cache),
        Err(AuthError::NotPaired)
    ));
    db.update_peer_status("peer-a", PeerStatus::Paired);
}

#[test]
fn authenticate_rejects_wrong_key_and_bad_signature() {
    let (db, _path) = unique_db();
    let (peer_id, key, kp) = seed_paired_peer(&db);
    let cache = new_channel_key_cache();

    // Encrypted with a different key → DecryptFailed.
    let wrong_key = [1u8; 32];
    let body = build_body(&kp, &wrong_key, "{}", now_ms());
    assert!(matches!(
        authenticate(&db, &peer_id, "", &body, &cache),
        Err(AuthError::DecryptFailed)
    ));

    // Signed by a different keypair → BadSignature.
    let (_other_kp, other_vk) = ed25519_generate();
    let mut peer = db.get_peer_by_id("peer-a").unwrap();
    peer.public_key = base64_encode(&other_vk);
    db.upsert_peer(&peer);
    let body = build_body(&kp, &key, "{}", now_ms());
    assert!(matches!(
        authenticate(&db, &peer_id, "", &body, &cache),
        Err(AuthError::BadSignature)
    ));
}

#[test]
fn authenticate_rejects_expired_timestamp_and_missing_fields() {
    let (db, _path) = unique_db();
    let (peer_id, key, kp) = seed_paired_peer(&db);
    let cache = new_channel_key_cache();

    // Six minutes old.
    let body = build_body(&kp, &key, "{}", now_ms() - 6 * 60 * 1000);
    assert!(matches!(
        authenticate(&db, &peer_id, "", &body, &cache),
        Err(AuthError::TimestampExpired)
    ));

    // Missing signature part.
    let payload = format!("|{}|{{}}", now_ms());
    let body = xchacha_encrypt_raw(&key, payload.as_bytes()).unwrap();
    assert!(matches!(
        authenticate(&db, &peer_id, "", &body, &cache),
        Err(AuthError::MissingFields)
    ));
}

#[test]
fn authenticate_channel_message_uses_channel_key_without_paired_check() {
    let (db, _path) = unique_db();
    let (_, peer_key, kp) = seed_paired_peer(&db);
    // Channel member (not paired) with a channel key in the cache.
    db.update_peer_status("peer-a", PeerStatus::Channel);
    let channel_key = [42u8; 32];
    let cache = new_channel_key_cache();
    cache
        .write()
        .unwrap()
        .insert("ch-1".into(), channel_key.to_vec());

    let graphql_json =
        r#"{"query":"mutation { channelSystemMessage(type: INVITE, payload: \"{}\") }"}"#;
    let body = build_body(&kp, &channel_key, graphql_json, now_ms());

    // With c-cid set, the channel key decrypts and no paired check fires.
    let authed = authenticate(&db, "peer-a", "ch-1", &body, &cache).expect("channel auth ok");
    assert_eq!(authed.graphql_json, graphql_json);
    assert_eq!(authed.key, channel_key.to_vec());

    // Without a cached channel key → NoChannelKey.
    let empty_cache = new_channel_key_cache();
    assert!(matches!(
        authenticate(&db, "peer-a", "ch-1", &body, &empty_cache),
        Err(AuthError::NoChannelKey)
    ));
    // Sanity: the peer's own key would NOT decrypt this body.
    let wrong = build_body(&kp, &peer_key, graphql_json, now_ms());
    assert!(matches!(
        authenticate(&db, "peer-a", "", &wrong, &cache),
        Err(AuthError::NotPaired)
    ));
}
