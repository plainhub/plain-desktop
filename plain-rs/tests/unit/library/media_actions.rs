use super::*;
#[path = "fixtures.rs"]
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
