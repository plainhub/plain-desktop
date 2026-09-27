use super::*;
use crate::chat::db::tests::unique_tmp_dir;

fn seed_chat(db: &ChatDb, id: &str, from_id: &str, to_id: &str, channel_id: &str) {
    let mut chat = DChat::new(from_id, to_id, channel_id, "{}");
    chat.id = id.to_string();
    db.insert_chat(&chat);
}

/// Seed with explicit, distinct timestamps — `created_at` has second
/// resolution, so same-second inserts tie in the ORDER BY.
fn seed_chat_at(db: &ChatDb, id: &str, from_id: &str, to_id: &str, channel_id: &str, at: &str) {
    let mut chat = DChat::new(from_id, to_id, channel_id, "{}");
    chat.id = id.to_string();
    chat.created_at = at.to_string();
    chat.updated_at = at.to_string();
    db.insert_chat(&chat);
}

#[test]
fn get_chats_by_peer_returns_both_directions() {
    let db = ChatDb::open(&unique_tmp_dir("peer-both").join("local_chat.db")).expect("open db");
    seed_chat(&db, "a", "me", "peer1", "");
    seed_chat(&db, "b", "peer1", "me", "");
    seed_chat(&db, "c", "me", "peer2", "");
    seed_chat(&db, "d", "me", "peer1", "ch1");

    let ids: Vec<String> = db
        .get_chats_by_peer("peer1")
        .into_iter()
        .map(|c| c.id)
        .collect();
    assert_eq!(ids, vec!["a".to_string(), "b".to_string()]);
}

#[test]
fn get_chats_page_resolves_peer_and_channel_targets() {
    let db = ChatDb::open(&unique_tmp_dir("page-targets").join("local_chat.db")).expect("open db");
    seed_chat_at(&db, "a", "me", "peer1", "", "2026-01-01T00:00:01Z");
    seed_chat_at(&db, "b", "peer1", "me", "", "2026-01-01T00:00:02Z");
    seed_chat_at(&db, "c", "me", "peer2", "", "2026-01-01T00:00:03Z");
    seed_chat_at(&db, "d", "me", "", "ch1", "2026-01-01T00:00:04Z");

    // Bare peer id, `peer:`-prefixed id, and `channel:`-prefixed id;
    // pages come back oldest-first.
    let ids: Vec<String> = db
        .get_chats_page("peer1", "", 0, 50)
        .into_iter()
        .map(|c| c.id)
        .collect();
    assert_eq!(ids, vec!["a".to_string(), "b".to_string()]);
    let ids: Vec<String> = db
        .get_chats_page("peer:peer1", "", 0, 50)
        .into_iter()
        .map(|c| c.id)
        .collect();
    assert_eq!(ids, vec!["a".to_string(), "b".to_string()]);
    let ids: Vec<String> = db
        .get_chats_page("channel:ch1", "", 0, 50)
        .into_iter()
        .map(|c| c.id)
        .collect();
    assert_eq!(ids, vec!["d".to_string()]);
    // Offset pages within one conversation.
    let ids: Vec<String> = db
        .get_chats_page("peer1", "", 1, 1)
        .into_iter()
        .map(|c| c.id)
        .collect();
    assert_eq!(ids, vec!["a".to_string()]);
    // Text filter narrows by content substring.
    let db2 = ChatDb::open(&unique_tmp_dir("page-text").join("local_chat.db")).expect("open db");
    let mut chat = DChat::new(
        "me",
        "peer1",
        "",
        r#"{"type":"TEXT","value":{"text":"hello world"}}"#,
    );
    chat.id = "x".to_string();
    db2.insert_chat(&chat);
    let ids: Vec<String> = db2
        .get_chats_page("peer1", "hello", 0, 50)
        .into_iter()
        .map(|c| c.id)
        .collect();
    assert_eq!(ids, vec!["x".to_string()]);
    let ids: Vec<String> = db2
        .get_chats_page("peer1", "nope", 0, 50)
        .into_iter()
        .map(|c| c.id)
        .collect();
    assert!(ids.is_empty());
}

#[test]
fn delete_chats_by_peer_preserves_channel_chats() {
    let db = ChatDb::open(&unique_tmp_dir("peer-del").join("local_chat.db")).expect("open db");
    seed_chat(&db, "a", "me", "peer1", "");
    seed_chat(&db, "b", "peer1", "me", "");
    seed_chat(&db, "c", "me", "peer1", "ch1");

    db.delete_chats_by_peer("peer1");

    assert!(db.get_chat_by_id("a").is_none());
    assert!(db.get_chat_by_id("b").is_none());
    assert!(db.get_chat_by_id("c").is_some());
}

#[test]
fn delete_chats_by_ids_removes_only_listed_ids() {
    let db = ChatDb::open(&unique_tmp_dir("ids-del").join("local_chat.db")).expect("open db");
    seed_chat(&db, "a", "me", "p", "");
    seed_chat(&db, "b", "me", "p", "");
    seed_chat(&db, "c", "me", "p", "");

    db.delete_chats_by_ids(&["a".to_string(), "c".to_string()]);

    assert!(db.get_chat_by_id("a").is_none());
    assert!(db.get_chat_by_id("b").is_some());
    assert!(db.get_chat_by_id("c").is_none());
}

#[test]
fn delete_chats_by_ids_noop_for_empty_list() {
    let db = ChatDb::open(&unique_tmp_dir("ids-empty").join("local_chat.db")).expect("open db");
    seed_chat(&db, "a", "me", "p", "");
    db.delete_chats_by_ids(&[]);
    assert!(db.get_chat_by_id("a").is_some());
}

#[test]
fn get_chats_by_channel_returns_only_that_channel() {
    let db = ChatDb::open(&unique_tmp_dir("chan-get").join("local_chat.db")).expect("open db");
    seed_chat(&db, "a", "me", "", "ch1");
    seed_chat(&db, "b", "me", "", "ch2");
    seed_chat(&db, "c", "me", "", "ch1");

    let ids: Vec<String> = db
        .get_chats_by_channel("ch1")
        .into_iter()
        .map(|c| c.id)
        .collect();
    assert_eq!(ids, vec!["a".to_string(), "c".to_string()]);
}

#[test]
fn get_all_latest_chats_returns_local_chat() {
    let db = ChatDb::open(&unique_tmp_dir("latest-local").join("local_chat.db")).expect("open db");
    seed_chat(&db, "a", "me", "local", "");

    let latest = db.get_all_latest_chats();
    assert_eq!(
        latest.len(),
        1,
        "should return 1 latest chat, got {latest:?}"
    );
    assert_eq!(latest[0].from_id, "me");
    assert_eq!(latest[0].to_id, "local");
}

#[test]
fn get_all_latest_chats_returns_peer_and_channel() {
    let db = ChatDb::open(&unique_tmp_dir("latest-mixed").join("local_chat.db")).expect("open db");
    seed_chat(&db, "a", "me", "local", "");
    seed_chat(&db, "b", "me", "peer1", "");
    seed_chat(&db, "c", "peer1", "me", "");
    seed_chat(&db, "d", "me", "", "ch1");

    let latest = db.get_all_latest_chats();
    // 4 conversations: me↔local, me→peer1, peer1→me, ch1
    assert_eq!(
        latest.len(),
        4,
        "should return 4 latest chats, got {latest:?}"
    );
}

#[test]
fn get_all_latest_chats_returns_empty_when_no_chats() {
    let db = ChatDb::open(&unique_tmp_dir("latest-empty").join("local_chat.db")).expect("open db");
    let latest = db.get_all_latest_chats();
    assert!(latest.is_empty());
}
