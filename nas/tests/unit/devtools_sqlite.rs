//! Unit tests for `src/devtools_sqlite.rs` — the two-store dispatch
//! behind `/developer/database`. The browsing primitives themselves
//! (identifier guards, JSON rows, PRAGMA metadata, delete) are locked in
//! plain-rs's `tests/unit/sqlite_browse.rs`; these lock the NAS-side
//! routing: bare table names (no store prefix), unknown/unsafe table
//! rejection, and per-store results.
use super::*;

fn tmp_dir(tag: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("plain-nas-devtools-{tag}-{nanos}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn dbs(tag: &str) -> (ChatDb, LibraryDb) {
    let dir = tmp_dir(tag);
    (
        ChatDb::open(&dir.join("chat.db")).unwrap(),
        LibraryDb::open(&dir.join("library.db")).unwrap(),
    )
}

#[test]
fn tables_lists_both_stores_bare_and_sorted() {
    let (chat, library) = dbs("tables");
    let tables = tables(&chat, &library);

    let mut sorted = tables.clone();
    sorted.sort();
    assert_eq!(tables, sorted);
    for t in &tables {
        assert!(
            !t.starts_with("chat.") && !t.starts_with("library."),
            "{t} must not carry a store prefix"
        );
    }
    // Both stores' schema tables are reachable by bare name.
    for expected in [
        "chats",
        "chat_channels",
        "bookmarks",
        "tags",
        "audio_playlists",
        "favorite_folders",
    ] {
        assert!(
            tables.contains(&expected.to_string()),
            "{expected} in {tables:?}"
        );
    }
    // The fjall KV namespaces are not tables here.
    for gone in ["event", "media", "session"] {
        assert!(!tables.contains(&gone.to_string()), "{gone} is not a table");
    }
}

#[test]
fn rows_count_and_id_key_route_to_the_owning_store() {
    let (chat, library) = dbs("route");
    chat.with_conn(|conn| {
        conn.execute(
            "INSERT INTO chats(id, from_id, to_id, created_at) VALUES ('c1', 'a', 'b', '2026')",
            [],
        )
        .unwrap();
    });
    library.with_conn(|conn| {
        conn.execute_batch(
            "INSERT INTO tags(id, type, name) VALUES ('t1', 1, 'rock');
                 INSERT INTO tags(id, type, name) VALUES ('t2', 2, 'jazz');",
        )
        .unwrap();
    });

    // chat.db table: one row, idKey = chats.id.
    assert_eq!(table_row_count(&chat, &library, "chats").unwrap(), 1);
    assert_eq!(
        table_id_key(&chat, &library, "chats").unwrap(),
        "id".to_string()
    );
    let rows = table_rows(&chat, &library, "chats", 0, 10).unwrap();
    let v: serde_json::Value = serde_json::from_str(&rows[0]).unwrap();
    assert_eq!(v["id"], serde_json::json!("c1"));

    // library.db table: two rows, idKey = tags.id, page window works.
    assert_eq!(table_row_count(&chat, &library, "tags").unwrap(), 2);
    assert_eq!(
        table_id_key(&chat, &library, "tags").unwrap(),
        "id".to_string()
    );
    assert_eq!(
        table_rows(&chat, &library, "tags", 1, 1).unwrap().len(),
        1
    );
    let cols = table_columns(&chat, &library, "tags").unwrap();
    assert_eq!(
        cols.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
        vec!["id", "type", "name"]
    );
}

#[test]
fn composite_primary_key_reports_first_key_column() {
    let (chat, library) = dbs("composite");
    assert_eq!(
        table_id_key(&chat, &library, "tag_relations").unwrap(),
        "tag_id".to_string()
    );
    assert_eq!(
        table_id_key(&chat, &library, "favorite_folders").unwrap(),
        "root_path".to_string()
    );
}

#[test]
fn prefixed_unknown_and_unsafe_table_names_are_rejected() {
    let (chat, library) = dbs("reject");
    // Prefixed (old scheme), missing, and SQL-injection shapes all fail
    // the same way.
    for bad in [
        "media",
        "event:",
        "media:foo",
        "chat.chats",
        "library.tags",
        "nope",
        "sqlite_master",
        "tags; DROP TABLE tags",
        "chats --",
    ] {
        assert!(table_row_count(&chat, &library, bad).is_err(), "{bad}");
        assert!(table_rows(&chat, &library, bad, 0, 10).is_err(), "{bad}");
        assert!(table_id_key(&chat, &library, bad).is_err(), "{bad}");
        assert!(table_columns(&chat, &library, bad).is_err(), "{bad}");
        assert!(
            delete_table_rows(&chat, &library, bad, &["x".to_string()]).is_err(),
            "{bad}"
        );
    }
}

#[test]
fn ambiguous_table_name_in_both_stores_is_rejected() {
    let (chat, library) = dbs("ambiguous");
    library.with_conn(|conn| {
        conn.execute_batch("CREATE TABLE chats(id TEXT PRIMARY KEY);").unwrap();
        conn.execute("INSERT INTO chats(id) VALUES ('x')", []).unwrap();
    });
    let err = table_row_count(&chat, &library, "chats").unwrap_err();
    assert!(err.to_string().contains("ambiguous"), "{err}");
}

#[test]
fn delete_removes_only_named_rows_in_the_owning_store() {
    let (chat, library) = dbs("delete");
    library.with_conn(|conn| {
        conn.execute_batch(
            "INSERT INTO tags(id, type, name) VALUES ('t1', 1, 'a');
                 INSERT INTO tags(id, type, name) VALUES ('t2', 2, 'b');
                 INSERT INTO tags(id, type, name) VALUES ('t3', 3, 'c');",
        )
        .unwrap();
    });

    assert_eq!(
        delete_table_rows(&chat, &library, "tags", &["t2".to_string()]).unwrap(),
        1
    );
    assert_eq!(table_row_count(&chat, &library, "tags").unwrap(), 2);
    // chat.db stays untouched by a library-store delete.
    assert_eq!(table_row_count(&chat, &library, "chats").unwrap(), 0);

    // Empty ids rejected.
    assert!(delete_table_rows(&chat, &library, "tags", &[]).is_err());
}
