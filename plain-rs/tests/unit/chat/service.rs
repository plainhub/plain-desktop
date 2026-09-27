use super::*;
use crate::chat::db::{ChatDb, DChat};
use crate::xchacha_decrypt;
use crate::{base64_decode, base64_encode};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_tmp_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id();
    std::env::temp_dir().join(format!("plain-rs-chat-svc-{label}-{pid}-{nanos}"))
}

fn seed(db: &ChatDb, id: &str, from_id: &str, to_id: &str, channel_id: &str) {
    let mut chat = DChat::new(from_id, to_id, channel_id, "{}");
    chat.id = id.to_string();
    db.insert_chat(&chat);
}

#[test]
fn resolve_ids_query_returns_listed_ids() {
    let db = ChatDb::open(&unique_tmp_dir("ids").join("local_chat.db")).expect("open db");
    seed(&db, "a", "me", "p", "");
    seed(&db, "b", "me", "p", "");

    let ids = resolve_chat_ids(&db, "ids:a,b");
    assert_eq!(ids, vec!["a".to_string(), "b".to_string()]);
}

#[test]
fn resolve_ids_query_trims_whitespace() {
    let db = ChatDb::open(&unique_tmp_dir("trim").join("local_chat.db")).expect("open db");
    let ids = resolve_chat_ids(&db, "ids: a , b , ");
    assert_eq!(ids, vec!["a".to_string(), "b".to_string()]);
}

#[test]
fn resolve_channel_query_returns_channel_chats() {
    let db = ChatDb::open(&unique_tmp_dir("chan").join("local_chat.db")).expect("open db");
    seed(&db, "a", "me", "", "ch1");
    seed(&db, "b", "me", "", "ch2");
    seed(&db, "c", "me", "", "ch1");

    let mut ids = resolve_chat_ids(&db, "channel:ch1");
    ids.sort();
    assert_eq!(ids, vec!["a".to_string(), "c".to_string()]);
}

#[test]
fn resolve_peer_query_returns_both_directions() {
    let db = ChatDb::open(&unique_tmp_dir("peer").join("local_chat.db")).expect("open db");
    seed(&db, "a", "me", "p1", "");
    seed(&db, "b", "p1", "me", "");
    seed(&db, "c", "me", "p2", "");

    let mut ids = resolve_chat_ids(&db, "peer:p1");
    ids.sort();
    assert_eq!(ids, vec!["a".to_string(), "b".to_string()]);
}

#[test]
fn resolve_peer_local_query_returns_local_notes() {
    let db = ChatDb::open(&unique_tmp_dir("local").join("local_chat.db")).expect("open db");
    seed(&db, "a", "me", "local", "");
    seed(&db, "b", "me", "p1", "");

    let ids = resolve_chat_ids(&db, "peer:local");
    assert_eq!(ids, vec!["a".to_string()]);
}

#[test]
fn resolve_unknown_query_returns_empty() {
    let db = ChatDb::open(&unique_tmp_dir("unknown").join("local_chat.db")).expect("open db");
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
