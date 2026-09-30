use super::*;

#[cfg(all(feature = "chat", feature = "library"))]
#[test]
fn one_connection_has_chat_and_library_tables() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("shared.db")).unwrap();
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

#[cfg(feature = "library")]
#[test]
fn imports_legacy_library_once() {
    let dir = tempfile::tempdir().unwrap();
    let old_path = dir.path().join("library.db");
    let old = Connection::open(&old_path).unwrap();
    old.execute_batch(
        "CREATE TABLE tags(id TEXT PRIMARY KEY, type INTEGER NOT NULL, name TEXT NOT NULL);
        INSERT INTO tags VALUES ('old', 1, 'legacy');
        CREATE TABLE notes(id TEXT PRIMARY KEY, title TEXT NOT NULL, content TEXT NOT NULL,
            deleted_at TEXT, created_at TEXT NOT NULL, updated_at TEXT NOT NULL);
        INSERT INTO notes VALUES ('note-1', 'Old note', 'body', NULL, '2026', '2026');",
    )
    .unwrap();
    drop(old);
    let db = Db::open(&dir.path().join("shared.db")).unwrap();
    db.import_legacy_library(&old_path).unwrap();
    db.with_conn(|conn| {
        assert_eq!(
            conn.query_row("SELECT name FROM tags WHERE id='old'", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "legacy"
        );
        assert_eq!(
            conn.query_row("SELECT content FROM notes WHERE id='note-1'", [], |r| {
                r.get::<_, String>(0)
            })
            .unwrap(),
            "body"
        );
        conn.execute("DELETE FROM tags WHERE id='old'", []).unwrap();
    });
    db.import_legacy_library(&old_path).unwrap();
    assert_eq!(
        db.with_conn(|conn| conn
            .query_row("SELECT COUNT(*) FROM tags WHERE id='old'", [], |r| r
                .get::<_, i64>(0))
            .unwrap()),
        0
    );
}
