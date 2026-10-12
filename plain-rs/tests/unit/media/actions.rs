use super::*;
#[path = "../library/fixtures.rs"]
mod fixtures;
use fixtures::test_db;
fn item(id: &str, path: &str) -> Item {
    Item {
        id: id.into(),
        path: path.into(),
        destination_path: String::new(),
    }
}
#[test]
fn only_acknowledged_successes_are_cleaned_and_type_isolated() {
    let db = test_db("media_action_success");
    db.with_conn(|c|c.execute_batch("INSERT INTO tags(id,name,type,count,created_at,updated_at) VALUES('audio','audio',1,0,'now','now'),('video','video',2,0,'now','now'); INSERT INTO tag_relations(tag_id,key,type,title,size,created_at) VALUES('audio','one',1,'one',1,'now'),('audio','failed',1,'failed',1,'now'),('video','one',2,'one',1,'now'); INSERT INTO media_item(media_type,media_id,duration_ms,updated_at) VALUES('audio','one',123,'now'),('video','one',456,'now'); INSERT INTO audio_queue_items(path,sort_order,title,artist,duration_ms) VALUES('/one',0,'one','',123),('/failed',1,'failed','',123);")).unwrap();
    let outcome = Outcome {
        successful: vec![item("one", "/one")],
        failed_ids: vec!["failed".into()],
    };
    validate(
        DataType::Audio,
        Action::Trash,
        &["one".into(), "failed".into()],
        &outcome,
    )
    .unwrap();
    assert_eq!(
        cleanup(&db, DataType::Audio, Action::Trash, &outcome.successful).unwrap(),
        1
    );
    db.with_conn(|c| {
        assert_eq!(
            c.query_row("SELECT count(*) FROM tag_relations", [], |r| r
                .get::<_, i64>(0))?,
            2
        );
        assert_eq!(
            c.query_row("SELECT duration_ms FROM media_item", [], |r| r
                .get::<_, i64>(0))?,
            456
        );
        assert_eq!(
            c.query_row("SELECT path FROM audio_queue_items", [], |r| r
                .get::<_, String>(0))?,
            "/failed"
        );
        Ok::<_, rusqlite::Error>(())
    })
    .unwrap();
}
#[test]
fn malformed_or_incomplete_receipts_are_rejected() {
    let requested = vec!["one".into(), "two".into()];
    let mut outcome = Outcome {
        successful: vec![item("one", "/one")],
        failed_ids: vec![],
    };
    assert!(validate(DataType::Image, Action::Delete, &requested, &outcome).is_err());
    outcome.failed_ids.push("two".into());
    validate(DataType::Image, Action::Delete, &requested, &outcome).unwrap();
    outcome.successful.push(item("alien", "/alien"));
    assert!(validate(DataType::Image, Action::Delete, &requested, &outcome).is_err());
    assert!(validate(DataType::Note, Action::Delete, &requested, &outcome).is_err());
}
#[test]
fn cleanup_transaction_failure_preserves_all_references() {
    let db = test_db("media_action_rollback");
    db.with_conn(|c|c.execute_batch("INSERT INTO tags(id,name,type,count,created_at,updated_at) VALUES('audio','audio',1,0,'now','now'); INSERT INTO tag_relations(tag_id,key,type,title,size,created_at) VALUES('audio','one',1,'one',1,'now'); INSERT INTO audio_queue_items(path,sort_order,title,artist,duration_ms) VALUES('/one',0,'one','',123); CREATE TRIGGER reject_queue BEFORE DELETE ON audio_queue_items BEGIN SELECT RAISE(ABORT,'synthetic failure'); END;")).unwrap();
    assert!(cleanup(&db, DataType::Audio, Action::Delete, &[item("one", "/one")]).is_err());
    db.with_conn(|c| {
        assert_eq!(
            c.query_row("SELECT count(*) FROM tag_relations", [], |r| r
                .get::<_, i64>(0))?,
            1
        );
        assert_eq!(
            c.query_row("SELECT count(*) FROM audio_queue_items", [], |r| r
                .get::<_, i64>(0))?,
            1
        );
        Ok::<_, rusqlite::Error>(())
    })
    .unwrap();
}

#[test]
fn video_cleanup_keeps_other_types_and_move_keeps_user_tags() {
    let db = test_db("media_action_video_image");
    db.with_conn(|c|c.execute_batch("INSERT INTO tags(id,name,type,count,created_at,updated_at) VALUES('image','image',3,0,'now','now'); INSERT INTO tag_relations(tag_id,key,type,title,size,created_at) VALUES('image','one',3,'one',1,'now'); INSERT INTO video_play_progress(media_id,position_ms,updated_at) VALUES('one',456,'now'); INSERT INTO media_item(media_type,media_id,duration_ms,updated_at) VALUES('video','one',123,'now'),('audio','one',456,'now'); INSERT INTO image_embeddings(id,path,embedding,created_at,updated_at) VALUES('one','/one',X'3f800000','now','now');")).unwrap();
    let item = Item {
        id: "one".into(),
        path: "/one".into(),
        destination_path: "/new/one".into(),
    };
    cleanup(
        &db,
        DataType::Image,
        Action::Move,
        std::slice::from_ref(&item),
    )
    .unwrap();
    db.with_conn(|c| {
        assert_eq!(
            c.query_row("SELECT count(*) FROM tag_relations", [], |r| r
                .get::<_, i64>(0))?,
            1
        );
        assert_eq!(
            c.query_row("SELECT count(*) FROM image_embeddings", [], |r| r
                .get::<_, i64>(0))?,
            0
        );
        Ok::<_, rusqlite::Error>(())
    })
    .unwrap();
    cleanup(
        &db,
        DataType::Video,
        Action::Delete,
        std::slice::from_ref(&item),
    )
    .unwrap();
    db.with_conn(|c| {
        assert_eq!(
            c.query_row("SELECT count(*) FROM tag_relations", [], |r| r
                .get::<_, i64>(0))?,
            1
        );
        assert_eq!(
            c.query_row("SELECT count(*) FROM video_play_progress", [], |r| r
                .get::<_, i64>(0))?,
            0
        );
        assert_eq!(
            c.query_row("SELECT duration_ms FROM media_item", [], |r| r
                .get::<_, i64>(0))?,
            456
        );
        Ok::<_, rusqlite::Error>(())
    })
    .unwrap();
}

fn catalog_file(path: &str) -> crate::db::MediaFileMetadata {
    crate::db::MediaFileMetadata {
        data_type: DataType::Doc,
        path: path.into(),
        title: "document".into(),
        artist: String::new(),
        bucket_id: "/library".into(),
        size_bytes: 10,
        duration_ms: 0,
        album_file_id: String::new(),
        width: 0,
        height: 0,
        rotation: 0,
        taken_at: None,
        created_at: "created".into(),
        updated_at: "updated".into(),
    }
}

#[test]
fn confirmed_actions_preserve_catalog_identity_and_restore_the_original_path() {
    let db = test_db("catalog_action_state");
    let original = db
        .record_media_files(&[catalog_file("/library/document.txt")])
        .unwrap()
        .remove(0);
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE media_files SET is_favorite=1 WHERE id=?1",
            [&original.id],
        )
    })
    .unwrap();
    let mut receipt = item(&original.id, &original.path);
    receipt.destination_path = "/.plain-trash/document.txt".into();
    cleanup(&db, DataType::Doc, Action::Trash, &[receipt.clone()]).unwrap();
    let trash = db
        .media_file_by_path(DataType::Doc, &receipt.destination_path)
        .unwrap()
        .unwrap();
    assert!(trash.is_trashed);
    assert!(trash.is_favorite);
    assert_eq!(trash.id, original.id);
    assert_eq!(trash.original_path, Some(original.path.clone()));
    cleanup(
        &db,
        DataType::Doc,
        Action::Restore,
        &[item(&trash.id, &trash.path)],
    )
    .unwrap();
    let restored = db
        .media_file_by_path(DataType::Doc, &original.path)
        .unwrap()
        .unwrap();
    assert!(!restored.is_trashed);
    assert_eq!(restored.original_path, None);
    assert_eq!(restored.created_at, original.created_at);
    receipt.path = restored.path;
    receipt.destination_path = "/other/renamed.pdf".into();
    cleanup(&db, DataType::Doc, Action::Move, &[receipt.clone()]).unwrap();
    let moved = db
        .media_file_by_path(DataType::Doc, &receipt.destination_path)
        .unwrap()
        .unwrap();
    assert_eq!(moved.id, original.id);
    assert_eq!(moved.bucket_id, "/other");
    assert!(moved.is_favorite);
    cleanup(
        &db,
        DataType::Doc,
        Action::Delete,
        &[item(&moved.id, &moved.path)],
    )
    .unwrap();
    assert!(
        db.media_file_by_path(DataType::Doc, &moved.path)
            .unwrap()
            .is_none()
    );
}

#[test]
fn stale_receipts_and_catalog_sql_failures_do_not_commit_partial_updates() {
    let db = test_db("catalog_action_atomic");
    let files = db
        .record_media_files(&[
            catalog_file("/library/one.txt"),
            catalog_file("/library/two.txt"),
        ])
        .unwrap();
    assert!(
        cleanup(
            &db,
            DataType::Doc,
            Action::Delete,
            &[item(&files[0].id, "/wrong/path")]
        )
        .is_err()
    );
    let mut first = item(&files[0].id, &files[0].path);
    first.destination_path = "/.plain-trash/first.txt".into();
    assert!(
        cleanup(
            &db,
            DataType::Doc,
            Action::Trash,
            &[first, item(&files[1].id, "/wrong/path")]
        )
        .is_err()
    );
    assert!(
        !db.media_file_by_path(DataType::Doc, &files[0].path)
            .unwrap()
            .unwrap()
            .is_trashed
    );
    db.with_conn(|conn| conn.execute_batch("CREATE TRIGGER reject_catalog_delete BEFORE DELETE ON media_files BEGIN SELECT RAISE(ABORT,'synthetic failure'); END;")).unwrap();
    assert!(
        cleanup(
            &db,
            DataType::Doc,
            Action::Delete,
            &[item(&files[0].id, &files[0].path)]
        )
        .is_err()
    );
    assert!(
        db.media_file_by_path(DataType::Doc, &files[0].path)
            .unwrap()
            .is_some()
    );
    db.with_conn(|conn| conn.execute_batch("DROP TRIGGER reject_catalog_delete;"))
        .unwrap();
    let mut conflict = item(&files[0].id, &files[0].path);
    conflict.destination_path = files[1].path.clone();
    assert!(cleanup(&db, DataType::Doc, Action::Move, &[conflict]).is_err());
    assert!(
        db.media_file_by_path(DataType::Doc, &files[0].path)
            .unwrap()
            .is_some()
    );
}
