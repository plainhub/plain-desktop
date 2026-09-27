use super::*;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn unique_tmp_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id();
    std::env::temp_dir().join(format!("plain-rs-chat-{label}-{pid}-{nanos}"))
}

#[test]
fn open_creates_missing_parent_dirs() {
    let nested = unique_tmp_dir("open-parent").join("a/b/c");
    let db_path = nested.join("local_chat.db");

    let db = ChatDb::open(&db_path).expect("should create missing parents");

    assert!(nested.exists(), "nested parent directory should be created");
    assert!(db_path.exists(), "db file should be created");

    // Second open on the same path should also succeed (reopen existing DB).
    let _reopen = ChatDb::open(&db_path).expect("should reopen existing DB");

    // Verify the wrapper actually wraps a live connection.
    db.with_conn(|_| ());
}

#[test]
fn open_works_when_db_already_exists() {
    let dir = unique_tmp_dir("open-existing");
    let db_path = dir.join("local_chat.db");

    let _ = ChatDb::open(&db_path).expect("first open");
    let _ = ChatDb::open(&db_path).expect("second open");
}

#[test]
fn open_sets_busy_timeout() {
    let dir = unique_tmp_dir("busy-timeout");
    let db = ChatDb::open(&dir.join("local_chat.db")).expect("open");

    let ms: i64 = db.with_conn(|conn| {
        conn.query_row("PRAGMA busy_timeout", [], |row| row.get(0))
            .expect("read busy_timeout pragma")
    });
    assert_eq!(ms, 5000, "concurrent access must wait instead of failing");
}

#[test]
fn primary_key_column_returns_named_pk() {
    let db_path = unique_tmp_dir("pk-named").join("local_chat.db");
    let db = ChatDb::open(&db_path).expect("open db");

    db.with_conn(|conn| {
        conn.execute_batch(
            "CREATE TABLE sessions (client_id TEXT PRIMARY KEY, token TEXT NOT NULL DEFAULT '');",
        )
        .expect("create sessions");
    });

    assert_eq!(db.primary_key_column("sessions"), "client_id");
    assert_eq!(db.primary_key_column("chats"), "id");
}

#[test]
fn primary_key_column_falls_back_for_missing_table() {
    let db_path = unique_tmp_dir("pk-fallback").join("local_chat.db");
    let db = ChatDb::open(&db_path).expect("open db");

    assert_eq!(db.primary_key_column("does_not_exist"), "id");
}

#[test]
fn table_columns_returns_declared_order() {
    let db_path = unique_tmp_dir("table-columns").join("local_chat.db");
    let db = ChatDb::open(&db_path).expect("open db");

    db.with_conn(|conn| {
        conn.execute_batch(
            "CREATE TABLE column_sample (id TEXT PRIMARY KEY, peer_id TEXT NOT NULL, updated_at INTEGER DEFAULT 0, note TEXT);",
        )
        .expect("create column_sample");
    });

    let cols = db.table_columns("column_sample");
    let names: Vec<&str> = cols.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["id", "peer_id", "updated_at", "note"]);
    let by_name = |n: &str| cols.iter().find(|c| c.name == n).expect("column");
    assert_eq!(
        (by_name("id").primary_key, by_name("id").not_null),
        (true, false)
    );
    assert_eq!(by_name("id").data_type, "TEXT");
    assert_eq!(by_name("peer_id").data_type, "TEXT");
    assert!(by_name("peer_id").not_null);
    assert!(!by_name("peer_id").primary_key);
    assert_eq!(by_name("updated_at").data_type, "INTEGER");
    assert_eq!(by_name("updated_at").default_value.as_deref(), Some("0"));
    assert_eq!(by_name("note").default_value, None);
    assert!(db.table_columns("does_not_exist").is_empty());
}

/// The five chat tables exist with the plain-app column sets — the DB
/// design alignment lock.
#[test]
fn chat_tables_match_plain_app_schema() {
    let db_path = unique_tmp_dir("schema").join("local_chat.db");
    let db = ChatDb::open(&db_path).expect("open db");

    let expect: &[(&str, &[&str])] = &[
        (
            "chats",
            &[
                "id",
                "from_id",
                "to_id",
                "channel_id",
                "content",
                "status",
                "status_data",
                "created_at",
                "updated_at",
            ],
        ),
        (
            "chat_channels",
            &[
                "id",
                "name",
                "owner_id",
                "members",
                "key",
                "version",
                "status",
                "created_at",
                "updated_at",
            ],
        ),
        (
            "peers",
            &[
                "id",
                "name",
                "ip",
                "key",
                "public_key",
                "status",
                "port",
                "device_type",
                "token",
                "created_at",
                "updated_at",
            ],
        ),
        (
            "nearby_device_cache",
            &[
                "id",
                "name",
                "ips",
                "port",
                "device_type",
                "version",
                "platform",
                "last_seen",
            ],
        ),
        (
            "app_files",
            &[
                "id",
                "size",
                "mime_type",
                "real_path",
                "ref_count",
                "weak_hash",
                "created_at",
                "updated_at",
            ],
        ),
    ];
    for (table, cols) in expect {
        let names: Vec<String> = db
            .table_columns(table)
            .into_iter()
            .map(|c| c.name)
            .collect();
        assert_eq!(
            names.iter().map(String::as_str).collect::<Vec<_>>(),
            *cols,
            "table {table} columns"
        );
    }
}
