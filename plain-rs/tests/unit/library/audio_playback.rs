use super::*;
use crate::library::audio_queue;
#[test]
fn resume_is_exact_and_stale_reports_cannot_overwrite_seek_or_returned_track() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db");
    let db = Db::open(&path).unwrap();
    audio_queue::save_audio_current(&db, "one").unwrap();
    let first = snapshot(&db).unwrap();
    assert!(report(&db, "one", first.revision, 3_000_000_123).unwrap());
    drop(db);
    let db = Db::open(&path).unwrap();
    assert_eq!(snapshot(&db).unwrap().position_ms, 3_000_000_123);
    let resumed = invalidate(&db).unwrap();
    assert_eq!(resumed.position_ms, 3_000_000_123);
    assert!(!report(&db, "one", first.revision, 0).unwrap());
    let sought = seek(&db, 0).unwrap();
    assert!(!report(&db, "one", resumed.revision, 100).unwrap());
    assert!(report(&db, "one", sought.revision, 456).unwrap());
    audio_queue::save_audio_current(&db, "two").unwrap();
    assert_eq!(snapshot(&db).unwrap().position_ms, 0);
    audio_queue::save_audio_current(&db, "one").unwrap();
    assert!(!report(&db, "one", sought.revision, 999).unwrap());
    assert_eq!(snapshot(&db).unwrap().position_ms, 0);
    assert!(seek(&db, -1).is_err());
    assert!(report(&db, "one", 0, -1).is_err());
}
#[test]
fn source_and_progress_reset_are_atomic() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("db")).unwrap();
    audio_queue::save_audio_current(&db, "one").unwrap();
    seek(&db, 123).unwrap();
    db.with_conn(|c| c.execute_batch("CREATE TRIGGER fail_playback BEFORE UPDATE ON audio_playback BEGIN SELECT RAISE(ABORT,'fail'); END")).unwrap();
    assert!(audio_queue::save_audio_current(&db, "two").is_err());
    assert_eq!(audio_queue::get_audio_current(&db).unwrap(), "one");
    assert_eq!(snapshot(&db).unwrap().position_ms, 123);
    assert!(seek(&db, 456).is_err());
    assert_eq!(snapshot(&db).unwrap().position_ms, 123);
}

#[test]
fn playback_start_is_once_per_revision_and_failed_history_write_can_retry() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("db")).unwrap();
    let track = audio_queue::AudioTrack::from_path_stem("one");
    let row = prepare_track(&db, &track, true).unwrap();
    assert!(started(&db, &track, row.revision).unwrap());
    assert!(!started(&db, &track, row.revision).unwrap());
    assert_eq!(
        audio_queue::history_page(&db, 0, 10, "").unwrap()[0].play_count,
        1
    );
    let newer = prepare_track(&db, &track, false).unwrap();
    assert!(!started(&db, &track, row.revision).unwrap());
    db.with_conn(|c| c.execute_batch("CREATE TRIGGER fail_history BEFORE INSERT ON audio_play_history BEGIN SELECT RAISE(ABORT,'fail'); END")).unwrap();
    assert!(started(&db, &track, newer.revision).is_err());
    db.with_conn(|c| c.execute_batch("DROP TRIGGER fail_history"))
        .unwrap();
    assert!(started(&db, &track, newer.revision).unwrap());
    assert_eq!(
        audio_queue::history_page(&db, 0, 10, "").unwrap()[0].play_count,
        2
    );
}
