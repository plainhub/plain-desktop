use super::*;

#[cfg(all(feature = "chat", feature = "library"))]
#[test]
fn one_connection_has_chat_and_library_tables() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("plain.db")).unwrap();
    let clone = db.clone();
    db.with_conn(|conn| {
        conn.execute("INSERT INTO chats(id) VALUES ('chat-1')", [])
            .unwrap();
        conn.execute("INSERT INTO tags(id, name) VALUES ('tag-1', 'shared')", [])
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
