//! Unit tests for `src/sqlite_browse.rs` — the shared SQLite browsing
//! core behind the /developer/database pages. In-memory database, no
//! filesystem, no timing.
use super::*;

fn conn() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE items (
            id   TEXT PRIMARY KEY,
            n    INTEGER NOT NULL DEFAULT 0,
            f    REAL,
            t    TEXT NOT NULL DEFAULT '',
            b    BLOB
        );
        CREATE TABLE pair (
            x TEXT NOT NULL,
            y TEXT NOT NULL,
            PRIMARY KEY (y, x)
        );
        CREATE TABLE no_pk (z TEXT);
        CREATE TABLE auto_inc (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            v  TEXT
        );",
    )
    .unwrap();
    conn
}

/// One seeded items row: (id, n, f, t, b).
type ItemRow<'a> = (&'a str, i64, Option<f64>, &'a str, Option<&'a [u8]>);

fn insert_items(conn: &Connection, rows: &[ItemRow]) {
    for (id, n, f, t, b) in rows {
        conn.execute(
            "INSERT INTO items(id, n, f, t, b) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![id, n, f, t, b],
        )
        .unwrap();
    }
}

#[test]
fn is_safe_identifier_accepts_plain_identifiers_only() {
    for ok in ["items", "_x", "a_1", "X9"] {
        assert!(is_safe_identifier(ok), "{ok}");
    }
    for bad in [
        "",
        "1x",
        "has space",
        "items; DROP TABLE items",
        "a-b",
        "t.x",
        "héllo",
        "id`--",
    ] {
        assert!(!is_safe_identifier(bad), "{bad}");
    }
}

#[test]
fn tables_lists_user_tables_sorted_without_internals() {
    let conn = conn();
    // auto_inc's AUTOINCREMENT forces the internal sqlite_sequence table
    // to exist — it must not be listed.
    conn.execute("INSERT INTO auto_inc(v) VALUES ('a')", [])
        .unwrap();
    assert_eq!(tables(&conn), vec!["auto_inc", "items", "no_pk", "pair"]);
}

#[test]
fn table_exists_answers_only_for_real_user_tables() {
    let conn = conn();
    assert!(table_exists(&conn, "items"));
    assert!(!table_exists(&conn, "sqlite_sequence"));
    assert!(!table_exists(&conn, "nope"));
    assert!(!table_exists(&conn, "items; DROP TABLE items"));
}

#[test]
fn row_count_counts_and_rejects_unsafe_names() {
    let conn = conn();
    insert_items(
        &conn,
        &[("a", 1, None, "x", None), ("b", 2, None, "y", None)],
    );
    assert_eq!(row_count(&conn, "items").unwrap(), 2);
    assert!(matches!(
        row_count(&conn, "items; DROP TABLE items").unwrap_err(),
        SqliteBrowseError::Invalid(_)
    ));
}

#[test]
fn rows_page_maps_sqlite_values_onto_json() {
    let conn = conn();
    insert_items(&conn, &[("a", 7, Some(1.5), "hi", Some(&[0xCA, 0xFE][..]))]);
    let rows = rows_page(&conn, "items", 0, 10).unwrap();
    assert_eq!(rows.len(), 1);
    let v: serde_json::Value = serde_json::from_str(&rows[0]).unwrap();
    assert_eq!(v["id"], serde_json::json!("a"));
    assert_eq!(v["n"], serde_json::json!(7));
    assert_eq!(v["f"], serde_json::json!(1.5));
    assert_eq!(v["t"], serde_json::json!("hi"));
    assert_eq!(v["b"], serde_json::json!("cafe"));

    // NULL columns map to JSON null (f and b of another row).
    insert_items(&conn, &[("b", 0, None, "", None)]);
    let rows = rows_page(&conn, "items", 1, 1).unwrap();
    let v: serde_json::Value = serde_json::from_str(&rows[0]).unwrap();
    assert_eq!(v["id"], serde_json::json!("b"));
    assert!(v["f"].is_null());
    assert!(v["b"].is_null());
}

#[test]
fn rows_page_pages_by_offset_and_clamps_limit() {
    let conn = conn();
    let mut batch = String::new();
    for i in 0..1005 {
        batch.push_str(&format!(
            "INSERT INTO items(id, t) VALUES ('r{i:04}', 'v');"
        ));
    }
    conn.execute_batch(&batch).unwrap();

    let first = rows_page(&conn, "items", 0, 2).unwrap();
    assert_eq!(first.len(), 2);
    let third = rows_page(&conn, "items", 2, 2).unwrap();
    assert_eq!(third.len(), 2);
    assert_ne!(first[0], third[0]);

    // A runaway limit is clamped to MAX_PAGE_LIMIT (1000), not honoured.
    assert_eq!(rows_page(&conn, "items", 0, 100_000).unwrap().len(), 1000);
    // Negative offset floors at 0.
    assert_eq!(rows_page(&conn, "items", -5, 1).unwrap().len(), 1);
}

#[test]
fn primary_key_column_handles_single_composite_and_missing() {
    let conn = conn();
    assert_eq!(primary_key_column(&conn, "items").as_deref(), Some("id"));
    // PRIMARY KEY (y, x): y is the first key column even though x is
    // declared first.
    assert_eq!(primary_key_column(&conn, "pair").as_deref(), Some("y"));
    assert_eq!(primary_key_column(&conn, "no_pk"), None);
    assert_eq!(primary_key_column(&conn, "nope"), None);
    assert_eq!(primary_key_column(&conn, "items; DROP TABLE items"), None);
}

#[test]
fn table_columns_reports_pragma_metadata() {
    let conn = conn();
    let cols = table_columns(&conn, "items");
    let names: Vec<(&str, bool, bool)> = cols
        .iter()
        .map(|c| (c.name.as_str(), c.not_null, c.primary_key))
        .collect();
    assert_eq!(
        names,
        vec![
            ("id", false, true),
            ("n", true, false),
            ("f", false, false),
            ("t", true, false),
            ("b", false, false)
        ]
    );
    assert_eq!(cols[1].data_type, "INTEGER");
    assert_eq!(cols[1].default_value.as_deref(), Some("0"));
    assert_eq!(cols[0].default_value, None);
    assert_eq!(cols[2].data_type, "REAL");
    assert!(table_columns(&conn, "nope").is_empty());
    assert!(table_columns(&conn, "items; DROP TABLE items").is_empty());
}

#[test]
fn column_type_of_maps_the_standard_set() {
    assert_eq!(column_type_of("TEXT"), SqliteColumnType::Text);
    assert_eq!(column_type_of("integer"), SqliteColumnType::Integer);
    assert_eq!(column_type_of("REAL"), SqliteColumnType::Real);
    assert_eq!(column_type_of("BLOB"), SqliteColumnType::Blob);
    assert_eq!(column_type_of("NUMERIC"), SqliteColumnType::Numeric);
    assert_eq!(column_type_of(""), SqliteColumnType::Unknown);
    assert_eq!(column_type_of("VARCHAR(10)"), SqliteColumnType::Unknown);
}

#[test]
fn delete_rows_removes_matching_ids_and_validates() {
    let conn = conn();
    insert_items(
        &conn,
        &[
            ("a", 1, None, "x", None),
            ("b", 2, None, "y", None),
            ("c", 3, None, "z", None),
        ],
    );
    assert_eq!(delete_rows(&conn, "items", "id", &["b".into()]).unwrap(), 1);
    assert_eq!(row_count(&conn, "items").unwrap(), 2);
    assert_eq!(
        delete_rows(
            &conn,
            "items",
            "id",
            &["a".into(), "c".into(), "gone".into()]
        )
        .unwrap(),
        2
    );
    assert_eq!(row_count(&conn, "items").unwrap(), 0);

    // Empty ids and unsafe table/id names are rejected before any SQL.
    assert!(matches!(
        delete_rows(&conn, "items", "id", &[]).unwrap_err(),
        SqliteBrowseError::Invalid(_)
    ));
    assert!(matches!(
        delete_rows(&conn, "items; DROP TABLE items", "id", &["a".into()]).unwrap_err(),
        SqliteBrowseError::Invalid(_)
    ));
    assert!(matches!(
        delete_rows(&conn, "items", "id; DROP TABLE items", &["a".into()]).unwrap_err(),
        SqliteBrowseError::Invalid(_)
    ));
}

#[test]
fn insert_row_binds_json_object_as_text_with_null() {
    let conn = conn();
    let mut row = serde_json::Map::new();
    row.insert("id".into(), serde_json::json!("a"));
    row.insert("n".into(), serde_json::json!(7));
    row.insert("f".into(), serde_json::Value::Null);
    assert_eq!(insert_row(&conn, "items", &row).unwrap(), 1);

    let v: serde_json::Value =
        serde_json::from_str(&rows_page(&conn, "items", 0, 1).unwrap()[0]).unwrap();
    assert_eq!(v["id"], serde_json::json!("a"));
    assert_eq!(v["n"], serde_json::json!(7)); // INTEGER affinity coerced the text bind
    assert!(v["f"].is_null());

    // Empty row / unsafe table / unsafe column key are rejected.
    assert!(matches!(
        insert_row(&conn, "items", &serde_json::Map::new()).unwrap_err(),
        SqliteBrowseError::Invalid(_)
    ));
    assert!(matches!(
        insert_row(&conn, "items; DROP TABLE items", &row).unwrap_err(),
        SqliteBrowseError::Invalid(_)
    ));
    let mut bad = serde_json::Map::new();
    bad.insert("id; DROP TABLE items".into(), serde_json::json!("a"));
    assert!(matches!(
        insert_row(&conn, "items", &bad).unwrap_err(),
        SqliteBrowseError::Invalid(_)
    ));
}
