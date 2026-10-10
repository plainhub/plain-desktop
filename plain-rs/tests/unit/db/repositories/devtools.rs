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

fn dbs(tag: &str) -> Db {
    let dir = tmp_dir(tag);
    Db::open(&dir.join("plain.db")).unwrap()
}

#[test]
fn tables_lists_shared_database_bare_and_sorted() {
    let db = dbs("tables");
    let tables = tables(&db);

    let mut sorted = tables.clone();
    sorted.sort();
    assert_eq!(tables, sorted);
    for t in &tables {
        assert!(
            !t.starts_with("chat.") && !t.starts_with("library."),
            "{t} must not carry a store prefix"
        );
    }
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
    for gone in ["event", "media", "session"] {
        assert!(!tables.contains(&gone.to_string()), "{gone} is not a table");
    }
}

#[test]
fn rows_count_and_id_key_use_shared_database() {
    let db = dbs("route");
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO chats(id, from_id, to_id, channel_id, content, created_at, updated_at) VALUES ('c1', 'a', 'b', '', '{}', '2026', '2026')",
            [],
        )
        .unwrap();
    });
    db.with_conn(|conn| {
        conn.execute_batch(
            "INSERT INTO tags(id, type, name, count, created_at, updated_at) VALUES ('t1', 1, 'rock', 0, '2026', '2026');
                 INSERT INTO tags(id, type, name, count, created_at, updated_at) VALUES ('t2', 2, 'jazz', 0, '2026', '2026');",
        )
        .unwrap();
    });

    assert_eq!(table_row_count(&db, "chats").unwrap(), 1);
    assert_eq!(table_id_key(&db, "chats").unwrap(), "id".to_string());
    let rows = table_rows(&db, "chats", 0, 10).unwrap();
    let v: serde_json::Value = serde_json::from_str(&rows[0]).unwrap();
    assert_eq!(v["id"], serde_json::json!("c1"));

    assert_eq!(table_row_count(&db, "tags").unwrap(), 2);
    assert_eq!(table_id_key(&db, "tags").unwrap(), "id".to_string());
    assert_eq!(table_rows(&db, "tags", 1, 1).unwrap().len(), 1);
    let cols = table_columns(&db, "tags").unwrap();
    assert_eq!(
        cols.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
        vec!["id", "name", "type", "count", "created_at", "updated_at"]
    );
}

#[test]
fn composite_primary_key_reports_first_key_column() {
    let db = dbs("composite");
    assert_eq!(
        table_id_key(&db, "tag_relations").unwrap(),
        "tag_id".to_string()
    );
    assert_eq!(
        table_id_key(&db, "favorite_folders").unwrap(),
        "root_path".to_string()
    );
}

#[test]
fn prefixed_unknown_and_unsafe_table_names_are_rejected() {
    let db = dbs("reject");
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
        assert!(table_row_count(&db, bad).is_err(), "{bad}");
        assert!(table_rows(&db, bad, 0, 10).is_err(), "{bad}");
        assert!(table_id_key(&db, bad).is_err(), "{bad}");
        assert!(table_columns(&db, bad).is_err(), "{bad}");
        assert!(
            delete_table_rows(&db, bad, &["x".to_string()]).is_err(),
            "{bad}"
        );
    }
}

#[test]
fn delete_removes_only_named_rows() {
    let db = dbs("delete");
    db.with_conn(|conn| {
        conn.execute_batch(
            "INSERT INTO tags(id, type, name, count, created_at, updated_at) VALUES ('t1', 1, 'a', 0, '2026', '2026');
                 INSERT INTO tags(id, type, name, count, created_at, updated_at) VALUES ('t2', 2, 'b', 0, '2026', '2026');
                 INSERT INTO tags(id, type, name, count, created_at, updated_at) VALUES ('t3', 3, 'c', 0, '2026', '2026');",
        )
        .unwrap();
    });

    assert_eq!(
        delete_table_rows(&db, "tags", &["t2".to_string()]).unwrap(),
        1
    );
    assert_eq!(table_row_count(&db, "tags").unwrap(), 2);
    assert_eq!(table_row_count(&db, "chats").unwrap(), 0);

    assert!(delete_table_rows(&db, "tags", &[]).is_err());
}
