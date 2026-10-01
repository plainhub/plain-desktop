use super::*;
use crate::chat::enums::{ChannelStatus, ChatStatus};
use crate::db::{DChat, Db};
use crate::xchacha_decrypt;
use crate::{base64_decode, base64_encode};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

struct TestTransport;

impl crate::chat::transport::PeerTransport for TestTransport {
    async fn post<'a>(
        &'a self,
        _url: &'a str,
        _client_id: &'a str,
        _channel_id: Option<&'a str>,
        _body: &'a [u8],
    ) -> Result<Vec<u8>, String> {
        Err("offline".to_string())
    }
}

fn unique_tmp_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id();
    std::env::temp_dir().join(format!("plain-rs-chat-svc-{label}-{pid}-{nanos}"))
}

fn seed(db: &Db, id: &str, from_id: &str, to_id: &str, channel_id: &str) {
    let mut chat = DChat::new(from_id, to_id, channel_id, "{}");
    chat.id = id.to_string();
    db.insert_chat(&chat);
}

#[tokio::test]
async fn send_chat_item_accepts_bare_peer_id_and_local_target() {
    let dir = unique_tmp_dir("send-target");
    let db = Db::open(&dir.join("plain.db")).expect("open db");
    let service = ChatService::new(
        db,
        String::new(),
        std::sync::Arc::new(ChatIdentity::new("self", "Self", String::new())),
        DeviceType::Computer,
        dir,
        std::sync::Arc::new(TestTransport),
        std::sync::Arc::new(NoChatHooks),
        no_link_previews(),
    );

    let bare = service.send_chat_item("peer-id".to_string(), "{}".to_string());
    assert_eq!(bare[0].to_id, "peer-id");
    assert_eq!(bare[0].status, ChatStatus::Pending);

    let local = service.send_chat_item("peer:local".to_string(), "{}".to_string());
    assert_eq!(local[0].to_id, "local");
    assert_eq!(local[0].status, ChatStatus::Sent);
}

#[test]
fn resolve_ids_query_returns_listed_ids() {
    let db = Db::open(&unique_tmp_dir("ids").join("plain.db")).expect("open db");
    seed(&db, "a", "me", "p", "");
    seed(&db, "b", "me", "p", "");

    let ids = resolve_chat_ids(&db, "ids:a,b");
    assert_eq!(ids, vec!["a".to_string(), "b".to_string()]);
}

#[test]
fn resolve_ids_query_trims_whitespace() {
    let db = Db::open(&unique_tmp_dir("trim").join("plain.db")).expect("open db");
    let ids = resolve_chat_ids(&db, "ids: a , b , ");
    assert_eq!(ids, vec!["a".to_string(), "b".to_string()]);
}

#[test]
fn resolve_channel_query_returns_channel_chats() {
    let db = Db::open(&unique_tmp_dir("chan").join("plain.db")).expect("open db");
    seed(&db, "a", "me", "", "ch1");
    seed(&db, "b", "me", "", "ch2");
    seed(&db, "c", "me", "", "ch1");

    let mut ids = resolve_chat_ids(&db, "channel:ch1");
    ids.sort();
    assert_eq!(ids, vec!["a".to_string(), "c".to_string()]);
}

#[test]
fn resolve_peer_query_returns_both_directions() {
    let db = Db::open(&unique_tmp_dir("peer").join("plain.db")).expect("open db");
    seed(&db, "a", "me", "p1", "");
    seed(&db, "b", "p1", "me", "");
    seed(&db, "c", "me", "p2", "");

    let mut ids = resolve_chat_ids(&db, "peer:p1");
    ids.sort();
    assert_eq!(ids, vec!["a".to_string(), "b".to_string()]);
}

#[test]
fn resolve_peer_local_query_returns_local_notes() {
    let db = Db::open(&unique_tmp_dir("local").join("plain.db")).expect("open db");
    seed(&db, "a", "me", "local", "");
    seed(&db, "b", "me", "p1", "");

    let ids = resolve_chat_ids(&db, "peer:local");
    assert_eq!(ids, vec!["a".to_string()]);
}

#[test]
fn resolve_unknown_query_returns_empty() {
    let db = Db::open(&unique_tmp_dir("unknown").join("plain.db")).expect("open db");
    seed(&db, "a", "me", "p", "");

    assert!(resolve_chat_ids(&db, "unknown:foo").is_empty());
    assert!(resolve_chat_ids(&db, "").is_empty());
    assert!(resolve_chat_ids(&db, "nocolon").is_empty());
}

#[test]
fn to_peer_content_converts_fid_to_fsid() {
    let token_raw = [99u8; 32];
    let token = base64_encode(&token_raw);
    let content = serde_json::json!({
        "type": "images",
        "value": {
            "items": [
                {"uri": "fid:abcdef0123456789.jpg", "fileName": "cat.jpg", "size": 1234}
            ]
        }
    })
    .to_string();

    let peer_content = to_peer_content(&content, &token);
    let v: Value = serde_json::from_str(&peer_content).unwrap();
    let uri = v["value"]["items"][0]["uri"].as_str().unwrap();
    assert!(
        uri.starts_with("fsid:"),
        "uri should be fsid: prefix, got: {uri}"
    );

    // The encrypted part (after fsid:) must round-trip through
    // xchacha_decrypt to the original fid: URI.
    let encrypted_b64 = uri.strip_prefix("fsid:").unwrap();
    let encrypted = base64_decode(encrypted_b64);
    let plaintext = xchacha_decrypt(&token, &encrypted).expect("must decrypt");
    let plaintext_str = std::str::from_utf8(&plaintext).unwrap();
    assert_eq!(plaintext_str, "fid:abcdef0123456789.jpg");
}

#[test]
fn to_peer_content_preserves_non_fid_uris() {
    let token = base64_encode(&[1u8; 32]);
    let content = serde_json::json!({
        "type": "files",
        "value": {
            "items": [
                {"uri": "https://example.com/file.pdf", "fileName": "doc.pdf", "size": 5678}
            ]
        }
    })
    .to_string();

    let peer_content = to_peer_content(&content, &token);
    let v: Value = serde_json::from_str(&peer_content).unwrap();
    let uri = v["value"]["items"][0]["uri"].as_str().unwrap();
    assert_eq!(uri, "https://example.com/file.pdf");
}

#[test]
fn to_peer_content_passthrough_on_invalid_json() {
    let token = base64_encode(&[1u8; 32]);
    let content = "not json at all";
    assert_eq!(to_peer_content(content, &token), content);
}

#[tokio::test]
async fn kicked_channel_rejects_incoming_message() {
    let dir = unique_tmp_dir("kicked-channel");
    let db = Db::open(&dir.join("plain.db")).expect("open db");
    let service = ChatService::new(
        db,
        String::new(),
        std::sync::Arc::new(ChatIdentity::new("self", "Self", String::new())),
        DeviceType::Computer,
        dir,
        std::sync::Arc::new(TestTransport),
        std::sync::Arc::new(NoChatHooks),
        no_link_previews(),
    );
    let mut channel = crate::db::DChannel::new("group", "self");
    channel.status = ChannelStatus::Kicked;
    service.db.insert_channel(&channel);

    assert_eq!(
        service
            .receive_peer_chat("peer", &channel.id, "{}")
            .unwrap_err(),
        "Channel not joined"
    );
    assert!(service.db.get_chats_by_channel(&channel.id).is_empty());
    assert_eq!(
        service
            .receive_peer_chat("peer", "missing", "{}")
            .unwrap_err(),
        "Unknown channel"
    );
}

#[tokio::test]
async fn unpaired_peer_send_fails_and_local_cache_tracks_deletion() {
    let dir = unique_tmp_dir("unpaired-send");
    let db = Db::open(&dir.join("plain.db")).expect("open db");
    let peer = crate::db::DPeer::new("peer", "Peer", "127.0.0.1", 8443, DeviceType::Phone);
    db.upsert_peer(&peer);
    let service = ChatService::new(
        db,
        String::new(),
        std::sync::Arc::new(ChatIdentity::new("self", "Self", String::new())),
        DeviceType::Computer,
        dir,
        std::sync::Arc::new(TestTransport),
        std::sync::Arc::new(NoChatHooks),
        no_link_previews(),
    );

    let sent = service.send_chat_item("peer:peer".to_string(), "{}".to_string());
    assert_eq!(
        service.db.get_chat_by_id(&sent[0].id).unwrap().status,
        ChatStatus::Failed
    );

    let local = service.send_chat_item("peer:local".to_string(), "{}".to_string());
    assert_eq!(
        service.cacher.get_latest_chat("local").unwrap().id,
        local[0].id
    );
    assert!(service.delete_chat_item(local[0].id.clone()));
    assert!(service.cacher.get_latest_chat("local").is_none());
}
