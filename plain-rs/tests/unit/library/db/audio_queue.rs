//! Unit tests for `src/library/db/audio_queue.rs` (row IO layer).

#[path = "../fixtures.rs"]
mod fixtures;
use fixtures::*;

#[test]
fn replace_queue_items_assigns_dense_sort_orders() {
    let db = test_db("replace_dense");
    crate::library::db::audio_queue::replace_queue_items(
        &db,
        &[
            crate::library::db::QueueItem {
                path: "/a.mp3".into(),
                sort_order: 99,
                title: "A".into(),
                artist: String::new(),
                duration_secs: 1,
            },
            crate::library::db::QueueItem {
                path: "/b.mp3".into(),
                sort_order: -5,
                title: "B".into(),
                artist: String::new(),
                duration_secs: 2,
            },
        ],
    );
    let items = crate::library::db::audio_queue::all_queue_items(&db);
    assert_eq!(items[0].sort_order, 0);
    assert_eq!(items[1].sort_order, 1);

    // Empty slice clears the table.
    crate::library::db::audio_queue::replace_queue_items(&db, &[]);
    assert!(crate::library::db::audio_queue::all_queue_items(&db).is_empty());
}

#[test]
fn history_upsert_and_trim_are_deterministic() {
    let db = test_db("history_io");
    let mk = |path: &str, played_at: &str| crate::library::db::PlayHistory {
        path: path.into(),
        title: "T".into(),
        artist: String::new(),
        duration_secs: 1,
        play_count: 1,
        played_at: played_at.into(),
    };
    crate::library::db::audio_queue::upsert_history(
        &db,
        &mk("/old.mp3", "2026-01-01T00:00:00.000Z"),
    );
    crate::library::db::audio_queue::upsert_history(
        &db,
        &mk("/new.mp3", "2026-01-02T00:00:00.000Z"),
    );
    // Same millisecond tie: rowid (insertion order) breaks it.
    crate::library::db::audio_queue::upsert_history(
        &db,
        &mk("/tie1.mp3", "2026-01-02T00:00:00.000Z"),
    );
    crate::library::db::audio_queue::upsert_history(
        &db,
        &mk("/tie2.mp3", "2026-01-02T00:00:00.000Z"),
    );
    let all = crate::library::db::audio_queue::all_history(&db);
    assert_eq!(
        all.iter().map(|h| h.path.as_str()).collect::<Vec<_>>(),
        ["/tie2.mp3", "/tie1.mp3", "/new.mp3", "/old.mp3"]
    );
    // Upsert keeps the row, bumps nothing by itself (caller sets fields).
    crate::library::db::audio_queue::upsert_history(
        &db,
        &mk("/old.mp3", "2026-01-03T00:00:00.000Z"),
    );
    assert_eq!(crate::library::db::audio_queue::all_history(&db).len(), 4);
    // Trim keeps the newest N.
    crate::library::db::audio_queue::trim_history(&db, 3);
    let all = crate::library::db::audio_queue::all_history(&db);
    assert_eq!(
        all.iter().map(|h| h.path.as_str()).collect::<Vec<_>>(),
        ["/old.mp3", "/tie2.mp3", "/tie1.mp3"]
    );
}

#[test]
fn remove_by_paths_deletes_only_matches() {
    let db = test_db("remove_paths_io");
    let history = |p: &str| crate::library::db::PlayHistory {
        path: p.into(),
        title: String::new(),
        artist: String::new(),
        duration_secs: 0,
        play_count: 1,
        played_at: "2026-01-01T00:00:00.000Z".into(),
    };
    crate::library::db::audio_queue::upsert_history(&db, &history("/a.mp3"));
    crate::library::db::audio_queue::upsert_history(&db, &history("/b.mp3"));
    crate::library::db::audio_queue::remove_history(
        &db,
        &["/a.mp3".to_string(), "/zz.mp3".to_string()],
    );
    let all = crate::library::db::audio_queue::all_history(&db);
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].path, "/b.mp3");
}

#[test]
fn playlist_item_counts_and_bulk_delete() {
    let db = test_db("pl_counts");
    let item = |pl: &str, path: &str, sort: i64| crate::library::db::PlaylistItem {
        id: format!("{pl}-{path}"),
        playlist_id: pl.into(),
        audio_path: path.into(),
        title: String::new(),
        artist: String::new(),
        duration_secs: 0,
        sort_order: sort,
        added_at: String::new(),
    };
    crate::library::db::audio_queue::insert_playlist_item(&db, &item("p1", "/1.mp3", 0));
    crate::library::db::audio_queue::insert_playlist_item(&db, &item("p1", "/2.mp3", 1));
    crate::library::db::audio_queue::insert_playlist_item(&db, &item("p2", "/3.mp3", 0));
    let counts = crate::library::db::audio_queue::playlist_item_counts(&db);
    assert_eq!(counts.get("p1"), Some(&2));
    assert_eq!(counts.get("p2"), Some(&1));
    assert!(!counts.contains_key("pX"));

    // Cascade delete by path across playlists.
    crate::library::db::audio_queue::remove_playlist_items_by_paths(&db, &["/1.mp3".to_string()]);
    assert_eq!(
        crate::library::db::audio_queue::playlist_items(&db, "p1").len(),
        1
    );
}
