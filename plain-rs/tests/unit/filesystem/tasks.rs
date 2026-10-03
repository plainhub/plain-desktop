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
