use super::*;

#[cfg(all(feature = "chat", feature = "library"))]
#[test]
fn one_connection_has_chat_and_library_tables() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("plain.db")).unwrap();
    let clone = db.clone();
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO chats(id, from_id, to_id, channel_id, content, created_at, updated_at) \
             VALUES ('chat-1', 'me', 'peer-1', '', 'hello', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO tags(id, name, type, count, created_at, updated_at) \
             VALUES ('tag-1', 'shared', 1, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
    });
    clone.with_conn(|conn| {
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM chats", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            conn.query_row("SELECT COUNT(*) FROM tags", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
    });
}
