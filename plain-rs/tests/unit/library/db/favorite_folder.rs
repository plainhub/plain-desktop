//! Unit tests for `src/library/db/favorite_folder.rs` (row IO layer).

#[path = "../fixtures.rs"]
mod fixtures;
use fixtures::*;

#[test]
fn rows_keep_insertion_order() {
    let db = test_db("ff_row_order");
    crate::library::db::favorite_folder::insert_folder(
        &db,
        &crate::library::db::FavoriteFolderRow {
            root_path: "/a".into(),
            relative_path: "x".into(),
            alias: None,
        },
    );
    crate::library::db::favorite_folder::insert_folder(
        &db,
        &crate::library::db::FavoriteFolderRow {
            root_path: "/b".into(),
            relative_path: "y".into(),
            alias: Some("Alias".into()),
        },
    );
    let all = crate::library::db::favorite_folder::all_folders(&db);
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].root_path, "/a");
    assert_eq!(all[1].alias.as_deref(), Some("Alias"));
}

#[test]
fn set_alias_updates_and_reports_existence() {
    let db = test_db("ff_row_alias");
    crate::library::db::favorite_folder::insert_folder(
        &db,
        &crate::library::db::FavoriteFolderRow {
            root_path: "/a".into(),
            relative_path: "x".into(),
            alias: None,
        },
    );
    assert!(crate::library::db::favorite_folder::set_folder_alias(
        &db,
        "/a",
        "x",
        Some("N")
    ));
    assert!(!crate::library::db::favorite_folder::set_folder_alias(
        &db,
        "/a",
        "missing",
        Some("N")
    ));
    assert_eq!(
        crate::library::db::favorite_folder::folder_by_paths(&db, "/a", "x")
            .unwrap()
            .alias
            .as_deref(),
        Some("N")
    );
}

#[test]
fn remove_folder_returns_the_row() {
    let db = test_db("ff_row_remove");
    crate::library::db::favorite_folder::insert_folder(
        &db,
        &crate::library::db::FavoriteFolderRow {
            root_path: "/a".into(),
            relative_path: "x".into(),
            alias: Some("Z".into()),
        },
    );
    let removed = crate::library::db::favorite_folder::remove_folder(&db, "/a", "x");
    assert_eq!(removed.unwrap().alias.as_deref(), Some("Z"));
    assert!(crate::library::db::favorite_folder::remove_folder(&db, "/a", "x").is_none());
}
