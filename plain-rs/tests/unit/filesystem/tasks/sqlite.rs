use super::*;
use crate::filesystem::tasks::{CompletedOp, FileTaskStatus, FileTaskType};
#[test]
fn sqlite_tasks_preserve_exact_progress_receipts_and_owner_across_reopen() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("tasks.db");
    let now = chrono::Utc::now();
    let task = FileTask {
        id: "synthetic".into(),
        client_id: "owner:one".into(),
        kind: FileTaskType::Copy,
        title: "copy".into(),
        status: FileTaskStatus::Done,
        error: String::new(),
        total_bytes: 4_000_000_000,
        done_bytes: 4_000_000_000,
        total_items: 1,
        done_items: 1,
        created_at: now,
        updated_at: now,
        completed_ops: vec![CompletedOp {
            recovery: None,
            src: "/source/a".into(),
            dst: "/dest/a_1".into(),
        }],
        last_persist: None,
    };
    {
        SqliteStore(Arc::new(Db::open(&path).unwrap()))
            .put(&task)
            .unwrap();
    }
    let db = Arc::new(Db::open(&path).unwrap());
    let store = SqliteStore(db.clone());
    let rows = store.list("owner:one").unwrap();
    assert_eq!(rows[0].done_bytes, 4_000_000_000);
    assert_eq!(rows[0].completed_ops[0].dst, "/dest/a_1");
    assert!(store.list("owner").unwrap().is_empty());
    let foreign = FileTask {
        client_id: "foreign".into(),
        ..task.clone()
    };
    assert!(store.put(&foreign).is_err());
    assert!(!store.remove("foreign", &task.id).unwrap());
    db.with_conn(|c| {
        c.execute(
            "UPDATE file_tasks SET completed_ops='bad JSON' WHERE id='synthetic'",
            [],
        )
    })
    .unwrap();
    assert!(store.list("owner:one").is_err());
    store.put(&task).unwrap();
    assert!(store.remove("owner:one", &task.id).unwrap());
    assert!(store.list("owner:one").unwrap().is_empty());
}
#[test]
fn sqlite_task_admission_propagates_write_failures() {
    let db = Arc::new(Db::open(std::path::Path::new(":memory:")).unwrap());
    db.with_conn(|c| c.execute_batch("CREATE TRIGGER fail_task_insert BEFORE INSERT ON file_tasks BEGIN SELECT RAISE(FAIL,'synthetic failure'); END;")).unwrap();
    let now = chrono::Utc::now();
    let task = FileTask {
        id: "failed".into(),
        client_id: "owner".into(),
        kind: FileTaskType::Move,
        title: String::new(),
        status: FileTaskStatus::Queued,
        error: String::new(),
        total_bytes: 0,
        done_bytes: 0,
        total_items: 0,
        done_items: 0,
        created_at: now,
        updated_at: now,
        completed_ops: Vec::new(),
        last_persist: None,
    };
    let store = SqliteStore(db);
    assert!(store.put(&task).is_err());
    assert!(store.list("owner").unwrap().is_empty());
}
