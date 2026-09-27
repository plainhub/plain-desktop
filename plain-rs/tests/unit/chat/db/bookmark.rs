use super::*;

#[test]
fn bookmark_store_survives_reopen_and_ungroups_on_group_delete() {
    let path = super::super::tests::unique_tmp_dir("bookmarks").join("local_chat.db");
    let db = ChatDb::open(&path).unwrap();
    let group = DBookmarkGroup::new("Links");
    insert_bookmark_group(&db, &group);
    let bookmark = DBookmark::new("https://example.com", &group.id);
    insert_bookmark(&db, &bookmark);
    assert_eq!(get_bookmarks_by_group_id(&db, &group.id).len(), 1);
    drop(db);

    let db = ChatDb::open(&path).unwrap();
    assert_eq!(get_bookmarks(&db).len(), 1);
    assert_eq!(get_bookmark_groups(&db).len(), 1);
    delete_bookmark_group(&db, &group.id);
    let ungrouped = get_bookmark_by_id(&db, &bookmark.id).unwrap();
    assert_eq!(ungrouped.group_id, "");
    assert!(get_bookmark_group_by_id(&db, &group.id).is_none());
    assert_eq!(delete_bookmarks(&db, &[bookmark.id]), 1);
    assert!(get_bookmarks(&db).is_empty());
}

/// Phone Room DAO listing order, locked: pinned first, then sort_order,
/// then creation (2026-09-26 — plain-nas switched to this store and the
/// order must not drift from the phone contract).
#[test]
fn listing_order_pins_first_then_sort_then_created() {
    let db = ChatDb::open(&super::super::tests::unique_tmp_dir("bm-order").join("local_chat.db"))
        .unwrap();
    let older = now_iso_minus(3600);
    let mut b1 = DBookmark::new("https://1.example", "");
    b1.sort_order = 2;
    b1.created_at = older.clone();
    b1.updated_at = older.clone();
    let mut b2 = DBookmark::new("https://2.example", "");
    b2.sort_order = 1;
    let mut b3 = DBookmark::new("https://3.example", "");
    b3.sort_order = 1;
    b3.created_at = older.clone();
    b3.updated_at = older.clone();
    b3.pinned = true;
    for b in [&b1, &b2, &b3] {
        insert_bookmark(&db, b);
    }
    let mine = [b1.id.clone(), b2.id.clone(), b3.id.clone()];
    let ids: Vec<String> = get_bookmarks(&db)
        .into_iter()
        .map(|b| b.id)
        .filter(|id| mine.contains(id))
        .collect();
    assert_eq!(ids, vec![b3.id.clone(), b2.id.clone(), b1.id.clone()]);
}

fn now_iso_minus(secs: u64) -> String {
    crate::utils::dbtime::iso_from_unix_millis(
        crate::utils::dbtime::now_millis() - secs as i64 * 1000,
    )
}
