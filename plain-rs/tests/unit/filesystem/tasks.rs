use super::*;
use std::sync::atomic::{AtomicBool, Ordering};
#[derive(Default)]
struct MemoryStore {
    tasks: Mutex<HashMap<String, FileTask>>,
    fail: AtomicBool,
    fail_done: AtomicBool,
}
impl Store for MemoryStore {
    fn remove(&self, client_id: &str, id: &str) -> Result<bool> {
        let mut tasks = self.tasks.lock().unwrap();
        if tasks.get(id).is_some_and(|task| {
            task.client_id == client_id
                && matches!(task.status, FileTaskStatus::Done | FileTaskStatus::Error)
        }) {
            tasks.remove(id);
            Ok(true)
        } else {
            Ok(false)
        }
    }
    fn put(&self, t: &FileTask) -> Result<()> {
        if self.fail.load(Ordering::SeqCst)
            || (t.status == FileTaskStatus::Done && self.fail_done.load(Ordering::SeqCst))
        {
            bail!("synthetic store failure");
        }
        self.tasks.lock().unwrap().insert(t.id.clone(), t.clone());
        Ok(())
    }
    fn list(&self, cid: &str) -> Result<Vec<FileTask>> {
        if self.fail.load(Ordering::SeqCst) {
            bail!("synthetic store failure");
        }
        Ok(self
            .tasks
            .lock()
            .unwrap()
            .values()
            .filter(|t| t.client_id == cid)
            .cloned()
            .collect())
    }
}
#[derive(Default)]
struct Recorder {
    events: Mutex<Vec<FileTask>>,
}
impl Events for Recorder {
    fn changed(&self, t: &FileTask) {
        self.events.lock().unwrap().push(t.clone());
    }
}
fn op(src: &Path, dst: &Path) -> FileTaskOp {
    FileTaskOp {
        src: src.to_str().unwrap().into(),
        dst: dst.to_str().unwrap().into(),
        overwrite: false,
    }
}
async fn terminal(service: &Service, cid: &str, id: &str) -> FileTask {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let task = service
                .list(cid)
                .unwrap()
                .into_iter()
                .find(|t| t.id == id)
                .unwrap();
            if matches!(task.status, FileTaskStatus::Done | FileTaskStatus::Error) {
                break task;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap()
}
#[tokio::test]
async fn owned_services_execute_multi_op_copy_and_directory_move_with_exact_cumulative_progress() {
    let store = Arc::new(MemoryStore::default());
    let events = Arc::new(Recorder::default());
    let service = Service::new(store, events.clone());
    let temp = tempfile::tempdir().unwrap();
    let a = temp.path().join("a");
    let b = temp.path().join("b");
    std::fs::write(&a, vec![7; 200_000]).unwrap();
    std::fs::write(&b, b"two").unwrap();
    let copy = service
        .create(
            "owner",
            FileTaskType::Copy,
            "copy",
            vec![
                op(&a, &temp.path().join("a-copy")),
                op(&b, &temp.path().join("b-copy")),
            ],
        )
        .unwrap();
    let done = terminal(&service, "owner", &copy.id).await;
    assert_eq!(done.status, FileTaskStatus::Done);
    assert_eq!(
        (
            done.total_bytes,
            done.done_bytes,
            done.total_items,
            done.done_items
        ),
        (200_003, 200_003, 2, 2)
    );
    assert_eq!(
        std::fs::read(temp.path().join("a-copy")).unwrap(),
        vec![7; 200_000]
    );
    assert_eq!(std::fs::read(temp.path().join("b-copy")).unwrap(), b"two");
    let directory = temp.path().join("dir");
    std::fs::create_dir_all(directory.join("empty")).unwrap();
    std::fs::write(directory.join("file"), b"directory").unwrap();
    let moved = service
        .create(
            "owner",
            FileTaskType::Move,
            "move",
            vec![op(&directory, &temp.path().join("moved"))],
        )
        .unwrap();
    let done = terminal(&service, "owner", &moved.id).await;
    assert_eq!(done.status, FileTaskStatus::Done);
    assert_eq!((done.done_bytes, done.done_items), (9, 1));
    assert!(!directory.exists());
    assert_eq!(
        std::fs::read(temp.path().join("moved/file")).unwrap(),
        b"directory"
    );
    assert!(temp.path().join("moved/empty").is_dir());
    assert!(service.list("other").unwrap().is_empty());
    let snapshots = events.events.lock().unwrap();
    let events = snapshots
        .iter()
        .filter(|t| t.id == copy.id)
        .collect::<Vec<_>>();
    assert_eq!(events.first().unwrap().status, FileTaskStatus::Queued);
    assert_eq!(events.last().unwrap().status, FileTaskStatus::Done);
    assert!(
        events
            .windows(2)
            .all(|pair| pair[1].done_bytes >= pair[0].done_bytes)
    );
}

fn interrupted_cross_volume_move(
    id: &str,
    source: &Path,
    destination: &Path,
    source_evidence: evidence::Evidence,
    destination_evidence: evidence::Evidence,
) -> FileTask {
    let now = Utc::now();
    FileTask {
        id: id.into(),
        client_id: "owner".into(),
        kind: FileTaskType::Move,
        title: "move".into(),
        status: FileTaskStatus::Error,
        error: "source cleanup interrupted".into(),
        total_bytes: 8,
        done_bytes: 8,
        total_items: 2,
        done_items: 2,
        created_at: now,
        updated_at: now,
        completed_ops: vec![CompletedOp {
            src: source.to_string_lossy().into_owned(),
            dst: destination.to_string_lossy().into_owned(),
            recovery: Some(Recovery {
                id: "receipt".into(),
                snapshot: serde_json::Value::Null,
                final_op: true,
                evidence: Some(destination_evidence),
                source_evidence: Some(source_evidence),
                source_cleanup_pending: true,
                physical_pending: false,
                destination_before: None,
            }),
        }],
        last_persist: None,
    }
}

#[tokio::test]
async fn cross_volume_move_recovery_finishes_partial_source_removal() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let destination = temp.path().join("destination");
    std::fs::create_dir(&source).unwrap();
    std::fs::create_dir(&destination).unwrap();
    std::fs::write(source.join("removed"), b"gone").unwrap();
    std::fs::write(source.join("remaining"), b"stay").unwrap();
    std::fs::write(destination.join("removed"), b"gone").unwrap();
    std::fs::write(destination.join("remaining"), b"stay").unwrap();
    let source_evidence = evidence::capture(source.clone()).await.unwrap();
    let destination_evidence = evidence::capture(destination.clone()).await.unwrap();
    evidence::verify_copy_pair(&source_evidence, &destination_evidence).unwrap();
    std::fs::remove_file(source.join("removed")).unwrap();

    #[cfg(feature = "content_api")]
    let store: Arc<dyn Store> = Arc::new(sqlite::SqliteStore(Arc::new(
        crate::db::Db::open(&temp.path().join("recovery.db")).unwrap(),
    )));
    #[cfg(not(feature = "content_api"))]
    let store: Arc<dyn Store> = Arc::new(MemoryStore::default());
    let task = interrupted_cross_volume_move(
        "partial-move",
        &source,
        &destination,
        source_evidence,
        destination_evidence,
    );
    store.put(&task).unwrap();
    let service = Service::new(store, Arc::new(Recorder::default()));
    service.recover("owner", &task.id).unwrap();
    let recovered = terminal(&service, "owner", &task.id).await;
    assert_eq!(recovered.status, FileTaskStatus::Done);
    assert!(!source.exists());
    assert_eq!(std::fs::read(destination.join("removed")).unwrap(), b"gone");
    assert_eq!(
        std::fs::read(destination.join("remaining")).unwrap(),
        b"stay"
    );
    assert!(recovered.completed_ops[0].recovery.is_none());
}

#[tokio::test]
async fn cross_volume_move_recovery_preserves_replaced_source_entries() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let destination = temp.path().join("destination");
    std::fs::create_dir(&source).unwrap();
    std::fs::create_dir(&destination).unwrap();
    std::fs::write(source.join("file"), b"original").unwrap();
    std::fs::write(destination.join("file"), b"original").unwrap();
    let source_evidence = evidence::capture(source.clone()).await.unwrap();
    let destination_evidence = evidence::capture(destination.clone()).await.unwrap();
    std::fs::remove_file(source.join("file")).unwrap();
    std::fs::write(source.join("file"), b"original").unwrap();

    let store = Arc::new(MemoryStore::default());
    let task = interrupted_cross_volume_move(
        "replaced-move",
        &source,
        &destination,
        source_evidence,
        destination_evidence,
    );
    store.put(&task).unwrap();
    let service = Service::new(store, Arc::new(Recorder::default()));
    service.recover("owner", &task.id).unwrap();
    let failed = terminal(&service, "owner", &task.id).await;
    assert_eq!(failed.status, FileTaskStatus::Error);
    assert!(failed.error.contains("source changed"));
    assert!(failed.completed_ops[0].recovery.is_some());
    assert_eq!(std::fs::read(source.join("file")).unwrap(), b"original");
    assert_eq!(
        std::fs::read(destination.join("file")).unwrap(),
        b"original"
    );
}
#[tokio::test]
async fn storage_errors_are_not_admitted_and_invalid_sources_finish_with_error() {
    let store = Arc::new(MemoryStore::default());
    let events = Arc::new(Recorder::default());
    let service = Service::new(store.clone(), events.clone());
    let temp = tempfile::tempdir().unwrap();
    store.fail.store(true, Ordering::SeqCst);
    assert!(
        service
            .create(
                "owner",
                FileTaskType::Copy,
                "copy",
                vec![op(&temp.path().join("absent"), &temp.path().join("out"))]
            )
            .is_err()
    );
    assert!(events.events.lock().unwrap().is_empty());
    assert!(service.list("owner").is_err());
    store.fail.store(false, Ordering::SeqCst);
    let task = service
        .create(
            "owner",
            FileTaskType::Copy,
            "copy",
            vec![op(&temp.path().join("absent"), &temp.path().join("out"))],
        )
        .unwrap();
    assert_eq!(
        terminal(&service, "owner", &task.id).await.status,
        FileTaskStatus::Error
    );
    assert!(!temp.path().join("out").exists());
}
#[tokio::test]
async fn stored_interrupted_tasks_become_errors_without_replaying_operations() {
    let store = Arc::new(MemoryStore::default());
    let service = Service::new(store.clone(), Arc::new(Recorder::default()));
    let now = Utc::now();
    let task = FileTask {
        id: "interrupted".into(),
        client_id: "owner".into(),
        kind: FileTaskType::Move,
        title: "move".into(),
        status: FileTaskStatus::Running,
        error: String::new(),
        total_bytes: 10,
        done_bytes: 5,
        total_items: 1,
        done_items: 0,
        created_at: now,
        updated_at: now,
        completed_ops: Vec::new(),
        last_persist: None,
    };
    store.put(&task).unwrap();
    let tasks = service.list("owner").unwrap();
    assert_eq!(tasks[0].status, FileTaskStatus::Error);
    assert_eq!(tasks[0].done_bytes, 5);
    assert_eq!(tasks[0].error, "file task interrupted");
}

#[tokio::test]
async fn bounded_admission_never_persists_a_rejected_task() {
    let store = Arc::new(MemoryStore::default());
    let events = Arc::new(Recorder::default());
    let service = Service::new(store.clone(), events.clone());
    let temp = tempfile::tempdir().unwrap();
    let mut admitted = 0;
    loop {
        match service.create(
            "owner",
            FileTaskType::Copy,
            "copy",
            vec![op(&temp.path().join("absent"), &temp.path().join("out"))],
        ) {
            Ok(_) => {
                admitted += 1;
                assert!(admitted < 128);
            }
            Err(error) => {
                assert!(error.to_string().contains("queue"));
                break;
            }
        }
    }
    assert_eq!(store.tasks.lock().unwrap().len(), admitted);
    assert_eq!(events.events.lock().unwrap().len(), admitted);
}
#[test]
fn a_closed_worker_queue_rejects_new_work_without_queued_records() {
    let store = Arc::new(MemoryStore::default());
    let events = Arc::new(Recorder::default());
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let service = runtime.block_on(async { Service::new(store.clone(), events.clone()) });
    drop(runtime);
    assert!(
        service
            .create(
                "owner",
                FileTaskType::Copy,
                "copy",
                vec![FileTaskOp {
                    src: "/synthetic/source".into(),
                    dst: "/synthetic/destination".into(),
                    overwrite: false
                }]
            )
            .is_err()
    );
    assert!(store.tasks.lock().unwrap().is_empty());
    assert!(events.events.lock().unwrap().is_empty());
}
#[tokio::test]
async fn completion_storage_failure_is_visible_in_active_error_state() {
    let store = Arc::new(MemoryStore::default());
    store.fail_done.store(true, Ordering::SeqCst);
    let service = Service::new(store.clone(), Arc::new(Recorder::default()));
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    std::fs::write(&source, b"synthetic").unwrap();
    let task = service
        .create(
            "owner",
            FileTaskType::Copy,
            "copy",
            vec![op(&source, &temp.path().join("destination"))],
        )
        .unwrap();
    let failed = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if let Some(t) = service.get(&task.id).unwrap() {
                if t.status == FileTaskStatus::Error {
                    break t;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    assert!(failed.error.contains("persistence"));
    assert_eq!(failed.done_items, 1);
    assert_eq!(
        std::fs::read(temp.path().join("destination")).unwrap(),
        b"synthetic"
    );
}

struct SlowStore(MemoryStore);
impl Store for SlowStore {
    fn remove(&self, client_id: &str, id: &str) -> Result<bool> {
        self.0.remove(client_id, id)
    }
    fn put(&self, task: &FileTask) -> Result<()> {
        self.0.put(task)
    }
    fn list(&self, client_id: &str) -> Result<Vec<FileTask>> {
        let tasks = self.0.list(client_id)?;
        std::thread::sleep(std::time::Duration::from_millis(1));
        Ok(tasks)
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_polling_never_changes_completed_tasks_to_interrupted() {
    let service = Service::new(
        Arc::new(SlowStore(MemoryStore::default())),
        Arc::new(Recorder::default()),
    );
    let temp = tempfile::tempdir().unwrap();
    for index in 0..16 {
        let source = temp.path().join(format!("source-{index}"));
        std::fs::write(&source, b"synthetic").unwrap();
        service
            .create(
                "owner",
                FileTaskType::Copy,
                "copy",
                vec![op(&source, &temp.path().join(format!("out-{index}")))],
            )
            .unwrap();
    }
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let tasks = service.list("owner").unwrap();
            assert_eq!(tasks.len(), 16);
            assert!(
                tasks
                    .iter()
                    .all(|task| task.status != FileTaskStatus::Error)
            );
            if tasks.iter().all(|task| task.status == FileTaskStatus::Done) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap();
    for index in 0..16 {
        assert_eq!(
            std::fs::read(temp.path().join(format!("out-{index}"))).unwrap(),
            b"synthetic"
        );
    }
}

struct ScanFailure;
impl Hooks for ScanFailure {
    fn authorize<'a>(&'a self, _: FileTaskType, _: &'a [FileTaskOp]) -> HookResult<'a> {
        Box::pin(async { Ok(()) })
    }
    fn completed<'a>(&'a self, _: FileTaskType, _: &'a CompletedOp) -> HookResult<'a> {
        Box::pin(async { bail!("synthetic scan failure") })
    }
}
#[tokio::test]
async fn completed_receipts_keep_resolved_collision_paths_when_native_scan_fails() {
    let service = Service::with_hooks(
        Arc::new(MemoryStore::default()),
        Arc::new(Recorder::default()),
        Arc::new(ScanFailure),
    );
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let target = temp.path().join("copy.txt");
    std::fs::write(&source, b"synthetic").unwrap();
    std::fs::write(&target, b"existing").unwrap();
    let task = service
        .create(
            "owner",
            FileTaskType::Copy,
            "copy",
            vec![op(&source, &target)],
        )
        .unwrap();
    let task = terminal(&service, "owner", &task.id).await;
    assert_eq!(task.status, FileTaskStatus::Error);
    assert!(task.error.contains("scan failure"));
    assert_eq!(task.completed_ops.len(), 1);
    assert_eq!(
        task.completed_ops[0].dst,
        temp.path().join("copy_1.txt").to_str().unwrap()
    );
    assert_eq!(std::fs::read(&target).unwrap(), b"existing");
    assert_eq!(
        std::fs::read(&task.completed_ops[0].dst).unwrap(),
        b"synthetic"
    );
    assert!(!service.remove("foreign", &task.id).unwrap());
    assert!(service.remove("owner", &task.id).unwrap());
    assert!(service.list("owner").unwrap().is_empty());
}
struct RevokedBeforeOperation(std::sync::atomic::AtomicUsize);
impl Hooks for RevokedBeforeOperation {
    fn authorize<'a>(&'a self, _: FileTaskType, _: &'a [FileTaskOp]) -> HookResult<'a> {
        Box::pin(async move {
            if self.0.fetch_add(1, Ordering::SeqCst) > 0 {
                bail!("synthetic permission revoked");
            }
            Ok(())
        })
    }
    fn completed<'a>(&'a self, _: FileTaskType, _: &'a CompletedOp) -> HookResult<'a> {
        Box::pin(async { Ok(()) })
    }
}
#[tokio::test]
async fn permission_revoked_after_measurement_prevents_physical_execution() {
    let service = Service::with_hooks(
        Arc::new(MemoryStore::default()),
        Arc::new(Recorder::default()),
        Arc::new(RevokedBeforeOperation(std::sync::atomic::AtomicUsize::new(
            0,
        ))),
    );
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let target = temp.path().join("target");
    std::fs::write(&source, b"synthetic").unwrap();
    let task = service
        .create(
            "owner",
            FileTaskType::Move,
            "move",
            vec![op(&source, &target)],
        )
        .unwrap();
    let failed = terminal(&service, "owner", &task.id).await;
    assert_eq!(failed.status, FileTaskStatus::Error);
    assert!(failed.error.contains("permission revoked"));
    assert!(failed.completed_ops.is_empty());
    assert_eq!(std::fs::read(&source).unwrap(), b"synthetic");
    assert!(!target.exists());
}
#[cfg(unix)]
#[tokio::test]
async fn special_files_fail_without_blocking_the_worker() {
    let service = Service::new(
        Arc::new(MemoryStore::default()),
        Arc::new(Recorder::default()),
    );
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("socket");
    let _socket = std::os::unix::net::UnixListener::bind(&source).unwrap();
    let target = temp.path().join("target");
    let task = service
        .create(
            "owner",
            FileTaskType::Copy,
            "copy",
            vec![op(&source, &target)],
        )
        .unwrap();
    let failed = terminal(&service, "owner", &task.id).await;
    assert_eq!(failed.status, FileTaskStatus::Error);
    assert!(failed.error.contains("unsupported file type"));
    assert!(!target.exists());
}
struct RecoverableHooks {
    fail: AtomicBool,
    denied: AtomicBool,
    calls: std::sync::atomic::AtomicUsize,
}
impl Hooks for RecoverableHooks {
    fn authorize<'a>(&'a self, _: FileTaskType, _: &'a [FileTaskOp]) -> HookResult<'a> {
        Box::pin(async move {
            if self.denied.load(Ordering::SeqCst) {
                bail!("revoked recovery permission");
            }
            Ok(())
        })
    }
    fn prepare<'a>(&'a self, _: FileTaskType, _: &'a FileTaskOp) -> PrepareResult<'a> {
        Box::pin(async { Ok(serde_json::json!({"originalMediaId":"original"})) })
    }
    fn completed_with_snapshot<'a>(
        &'a self,
        _: FileTaskType,
        _: &'a CompletedOp,
        snapshot: &'a serde_json::Value,
    ) -> HookResult<'a> {
        Box::pin(async move {
            assert_eq!(snapshot["originalMediaId"], "original");
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.fail.load(Ordering::SeqCst) {
                bail!("recoverable scanner failure");
            }
            Ok(())
        })
    }
    fn completed<'a>(&'a self, _: FileTaskType, _: &'a CompletedOp) -> HookResult<'a> {
        Box::pin(async { bail!("snapshot required") })
    }
}
#[cfg(feature = "content_api")]
#[tokio::test]
async fn persisted_receipt_recovers_after_restart_without_repeating_move_and_checks_owner_and_permission()
 {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let destination = temp.path().join("destination");
    std::fs::write(&source, b"fixture").unwrap();
    let store = Arc::new(sqlite::SqliteStore(Arc::new(
        crate::db::Db::open(&temp.path().join("db")).unwrap(),
    )));
    let hooks = Arc::new(RecoverableHooks {
        fail: AtomicBool::new(true),
        denied: AtomicBool::new(false),
        calls: std::sync::atomic::AtomicUsize::new(0),
    });
    let first = Service::with_hooks(store.clone(), Arc::new(Recorder::default()), hooks.clone());
    let queued = first
        .create(
            "owner",
            FileTaskType::Move,
            "move",
            vec![op(&source, &destination)],
        )
        .unwrap();
    let failed = terminal(&first, "owner", &queued.id).await;
    assert_eq!(failed.status, FileTaskStatus::Error);
    assert!(failed.completed_ops[0].recovery.is_some());
    assert!(!source.exists());
    drop(first);
    let reopened = Service::with_hooks(
        Arc::new(sqlite::SqliteStore(Arc::new(
            crate::db::Db::open(&temp.path().join("db")).unwrap(),
        ))),
        Arc::new(Recorder::default()),
        hooks.clone(),
    );
    assert!(reopened.recover("foreign", &queued.id).unwrap().is_none());
    hooks.fail.store(false, Ordering::SeqCst);
    hooks.denied.store(true, Ordering::SeqCst);
    reopened.recover("owner", &queued.id).unwrap();
    assert!(
        terminal(&reopened, "owner", &queued.id)
            .await
            .error
            .contains("permission")
    );
    hooks.denied.store(false, Ordering::SeqCst);
    reopened.recover("owner", &queued.id).unwrap();
    let recovered = terminal(&reopened, "owner", &queued.id).await;
    assert_eq!(recovered.status, FileTaskStatus::Done);
    assert!(recovered.completed_ops[0].recovery.is_none());
    assert_eq!(std::fs::read(&destination).unwrap(), b"fixture");
    assert!(!temp.path().join("destination_1").exists());
    assert_eq!(hooks.calls.load(Ordering::SeqCst), 2);
    reopened.recover("owner", &queued.id).unwrap();
    assert_eq!(hooks.calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn physical_move_intent_recovers_atomic_rename_before_first_receipt_write() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let destination = temp.path().join("destination");
    std::fs::write(&source, b"atomic move").unwrap();
    let source_evidence = evidence::capture(source.clone()).await.unwrap();
    let now = Utc::now();
    let task = FileTask {
        id: "interrupted-atomic-move".into(),
        client_id: "owner".into(),
        kind: FileTaskType::Move,
        title: "move".into(),
        status: FileTaskStatus::Error,
        error: "process interrupted".into(),
        total_bytes: 11,
        done_bytes: 11,
        total_items: 1,
        done_items: 1,
        created_at: now,
        updated_at: now,
        completed_ops: vec![CompletedOp {
            src: source.to_string_lossy().into_owned(),
            dst: destination.to_string_lossy().into_owned(),
            recovery: Some(Recovery {
                id: "pending-atomic-receipt".into(),
                snapshot: serde_json::json!({"originalMediaId":"original"}),
                final_op: true,
                evidence: None,
                source_evidence: Some(source_evidence),
                source_cleanup_pending: false,
                physical_pending: true,
                destination_before: None,
            }),
        }],
        last_persist: None,
    };
    let store = Arc::new(MemoryStore::default());
    store.put(&task).unwrap();
    std::fs::rename(&source, &destination).unwrap();
    let hooks = Arc::new(RecoverableHooks {
        fail: AtomicBool::new(false),
        denied: AtomicBool::new(false),
        calls: std::sync::atomic::AtomicUsize::new(0),
    });
    let service = Service::with_hooks(store, Arc::new(Recorder::default()), hooks.clone());

    service.recover("owner", &task.id).unwrap();
    let recovered = terminal(&service, "owner", &task.id).await;

    assert_eq!(recovered.status, FileTaskStatus::Done);
    assert!(recovered.completed_ops[0].recovery.is_none());
    assert_eq!(std::fs::read(&destination).unwrap(), b"atomic move");
    assert!(!source.exists());
    assert_eq!(hooks.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn pending_move_intent_refuses_changed_source_and_preserves_receipt() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let destination = temp.path().join("destination");
    std::fs::write(&source, b"original").unwrap();
    let source_evidence = evidence::capture(source.clone()).await.unwrap();
    std::fs::write(&source, b"changed!").unwrap();
    let now = Utc::now();
    let task = FileTask {
        id: "changed-source-intent".into(),
        client_id: "owner".into(),
        kind: FileTaskType::Move,
        title: "move".into(),
        status: FileTaskStatus::Error,
        error: "process interrupted".into(),
        total_bytes: 8,
        done_bytes: 8,
        total_items: 1,
        done_items: 1,
        created_at: now,
        updated_at: now,
        completed_ops: vec![CompletedOp {
            src: source.to_string_lossy().into_owned(),
            dst: destination.to_string_lossy().into_owned(),
            recovery: Some(Recovery {
                id: "pending-changed-receipt".into(),
                snapshot: serde_json::Value::Null,
                final_op: true,
                evidence: None,
                source_evidence: Some(source_evidence),
                source_cleanup_pending: false,
                physical_pending: true,
                destination_before: None,
            }),
        }],
        last_persist: None,
    };
    let store = Arc::new(MemoryStore::default());
    store.put(&task).unwrap();
    let service = Service::new(store, Arc::new(Recorder::default()));

    service.recover("owner", &task.id).unwrap();
    let recovered = terminal(&service, "owner", &task.id).await;

    assert_eq!(recovered.status, FileTaskStatus::Error);
    assert!(recovered.error.contains("destination"));
    assert!(recovered.completed_ops[0].recovery.is_some());
    assert_eq!(std::fs::read(&source).unwrap(), b"changed!");
    assert!(!destination.exists());
}

#[tokio::test]
async fn pending_copy_move_intent_never_deletes_source_without_a_persisted_copy_checkpoint() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let destination = temp.path().join("destination");
    std::fs::write(&source, b"copy completed before checkpoint").unwrap();
    let source_evidence = evidence::capture(source.clone()).await.unwrap();
    std::fs::write(&destination, b"copy completed before checkpoint").unwrap();
    let now = Utc::now();
    let task = FileTask {
        id: "ambiguous-copy-move".into(),
        client_id: "owner".into(),
        kind: FileTaskType::Move,
        title: "move".into(),
        status: FileTaskStatus::Error,
        error: "process interrupted".into(),
        total_bytes: 32,
        done_bytes: 32,
        total_items: 1,
        done_items: 1,
        created_at: now,
        updated_at: now,
        completed_ops: vec![CompletedOp {
            src: source.to_string_lossy().into_owned(),
            dst: destination.to_string_lossy().into_owned(),
            recovery: Some(Recovery {
                id: "ambiguous-copy-receipt".into(),
                snapshot: serde_json::Value::Null,
                final_op: true,
                evidence: None,
                source_evidence: Some(source_evidence),
                source_cleanup_pending: false,
                physical_pending: true,
                destination_before: None,
            }),
        }],
        last_persist: None,
    };
    let store = Arc::new(MemoryStore::default());
    store.put(&task).unwrap();
    let service = Service::new(store, Arc::new(Recorder::default()));

    service.recover("owner", &task.id).unwrap();
    let recovered = terminal(&service, "owner", &task.id).await;

    assert_eq!(recovered.status, FileTaskStatus::Error);
    assert!(recovered.error.contains("ambiguous"));
    assert!(recovered.completed_ops[0].recovery.is_some());
    assert_eq!(
        std::fs::read(&source).unwrap(),
        b"copy completed before checkpoint"
    );
    assert_eq!(
        std::fs::read(&destination).unwrap(),
        b"copy completed before checkpoint"
    );
}
#[tokio::test]
async fn recovering_one_completed_operation_never_claims_or_replays_unexecuted_operations() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("one");
    let second = temp.path().join("two");
    std::fs::write(&source, b"first").unwrap();
    std::fs::write(&second, b"second").unwrap();
    let hooks = Arc::new(RecoverableHooks {
        fail: AtomicBool::new(true),
        denied: AtomicBool::new(false),
        calls: std::sync::atomic::AtomicUsize::new(0),
    });
    let service = Service::with_hooks(
        Arc::new(MemoryStore::default()),
        Arc::new(Recorder::default()),
        hooks.clone(),
    );
    let task = service
        .create(
            "owner",
            FileTaskType::Move,
            "two operations",
            vec![
                op(&source, &temp.path().join("first-out")),
                op(&second, &temp.path().join("second-out")),
            ],
        )
        .unwrap();
    assert_eq!(
        terminal(&service, "owner", &task.id).await.status,
        FileTaskStatus::Error
    );
    hooks.fail.store(false, Ordering::SeqCst);
    service.recover("owner", &task.id).unwrap();
    let result = terminal(&service, "owner", &task.id).await;
    assert_eq!(result.status, FileTaskStatus::Error);
    assert!(result.error.contains("incomplete physical task"));
    assert!(second.exists());
    assert!(!temp.path().join("second-out").exists());
    assert!(result.completed_ops[0].recovery.is_none());
}

#[tokio::test]
async fn renamed_files_have_recoverable_receipts_and_never_choose_a_collision_name() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let destination = temp.path().join("renamed");
    std::fs::write(&source, b"rename fixture").unwrap();
    let hooks = Arc::new(RecoverableHooks {
        fail: AtomicBool::new(true),
        denied: AtomicBool::new(false),
        calls: std::sync::atomic::AtomicUsize::new(0),
    });
    let store = Arc::new(MemoryStore::default());
    let service = Service::with_hooks(store.clone(), Arc::new(Recorder::default()), hooks.clone());
    let queued = service
        .rename(
            "owner",
            source.to_str().unwrap().into(),
            destination.to_str().unwrap().into(),
        )
        .unwrap();
    let failed = terminal(&service, "owner", &queued.id).await;
    assert_eq!(failed.status, FileTaskStatus::Error);
    assert!(!source.exists());
    assert_eq!(failed.completed_ops[0].dst, destination.to_str().unwrap());
    drop(service);
    hooks.fail.store(false, Ordering::SeqCst);
    let service = Service::with_hooks(store, Arc::new(Recorder::default()), hooks.clone());
    service.recover("owner", &queued.id).unwrap();
    let done = terminal(&service, "owner", &queued.id).await;
    assert_eq!(done.status, FileTaskStatus::Done);
    assert_eq!(done.done_bytes, 14);
    assert!(done.completed_ops[0].recovery.is_none());
    assert!(!temp.path().join("renamed_1").exists());
    std::fs::write(&source, b"another source").unwrap();
    let collision = service
        .rename(
            "owner",
            source.to_str().unwrap().into(),
            destination.to_str().unwrap().into(),
        )
        .unwrap();
    let failed = terminal(&service, "owner", &collision.id).await;
    assert_eq!(failed.status, FileTaskStatus::Error);
    assert!(failed.completed_ops.is_empty());
    assert_eq!(std::fs::read(&destination).unwrap(), b"rename fixture");
    assert_eq!(std::fs::read(&source).unwrap(), b"another source");
    assert!(!temp.path().join("renamed_1").exists());
}

#[tokio::test]
async fn recovery_rejects_replaced_directory_files_links_and_recreated_move_sources() {
    for alteration in 0..5 {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let destination = temp.path().join("destination");
        std::fs::create_dir(&source).unwrap();
        std::fs::write(source.join("file"), b"before").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink("file", source.join("link")).unwrap();
        let hooks = Arc::new(RecoverableHooks {
            fail: AtomicBool::new(true),
            denied: AtomicBool::new(false),
            calls: std::sync::atomic::AtomicUsize::new(0),
        });
        let store = Arc::new(MemoryStore::default());
        let service =
            Service::with_hooks(store.clone(), Arc::new(Recorder::default()), hooks.clone());
        let task = service
            .create(
                "owner",
                FileTaskType::Move,
                "move",
                vec![op(&source, &destination)],
            )
            .unwrap();
        assert_eq!(
            terminal(&service, "owner", &task.id).await.status,
            FileTaskStatus::Error
        );
        drop(service);
        hooks.fail.store(false, Ordering::SeqCst);
        match alteration {
            0 => {
                let path = destination.join("file");
                let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
                std::fs::write(&path, b"edited").unwrap();
                std::fs::File::options()
                    .write(true)
                    .open(&path)
                    .unwrap()
                    .set_times(std::fs::FileTimes::new().set_modified(modified))
                    .unwrap();
            }
            1 => {
                std::fs::rename(destination.join("file"), destination.join("old")).unwrap();
                std::fs::write(destination.join("file"), b"before").unwrap();
                std::fs::remove_file(destination.join("old")).unwrap();
            }
            2 => {
                std::fs::create_dir(&source).unwrap();
            }
            4 => {
                std::fs::create_dir(destination.join("extra-empty")).unwrap();
            }
            _ => {
                #[cfg(unix)]
                {
                    std::fs::remove_file(destination.join("link")).unwrap();
                    std::os::unix::fs::symlink("other", destination.join("link")).unwrap();
                }
                #[cfg(not(unix))]
                {
                    std::fs::write(destination.join("extra"), b"extra").unwrap();
                }
            }
        }
        let service =
            Service::with_hooks(store.clone(), Arc::new(Recorder::default()), hooks.clone());
        service.recover("owner", &task.id).unwrap();
        let failed = terminal(&service, "owner", &task.id).await;
        assert_eq!(
            failed.status,
            FileTaskStatus::Error,
            "alteration {alteration}"
        );
        assert!(failed.completed_ops[0].recovery.is_some());
        assert_eq!(hooks.calls.load(Ordering::SeqCst), 1);
        assert!(!temp.path().join("destination_1").exists());
        assert!(
            failed.error.contains("changed") || failed.error.contains("source exists"),
            "{}",
            failed.error
        );
    }
}

#[tokio::test]
async fn recovery_with_missing_evidence_keeps_receipts_and_refuses_unverified_success() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let target = temp.path().join("target");
    std::fs::write(&source, b"fixture").unwrap();
    let hooks = Arc::new(RecoverableHooks {
        fail: AtomicBool::new(true),
        denied: AtomicBool::new(false),
        calls: std::sync::atomic::AtomicUsize::new(0),
    });
    let store = Arc::new(MemoryStore::default());
    let service = Service::with_hooks(store.clone(), Arc::new(Recorder::default()), hooks.clone());
    let task = service
        .create(
            "owner",
            FileTaskType::Copy,
            "copy",
            vec![op(&source, &target)],
        )
        .unwrap();
    let mut failed = terminal(&service, "owner", &task.id).await;
    failed.completed_ops[0].recovery.as_mut().unwrap().evidence = None;
    store.put(&failed).unwrap();
    drop(service);
    hooks.fail.store(false, Ordering::SeqCst);
    let service = Service::with_hooks(store, Arc::new(Recorder::default()), hooks.clone());
    service.recover("owner", &task.id).unwrap();
    let failed = terminal(&service, "owner", &task.id).await;
    assert_eq!(failed.status, FileTaskStatus::Error);
    assert!(failed.error.contains("evidence unavailable"));
    assert!(failed.completed_ops[0].recovery.is_some());
    assert_eq!(hooks.calls.load(Ordering::SeqCst), 1);
}

struct FailSecondReceipt(std::sync::atomic::AtomicUsize);
impl Hooks for FailSecondReceipt {
    fn authorize<'a>(&'a self, _: FileTaskType, _: &'a [FileTaskOp]) -> HookResult<'a> {
        Box::pin(async { Ok(()) })
    }
    fn completed<'a>(&'a self, _: FileTaskType, _: &'a CompletedOp) -> HookResult<'a> {
        Box::pin(async move {
            if self.0.fetch_add(1, Ordering::SeqCst) == 1 {
                bail!("second receipt failed");
            }
            Ok(())
        })
    }
}
#[tokio::test]
async fn recovery_persists_each_ack_before_a_later_receipt_fails() {
    let temp = tempfile::tempdir().unwrap();
    let store = Arc::new(MemoryStore::default());
    let creator = Service::with_hooks(
        store.clone(),
        Arc::new(Recorder::default()),
        Arc::new(ScanFailure),
    );
    let mut failed = Vec::new();
    for i in 0..2 {
        let source = temp.path().join(format!("source-{i}"));
        let target = temp.path().join(format!("target-{i}"));
        std::fs::write(&source, b"fixture").unwrap();
        let task = creator
            .create(
                "owner",
                FileTaskType::Copy,
                "copy",
                vec![op(&source, &target)],
            )
            .unwrap();
        failed.push(terminal(&creator, "owner", &task.id).await);
    }
    drop(creator);
    let mut combined = failed.remove(0);
    combined.completed_ops[0]
        .recovery
        .as_mut()
        .unwrap()
        .final_op = false;
    combined
        .completed_ops
        .push(failed[0].completed_ops[0].clone());
    combined.total_bytes = 14;
    combined.done_bytes = 14;
    combined.total_items = 2;
    combined.done_items = 2;
    store.put(&combined).unwrap();
    let hooks = Arc::new(FailSecondReceipt(std::sync::atomic::AtomicUsize::new(0)));
    let first = Service::with_hooks(store.clone(), Arc::new(Recorder::default()), hooks.clone());
    first.recover("owner", &combined.id).unwrap();
    let partial = terminal(&first, "owner", &combined.id).await;
    assert_eq!(partial.status, FileTaskStatus::Error);
    assert!(partial.completed_ops[0].recovery.is_none());
    assert!(partial.completed_ops[1].recovery.is_some());
    drop(first);
    let restarted = Service::with_hooks(store, Arc::new(Recorder::default()), hooks.clone());
    restarted.recover("owner", &combined.id).unwrap();
    let done = terminal(&restarted, "owner", &combined.id).await;
    assert_eq!(done.status, FileTaskStatus::Done);
    assert!(done.completed_ops.iter().all(|op| op.recovery.is_none()));
    assert_eq!(hooks.0.load(Ordering::SeqCst), 3);
}
