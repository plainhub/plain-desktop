use super::*;
use plain_rs::db::MediaFileMetadata;

fn file(data_type: DataType, path: &str, title: &str, size_bytes: i64) -> MediaFileMetadata {
    MediaFileMetadata {
        data_type,
        path: path.into(),
        title: title.into(),
        artist: "Artist".into(),
        bucket_id: "bucket".into(),
        size_bytes,
        duration_ms: 10,
        album_file_id: String::new(),
        width: 0,
        height: 0,
        rotation: 0,
        taken_at: None,
        created_at: "2026-10-12T00:00:00Z".into(),
        updated_at: "2026-10-12T00:00:00Z".into(),
    }
}

#[test]
fn rows_and_count_share_literal_filters_and_stable_pagination() {
    let directory = tempfile::tempdir().unwrap();
    let db = Db::open(&directory.path().join("plain.db")).unwrap();
    db.record_media_files(&[
        file(DataType::Audio, "/audio/one.mp3", "100% Song", 30),
        file(DataType::Audio, "/audio/two.mp3", "Other Song", 10),
        file(DataType::Image, "/photos/one.jpg", "100% Song", 10),
    ])
    .unwrap();
    assert_eq!(count(&db, DataType::Audio, "text:100%").unwrap(), 1);
    assert_eq!(
        rows(
            &db,
            DataType::Audio,
            "text:100%",
            FileSortBy::NameAsc,
            10,
            0
        )
        .unwrap()
        .len(),
        1
    );
    let rows = rows(&db, DataType::Audio, "", FileSortBy::SizeAsc, 1, 1).unwrap();
    assert_eq!(rows[0].size_bytes, 30);
    assert_eq!(count(&db, DataType::Audio, "artist:Artist").unwrap(), 2);
    assert!(count(&db, DataType::Audio, "unknown:selection").is_err());
}

#[test]
fn document_filters_and_explicit_trash_selection_use_current_catalog() {
    let directory = tempfile::tempdir().unwrap();
    let db = Db::open(&directory.path().join("plain.db")).unwrap();
    let created = db
        .record_media_files(&[
            file(DataType::Doc, "/docs/one.pdf", "Document title", 30),
            file(DataType::Doc, "/docs/two.txt", "Another title", 10),
        ])
        .unwrap();
    assert_eq!(count(&db, DataType::Doc, "ext:pdf").unwrap(), 1);
    assert_eq!(
        count(&db, DataType::Doc, "type:application/pdf").unwrap(),
        1
    );
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE media_files SET is_trashed=1 WHERE id=?1",
            [&created[0].id],
        )
    })
    .unwrap();
    assert_eq!(count(&db, DataType::Doc, "").unwrap(), 1);
    assert_eq!(count(&db, DataType::Doc, "trash:true ext:pdf").unwrap(), 1);
    assert_eq!(
        rows(&db, DataType::Doc, "trash:true", FileSortBy::NameAsc, 10, 0).unwrap()[0].id,
        created[0].id
    );
}

#[test]
fn bucket_items_use_shared_visibility_and_original_folder_names() {
    let directory = tempfile::tempdir().unwrap();
    let db = Db::open(&directory.path().join("plain.db")).unwrap();
    let mut older = file(
        DataType::Image,
        "/first/Albums/old.jpg",
        "Metadata title",
        1,
    );
    older.bucket_id = "/first/Albums".into();
    let mut recent = file(
        DataType::Image,
        "/first/Albums/recent.jpg",
        "Different title",
        9_000_000_000,
    );
    recent.bucket_id = older.bucket_id.clone();
    recent.updated_at = "2026-10-13T00:00:00Z".into();
    let mut other = file(DataType::Image, "/second/Albums/other.jpg", "Title", 7);
    other.bucket_id = "/second/Albums".into();
    let created = db
        .record_media_files(&[
            older,
            recent,
            other,
            file(DataType::Audio, "/first/Albums/song.mp3", "Song", 10),
        ])
        .unwrap();
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE media_files SET is_trashed=1 WHERE id=?1",
            [&created[0].id],
        )
    })
    .unwrap();
    let items = bucket_item_facts(&db, DataType::Image).unwrap();
    assert_eq!(items.len() as i64, count(&db, DataType::Image, "").unwrap());
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["path"], "/first/Albums/recent.jpg");
    assert_eq!(items[0]["id"], "/first/Albums");
    assert_eq!(items[0]["name"], "Albums");
    assert_eq!(items[0]["sortName"], "Albums");
    assert_eq!(items[0]["size"], 9_000_000_000i64);
    assert_eq!(items[1]["id"], "/second/Albums");
    assert!(
        items
            .iter()
            .all(|item| item.as_object().unwrap().len() == 5)
    );
    assert!(bucket_item_facts(&db, DataType::Contact).is_err());
}

#[test]
fn document_bucket_items_keep_the_same_nonempty_document_filter_as_rows() {
    let directory = tempfile::tempdir().unwrap();
    let db = Db::open(&directory.path().join("plain.db")).unwrap();
    db.record_media_files(&[
        file(DataType::Doc, "/docs/readme.txt", "Readme", 5),
        file(DataType::Doc, "/docs/empty.txt", "Empty", 0),
    ])
    .unwrap();
    let items = bucket_item_facts(&db, DataType::Doc).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["path"], "/docs/readme.txt");
    assert_eq!(items.len() as i64, count(&db, DataType::Doc, "").unwrap());
}

#[test]
fn action_ids_share_filters_and_explicit_from_trash_overrides_the_query_mode() {
    let directory = tempfile::tempdir().unwrap();
    let db = Db::open(&directory.path().join("plain.db")).unwrap();
    let created = db
        .record_media_files(&[
            file(DataType::Doc, "/docs/one.pdf", "one", 20),
            file(DataType::Doc, "/docs/two.pdf", "two", 20),
            file(DataType::Doc, "/docs/three.txt", "three", 20),
        ])
        .unwrap();
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE media_files SET is_trashed=1 WHERE id=?1",
            [&created[0].id],
        )
    })
    .unwrap();
    assert_eq!(
        ids(&db, DataType::Doc, "ext:pdf", false).unwrap(),
        vec![created[1].id.clone()]
    );
    assert_eq!(
        ids(&db, DataType::Doc, "trash:false ext:pdf", true).unwrap(),
        vec![created[0].id.clone()]
    );
    assert_eq!(
        ids(&db, DataType::Doc, "trash:true ext:pdf", false).unwrap(),
        vec![created[0].id.clone()]
    );
    assert!(ids(&db, DataType::Contact, "", false).is_err());
    assert!(ids(&db, DataType::Doc, "unknown:filter", false).is_err());
}
