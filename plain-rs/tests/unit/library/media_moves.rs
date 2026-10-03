use super::*;
fn db() -> Db {
    Db::open(std::path::Path::new(":memory:")).unwrap()
}
fn binding(kind: i32, old: &str, new: &str) -> Binding {
    Binding {
        media_type: kind,
        source_id: old.into(),
        destination_id: new.into(),
        source_path: format!("/source/{old}"),
        destination_path: format!("/target/{new}"),
    }
}
#[test]
fn typed_bindings_preserve_tags_exact_durations_video_progress_and_remove_old_audio_paths() {
    let db = db();
    db.with_conn(|c|c.execute_batch("INSERT INTO tag_relations(tag_id,key,type,created_at,size,title) VALUES('audio','old',1,'first',4,'audio'),('video','old',2,'first',5,'video'),('foreign','old',6,'first',5,'note'),('file','/source/nested/file.txt',22,'first',7,'file'); INSERT INTO media_item(media_type,media_id,duration_ms,updated_at) VALUES('audio','old',5000000001,'first'),('video','old',6000000001,'first'); INSERT INTO video_play_progress(media_id,position_ms,updated_at) VALUES('old',4000000001,'first'); INSERT INTO audio_queue_items(path,sort_order,title,artist,duration_ms) VALUES('/source/old',0,'audio','',5000000001),('/unrelated',1,'unrelated','',1);")).unwrap();
    assert_eq!(
        rebind(
            &db,
            &[
                binding(1, "old", "audio-new"),
                binding(2, "old", "video-new")
            ],
            "/source",
            "/target"
        )
        .unwrap(),
        2
    );
    db.with_conn(|c| {
        assert_eq!(c.query_row("SELECT duration_ms FROM media_item WHERE media_type='audio' AND media_id='audio-new'",[],|r|r.get::<_,i64>(0)).unwrap(),5000000001);
        assert_eq!(c.query_row("SELECT position_ms FROM video_play_progress WHERE media_id='video-new'",[],|r|r.get::<_,i64>(0)).unwrap(),4000000001);
        assert_eq!(c.query_row("SELECT key FROM tag_relations WHERE tag_id='audio'",[],|r|r.get::<_,String>(0)).unwrap(),"audio-new");
        assert_eq!(c.query_row("SELECT key FROM tag_relations WHERE tag_id='foreign'",[],|r|r.get::<_,String>(0)).unwrap(),"old");
        assert_eq!(c.query_row("SELECT key FROM tag_relations WHERE tag_id='file'",[],|r|r.get::<_,String>(0)).unwrap(),"/target/nested/file.txt");
        assert_eq!(c.query_row("SELECT path FROM audio_queue_items",[],|r|r.get::<_,String>(0)).unwrap(),"/unrelated");
    });
}
#[test]
fn source_snapshots_handle_permuted_provider_ids_without_overwriting_the_other_source() {
    let db = db();
    db.with_conn(|c|c.execute_batch("INSERT INTO tag_relations(tag_id,key,type,created_at,size,title) VALUES('one','a',2,'first',1,'a'),('two','b',2,'second',2,'b'); INSERT INTO media_item(media_type,media_id,duration_ms,updated_at) VALUES('video','a',11,'first'),('video','b',22,'second'); INSERT INTO video_play_progress(media_id,position_ms,updated_at) VALUES('a',1,'first'),('b',2,'second');")).unwrap();
    rebind(
        &db,
        &[binding(2, "a", "b"), binding(2, "b", "a")],
        "/source",
        "/target",
    )
    .unwrap();
    db.with_conn(|c| {
        assert_eq!(
            c.query_row(
                "SELECT duration_ms FROM media_item WHERE media_id='a'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            22
        );
        assert_eq!(
            c.query_row(
                "SELECT position_ms FROM video_play_progress WHERE media_id='b'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        assert_eq!(
            c.query_row(
                "SELECT key FROM tag_relations WHERE tag_id='one'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "b"
        );
    });
}
#[test]
fn database_failure_rolls_back_typed_and_path_bindings() {
    let db = db();
    db.with_conn(|c|c.execute_batch("INSERT INTO tag_relations(tag_id,key,type,created_at,size,title) VALUES('audio','old',1,'first',4,'audio'),('file','/source/file',22,'first',1,'file'); INSERT INTO audio_queue_items(path,sort_order,title,artist,duration_ms) VALUES('/source/old',0,'audio','',1); CREATE TRIGGER fail_queue BEFORE DELETE ON audio_queue_items BEGIN SELECT RAISE(ABORT,'synthetic failure'); END;")).unwrap();
    assert!(rebind(&db, &[binding(1, "old", "new")], "/source", "/target").is_err());
    db.with_conn(|c| {
        assert_eq!(
            c.query_row(
                "SELECT count(*) FROM tag_relations WHERE key IN ('old','/source/file')",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            2
        )
    });
}
#[test]
fn invalid_bindings_are_rejected_before_any_write_and_path_prefixes_are_exact() {
    let db = db();
    db.with_conn(|c|c.execute_batch("INSERT INTO tag_relations(tag_id,key,type,created_at,size,title) VALUES('file','/source-other/file',22,'first',1,'file');")).unwrap();
    assert!(rebind(&db, &[binding(6, "a", "b")], "/source", "/target").is_err());
    assert!(
        rebind(
            &db,
            &[binding(1, "a", "b"), binding(1, "a", "c")],
            "/source",
            "/target"
        )
        .is_err()
    );
    rebind(&db, &[], "/source", "/target").unwrap();
    db.with_conn(|c| {
        assert_eq!(
            c.query_row("SELECT key FROM tag_relations", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "/source-other/file"
        )
    });
}
