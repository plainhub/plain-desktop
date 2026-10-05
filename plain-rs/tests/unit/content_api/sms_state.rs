use super::*;
#[test]
fn trash_restore_counts_are_atomic_and_do_not_overwrite_timestamps() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("db.sqlite")).unwrap();
    assert_eq!(
        execute(
            &db,
            Request::Trash {
                ids: vec!["1".into(), "mms_2".into(), "1".into()]
            }
        )
        .unwrap()["count"],
        2
    );
    let before = db
        .with_conn(|c| {
            c.query_row(
                "SELECT trashed_at FROM trashed_sms WHERE message_id='1'",
                [],
                |r| r.get::<_, String>(0),
            )
        })
        .unwrap();
    assert_eq!(
        execute(
            &db,
            Request::Trash {
                ids: vec!["1".into()]
            }
        )
        .unwrap()["count"],
        0
    );
    let after = db
        .with_conn(|c| {
            c.query_row(
                "SELECT trashed_at FROM trashed_sms WHERE message_id='1'",
                [],
                |r| r.get::<_, String>(0),
            )
        })
        .unwrap();
    assert_eq!(before, after);
    assert_eq!(
        execute(
            &db,
            Request::Restore {
                ids: vec!["1".into(), "missing".into(), "1".into()]
            }
        )
        .unwrap()["count"],
        1
    );
    assert_eq!(db.trashed_message_ids().unwrap(), vec!["mms_2"]);
}
#[test]
fn archive_replaces_date_and_rejects_invalid_time() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("db.sqlite")).unwrap();
    execute(
        &db,
        Request::Archive {
            id: "thread".into(),
            date: "2026-10-05T01:00:00Z".into(),
        },
    )
    .unwrap();
    execute(
        &db,
        Request::Archive {
            id: "thread".into(),
            date: "2026-10-05T02:00:00Z".into(),
        },
    )
    .unwrap();
    assert_eq!(db.archived_conversation_list().unwrap().len(), 1);
    assert!(
        execute(
            &db,
            Request::Archive {
                id: "thread".into(),
                date: "invalid".into()
            }
        )
        .is_err()
    );
    assert_eq!(
        execute(
            &db,
            Request::Unarchive {
                id: "thread".into()
            }
        )
        .unwrap()["count"],
        1
    );
}
