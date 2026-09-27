//! Unit tests for `src/library/db/mod.rs` (LibraryDb open + DDL).

#[path = "../fixtures.rs"]
mod fixtures;
use crate::library::db::LibraryDb;
use fixtures::*;

#[test]
fn open_creates_every_table_once() {
    let db = test_db("open_tables");
    let tables: Vec<String> = db.with_conn(|conn| {
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap();
        stmt.query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect()
    });
    for expected in [
        "audio_queue_source",
        "audio_queue_items",
        "audio_playlists",
        "audio_playlist_items",
        "audio_play_history",
        "tags",
        "tag_relations",
        "favorite_folders",
        "library_prefs",
    ] {
        assert!(
            tables.iter().any(|t| t == expected),
            "missing table {expected}"
        );
    }
    // Reopening the same file is fine (CREATE IF NOT EXISTS).
    let path = std::env::temp_dir().join(format!(
        "plain-rs-library-reopen2-{}.db",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    {
        let _a = LibraryDb::open(&path).unwrap();
    }
    let _b = LibraryDb::open(&path).unwrap();
    let _ = std::fs::remove_file(&path);
}

#[test]
fn source_row_defaults_to_none() {
    let db = test_db("source_default");
    let src = crate::library::db::audio_queue::get_source(&db);
    assert_eq!(
        src.source,
        crate::library::db::audio_queue::QueueSourceKind::None
    );
    assert_eq!(src.current_path, "");
    assert_eq!(src.current_index, -1);
}

#[test]
fn prefs_round_trip() {
    let db = test_db("prefs");
    assert!(crate::library::db::audio_queue::get_pref(&db, "k").is_none());
    crate::library::db::audio_queue::set_pref(&db, "k", "v1");
    crate::library::db::audio_queue::set_pref(&db, "k", "v2");
    assert_eq!(
        crate::library::db::audio_queue::get_pref(&db, "k").as_deref(),
        Some("v2")
    );
}
