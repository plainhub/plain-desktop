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
