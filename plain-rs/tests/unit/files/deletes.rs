use super::*;
#[test]
fn deleted_receipts_cascade_atomically_and_preserve_failed_and_foreign_records() {
    let db = Db::open(std::path::Path::new(":memory:")).unwrap();
    db.with_conn(|c|c.execute_batch("INSERT INTO tag_relations(tag_id,key,type,created_at,size,title) VALUES('video','old',2,'first',4,'video'),('foreign','old',6,'first',4,'note'),('file','/root/deleted',22,'first',4,'file'),('failed','/root/failed',22,'first',4,'failed'),('sibling','/root_sibling/file',22,'first',4,'sibling'); INSERT INTO media_item(media_type,media_id,duration_ms,updated_at) VALUES('video','old',5000000001,'first'),('video','keep',5000000002,'first'); INSERT INTO video_play_progress(media_id,position_ms,updated_at) VALUES('old',4000000001,'first'),('keep',4000000002,'first'); INSERT INTO audio_queue_items(path,sort_order,title,artist,duration_ms) VALUES('/root/deleted',0,'gone','',1),('/root/failed',1,'keep','',1),('/root_sibling/file',2,'keep','',1);")).unwrap();
    db.with_conn(|c|c.execute_batch("INSERT INTO image_embeddings(id,path,embedding,created_at,updated_at) VALUES('private','/root/deleted',x'3f800000','first','first'),('pending','/root/failed',x'3f800000','first','first'),('sibling','/root_sibling/file',x'3f800000','first','first');")).unwrap();
    cleanup(
        &db,
        &[Item {
            media_type: 2,
            id: "old".into(),
            path: "/root/deleted".into(),
        }],
        &["/root/deleted".into()],
        &[],
    )
    .unwrap();
    db.with_conn(|c| {
        assert_eq!(
            c.query_row("SELECT count(*) FROM media_item", [], |r| r
                .get::<_, i64>(0))?,
            1
        );
        assert_eq!(
            c.query_row("SELECT count(*) FROM video_play_progress", [], |r| r
                .get::<_, i64>(0))?,
            1
        );
        assert_eq!(
            c.query_row("SELECT count(*) FROM tag_relations", [], |r| r
                .get::<_, i64>(0))?,
            3
        );
        assert_eq!(
            c.query_row("SELECT count(*) FROM image_embeddings", [], |r| r
                .get::<_, i64>(0))?,
            2
        );
        assert_eq!(
            c.query_row("SELECT count(*) FROM audio_queue_items", [], |r| r
                .get::<_, i64>(0))?,
            2
        );
        Ok::<_, rusqlite::Error>(())
    })
    .unwrap();
    cleanup(&db, &[], &["/root".into()], &["/root".into()]).unwrap();
    assert_eq!(
        db.with_conn(
            |c| c.query_row("SELECT count(*) FROM image_embeddings", [], |r| r
                .get::<_, i64>(0))
        )
        .unwrap(),
        1
    );
    db.with_conn(|c| {
        assert_eq!(
            c.query_row("SELECT count(*) FROM audio_queue_items", [], |r| r
                .get::<_, i64>(0))?,
            1
        );
        assert_eq!(
            c.query_row("SELECT count(*) FROM tag_relations", [], |r| r
                .get::<_, i64>(0))?,
            2
        );
        Ok::<_, rusqlite::Error>(())
    })
    .unwrap();
}
