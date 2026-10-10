use super::*;
#[test]
fn restart_exact_milliseconds_validation_and_recent_boundary() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let db = Db::open(&path).unwrap();
    assert!(save(&db, " ", 12).is_err());
    assert!(save(&db, "video", -1).is_err());
    let row = save(&db, "video", 3_000_000_123).unwrap();
    drop(db);
    let db = Db::open(&path).unwrap();
    assert_eq!(
        db.video_progress_get("video").unwrap().unwrap().position_ms,
        3_000_000_123
    );
    let boundary = chrono::DateTime::parse_from_rfc3339(&row.updated_at)
        .unwrap()
        .with_timezone(&chrono::Utc);
    assert_eq!(recent(&db, boundary).unwrap().len(), 1);
    assert!(
        recent(&db, boundary + chrono::Duration::milliseconds(1))
            .unwrap()
            .is_empty()
    );
    save(&db, "video", 0).unwrap();
    assert_eq!(
        db.video_progress_get("video").unwrap().unwrap().position_ms,
        0
    );
    delete(&db, "video").unwrap();
    delete(&db, "video").unwrap();
    assert!(db.video_progress_get("video").unwrap().is_none());
}
#[test]
fn failed_save_preserves_existing_position() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("db")).unwrap();
    save(&db, "video", 123).unwrap();
    db.with_conn(|c| c.execute_batch("CREATE TRIGGER fail_progress BEFORE INSERT ON video_play_progress BEGIN SELECT RAISE(ABORT, 'fail'); END")).unwrap();
    assert!(save(&db, "video", 456).is_err());
    assert_eq!(
        db.video_progress_get("video").unwrap().unwrap().position_ms,
        123
    );
}
