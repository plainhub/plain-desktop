use super::*;
#[path = "../library/fixtures.rs"]
mod fixtures;

fn row(id: &str, path: &std::path::Path, kind: &str) -> scan::MediaFile {
    scan::MediaFile {
        uuid: id.into(),
        fsuuid: String::new(),
        ino: 0,
        ctime: 0,
        duration_sec: 0,
        duration_ref_mod: 0,
        duration_ref_size: 0,
        artist: String::new(),
        artist_ref_mod: 0,
        artist_ref_size: 0,
        title: String::new(),
        title_ref_mod: 0,
        title_ref_size: 0,
        path: path.to_str().unwrap().into(),
        original_path: String::new(),
        name: id.into(),
        size: 1,
        modified_at: 1,
        r#type: kind.into(),
        is_trash: false,
        trash_path: String::new(),
        deleted_at: 0,
    }
}
fn save(db: &Db, media: &scan::MediaFile) {
    db.insert(
        format!("media:uuid:{}", media.uuid),
        serde_json::to_vec(media).unwrap(),
    )
    .unwrap();
}
fn references(db: &SqlDb, media: &scan::MediaFile) {
    db.with_conn(|c| {
        c.execute("INSERT INTO audio_queue_items(path,sort_order,title,artist,duration_ms) VALUES(?1,0,'synthetic','',10)",[&media.path])?;
        c.execute("INSERT INTO media_item(media_type,media_id,duration_ms,updated_at) VALUES('audio',?1,10,'now')",[&media.uuid])?;
        c.execute("INSERT INTO tag_relations(tag_id,key,type,title,size,created_at) VALUES('synthetic',?1,1,'',1,'now'),('synthetic',?1,2,'',1,'now')",[&media.uuid])?;
        Ok::<_,rusqlite::Error>(())
    }).unwrap();
}
#[tokio::test]
async fn physical_failure_preserves_references_and_partial_success_is_visible() {
    crate::media::paths::pin_test_data_dir();
    let library = fixtures::test_db("item_actions");
    let temp = tempfile::tempdir().unwrap();
    let kv = Arc::new(Db::open(&temp.path().join("kv")).unwrap());
    let blocked = temp.path().join("blocking-file");
    std::fs::write(&blocked, b"synthetic").unwrap();
    let failed = row("failed", &blocked.join("child"), "audio");
    let successful = row("successful", &temp.path().join("successful.wav"), "audio");
    std::fs::write(&successful.path, b"synthetic").unwrap();
    for media in [&failed, &successful] {
        save(&kv, media);
        references(&library, media);
    }
    let result = run_media_items_action(
        &kv,
        &library,
        Some("audio"),
        "ids:successful,failed",
        MediaItemsAction::Delete,
    )
    .await;
    assert!(result.unwrap_err().to_string().contains("1 completed"));
    assert!(!std::path::Path::new(&successful.path).exists());
    assert!(scan::get_by_uuid(&kv, "successful").unwrap().is_none());
    assert!(scan::get_by_uuid(&kv, "failed").unwrap().is_some());
    library
        .with_conn(|c| {
            assert_eq!(
                c.query_row("SELECT path FROM audio_queue_items", [], |r| r
                    .get::<_, String>(0))?,
                failed.path
            );
            assert_eq!(
                c.query_row("SELECT media_id FROM media_item", [], |r| r
                    .get::<_, String>(0))?,
                failed.uuid
            );
            assert_eq!(
                c.query_row("SELECT count(*) FROM tag_relations WHERE type=1", [], |r| r
                    .get::<_, i64>(0))?,
                1
            );
            assert_eq!(
                c.query_row("SELECT count(*) FROM tag_relations WHERE type=2", [], |r| r
                    .get::<_, i64>(0))?,
                2
            );
            Ok::<_, rusqlite::Error>(())
        })
        .unwrap();
}
#[tokio::test]
async fn counts_are_deduplicated_and_wrong_types_or_corrupt_rows_are_errors() {
    crate::media::paths::pin_test_data_dir();
    let library = fixtures::test_db("item_counts");
    let temp = tempfile::tempdir().unwrap();
    let kv = Arc::new(Db::open(&temp.path().join("kv")).unwrap());
    let media = row("one", &temp.path().join("one.png"), "image");
    std::fs::write(&media.path, b"synthetic").unwrap();
    save(&kv, &media);
    assert!(
        run_media_items_action(
            &kv,
            &library,
            Some("audio"),
            "ids:one",
            MediaItemsAction::Delete
        )
        .await
        .is_err()
    );
    assert!(std::path::Path::new(&media.path).exists());
    assert_eq!(
        run_media_items_action(
            &kv,
            &library,
            Some("image"),
            "ids:one,one",
            MediaItemsAction::Delete
        )
        .await
        .unwrap(),
        1
    );
    kv.insert("media:uuid:broken", b"invalid json").unwrap();
    assert!(scan::get_by_uuid(&kv, "broken").is_err());
    assert!(
        run_media_items_action(
            &kv,
            &library,
            Some("image"),
            "ids:broken",
            MediaItemsAction::Delete
        )
        .await
        .is_err()
    );
}
#[test]
fn search_selection_includes_every_page_beyond_ten_thousand() {
    let temp = tempfile::tempdir().unwrap();
    let index = image_index::MediaSearchIndex::open(temp.path()).unwrap();
    for i in 0..10_003 {
        let mut media = row(
            &format!("select{i}"),
            &temp.path().join(format!("select{i}.jpg")),
            "image",
        );
        media.name = "select synthetic".into();
        index.add_media_file(&media).unwrap();
    }
    index.commit().unwrap();
    assert_eq!(
        search_ids(&index, "text:select", Some("image"), None)
            .unwrap()
            .len(),
        10_003
    );
    assert!(
        search_ids(&index, "text:select", Some("audio"), None)
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn no_op_trash_does_not_count_and_trash_filesystem_errors_are_visible() {
    crate::media::paths::pin_test_data_dir();
    let library = fixtures::test_db("item_noop");
    let temp = tempfile::tempdir().unwrap();
    let kv = Arc::new(Db::open(&temp.path().join("kv")).unwrap());
    let mut media = row(
        "already-trashed",
        &temp.path().join("already-trashed"),
        "audio",
    );
    media.is_trash = true;
    save(&kv, &media);
    references(&library, &media);
    assert_eq!(
        run_media_items_action(
            &kv,
            &library,
            Some("audio"),
            "ids:already-trashed",
            MediaItemsAction::Trash
        )
        .await
        .unwrap(),
        0
    );
    assert_eq!(
        run_media_items_action(
            &kv,
            &library,
            Some("audio"),
            "ids:",
            MediaItemsAction::Delete
        )
        .await
        .unwrap(),
        0
    );
    library
        .with_conn(|c| {
            c.query_row("SELECT count(*) FROM audio_queue_items", [], |r| {
                r.get::<_, i64>(0)
            })
        })
        .map(|count| assert_eq!(count, 1))
        .unwrap();
    let blocking = temp.path().join("blocking-file");
    std::fs::write(&blocking, b"synthetic").unwrap();
    let trash_path = blocking.join(".nas-trash/data/2026/10/f_synthetic_file.wav");
    assert!(
        trash::delete_trash_by_path(trash_path.to_str().unwrap())
            .await
            .is_err()
    );
    assert_eq!(std::fs::read(&blocking).unwrap(), b"synthetic");
}
