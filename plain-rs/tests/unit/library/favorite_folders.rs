//! Behavior locks for `src/library/favorite_folders.rs` — ported 1:1
//! from plain-nas's `tests/unit/favorites.rs` (the prefs-backed
//! implementation these functions were moved from).

use super::*;
#[path = "fixtures.rs"]
mod fixtures;
use fixtures::*;

#[test]
fn add_then_list_round_trip() {
    let db = test_db("ff_round_trip");
    let added = add(&db, "/mnt/data", "photos/2024").unwrap();
    assert_eq!(added.root_path, "/mnt/data");
    assert_eq!(added.relative_path, "photos/2024");
    assert!(added.alias.is_none());

    let l = list(&db).unwrap();
    assert_eq!(l.len(), 1);
    assert_eq!(l[0].root_path, "/mnt/data");
    assert_eq!(l[0].relative_path, "photos/2024");
    assert!(l[0].alias.is_none());
}

#[test]
fn add_is_idempotent() {
    let db = test_db("ff_idempotent");
    add(&db, "/mnt/data", "photos").unwrap();
    let second = add(&db, "/mnt/data", "photos").unwrap();
    assert_eq!(list(&db).unwrap().len(), 1, "duplicate add must be deduped");
    assert_eq!(second.root_path, "/mnt/data");
    assert_eq!(second.relative_path, "photos");
}

#[test]
fn add_collapses_dot_relative_path() {
    let db = test_db("ff_dot");
    // "photos/." normalizes to "photos" so add and second-add collide.
    add(&db, "/mnt/data", "photos/.").unwrap();
    let second = add(&db, "/mnt/data", "photos").unwrap();
    assert_eq!(list(&db).unwrap().len(), 1);
    assert_eq!(second.relative_path, "photos");
}

#[test]
fn remove_hit_returns_entry_and_drops_it() {
    let db = test_db("ff_remove_hit");
    add(&db, "/mnt/data", "photos").unwrap();
    add(&db, "/mnt/data", "videos").unwrap();

    let removed = remove(&db, "/mnt/data", "photos").unwrap();
    assert_eq!(removed.relative_path, "photos");
    assert!(removed.alias.is_none());

    let l = list(&db).unwrap();
    assert_eq!(l.len(), 1);
    assert_eq!(l[0].relative_path, "videos");
}

#[test]
fn remove_miss_returns_synthetic_stub() {
    let db = test_db("ff_remove_miss");
    let r = remove(&db, "/mnt/data", "nope").unwrap();
    assert_eq!(r.root_path, "/mnt/data");
    assert_eq!(r.relative_path, "nope");
    assert!(r.alias.is_none());
    // Should not create a record.
    assert!(list(&db).unwrap().is_empty());
}

#[test]
fn alias_set_and_clear() {
    let db = test_db("ff_alias");
    add(&db, "/mnt/data", "photos").unwrap();

    set_alias(&db, "/mnt/data", "photos", "  My Pics  ").unwrap();
    let l = list(&db).unwrap();
    assert_eq!(
        l[0].alias.as_deref(),
        Some("My Pics"),
        "alias should be trimmed"
    );

    // Empty alias clears.
    set_alias(&db, "/mnt/data", "photos", "   ").unwrap();
    let l = list(&db).unwrap();
    assert!(l[0].alias.is_none(), "empty trimmed alias must clear field");
}

#[test]
fn alias_set_same_value_is_noop() {
    let db = test_db("ff_alias_noop");
    add(&db, "/mnt/data", "photos").unwrap();
    set_alias(&db, "/mnt/data", "photos", "My Pics").unwrap();
    // Same value: no-op, value survives.
    set_alias(&db, "/mnt/data", "photos", "My Pics").unwrap();
    assert_eq!(list(&db).unwrap()[0].alias.as_deref(), Some("My Pics"));
}

#[test]
fn list_handles_empty_store() {
    let db = test_db("ff_empty");
    assert!(list(&db).unwrap().is_empty());
}

#[test]
fn list_normalizes_paths_to_slash() {
    let db = test_db("ff_slash");
    add(&db, "/mnt/data", "photos/2024").unwrap();
    for f in list(&db).unwrap() {
        assert!(!f.root_path.contains('\\'));
        assert!(!f.relative_path.contains('\\'));
    }
}

// ---------------------------------------------------------------------------
// Phone-contract fullPath join/split helpers
// ---------------------------------------------------------------------------

#[test]
fn full_path_joins_and_splits_round_trip() {
    let f = FavoriteFolder {
        root_path: "/mnt/data".to_string(),
        relative_path: "photos/2024".to_string(),
        alias: None,
    };
    assert_eq!(full_path_of(&f), "/mnt/data/photos/2024");
    // Root itself: relative empty → fullPath == rootPath.
    let root = FavoriteFolder {
        root_path: "/mnt/data".to_string(),
        relative_path: String::new(),
        alias: None,
    };
    assert_eq!(full_path_of(&root), "/mnt/data");

    let (r, rel) = split_full_path("/mnt/data", "/mnt/data/photos/2024/").unwrap();
    assert_eq!(r, "/mnt/data");
    assert_eq!(rel, "photos/2024");
    // Paths outside the root are rejected.
    assert!(split_full_path("/mnt/data", "/other/x").is_err());
}

#[test]
fn find_by_full_path_matches_stored_rows() {
    let db = test_db("ff_find");
    add(&db, "/mnt/data", "photos").unwrap();
    assert!(
        find_by_full_path(&db, "/mnt/data/photos")
            .unwrap()
            .is_some()
    );
    // Trailing-slash-insensitive.
    assert!(
        find_by_full_path(&db, "/mnt/data/photos/")
            .unwrap()
            .is_some()
    );
    assert!(
        find_by_full_path(&db, "/mnt/data/videos")
            .unwrap()
            .is_none()
    );
}

#[test]
fn full_path_identity_preserves_alias_and_rejects_outside_root() {
    let db = test_db("favorite_full_identity");
    add_full_path(&db, "/synthetic", "/synthetic/folder").unwrap();
    set_alias(&db, "/synthetic", "folder", "Saved").unwrap();
    add_full_path(&db, "/synthetic/folder", "/synthetic/folder").unwrap();
    let items = list(&db).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].alias.as_deref(), Some("Saved"));
    assert_eq!(items[0].root_path, "/synthetic/folder");
    assert!(add_full_path(&db, "/synthetic", "/synthetic-other/folder").is_err());
    assert!(add_full_path(&db, "/synthetic", "/synthetic/../other").is_err());
    assert_eq!(
        split_full_path("/", "/folder").unwrap(),
        ("/".into(), "folder".into())
    );
}
#[test]
fn storage_failures_propagate_and_failed_writes_keep_the_alias() {
    let db = test_db("favorite_checked_io");
    add_full_path(&db, "/synthetic", "/synthetic/folder").unwrap();
    set_alias(&db, "/synthetic", "folder", "Saved").unwrap();
    db.with_conn(|c|c.execute_batch("CREATE TRIGGER reject_favorite BEFORE INSERT ON favorite_folders BEGIN SELECT RAISE(ABORT,'synthetic failure'); END;")).unwrap();
    assert!(add_full_path(&db, "/synthetic/folder", "/synthetic/folder").is_err());
    assert_eq!(list(&db).unwrap()[0].alias.as_deref(), Some("Saved"));
    db.with_conn(|c| c.execute_batch("DROP TABLE favorite_folders"))
        .unwrap();
    assert!(list(&db).is_err());
    assert!(add(&db, "/synthetic", "new").is_err());
    assert!(remove(&db, "/synthetic", "folder").is_err());
    assert!(set_alias(&db, "/synthetic", "folder", "New").is_err());
}

#[test]
fn literal_folder_name_whitespace_is_preserved() {
    let db = test_db("favorite_literal_paths");
    add_full_path(&db, "/synthetic", "/synthetic/folder ").unwrap();
    assert_eq!(full_path_of(&list(&db).unwrap()[0]), "/synthetic/folder ");
}

#[test]
#[cfg(not(windows))]
fn literal_posix_backslash_is_not_a_separator() {
    let db = test_db("favorite_literal_backslash");
    let path = r"/synthetic/folder\literal";
    add_full_path(&db, "/synthetic", path).unwrap();
    assert_eq!(full_path_of(&list(&db).unwrap()[0]), path);
}
