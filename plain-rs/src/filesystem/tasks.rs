mod evidence;
#[cfg(feature = "graphql")]
pub mod sqlite;
use anyhow::{Result, anyhow, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    sync::{Arc, Mutex},
};
use tokio::sync::mpsc;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "graphql", derive(async_graphql::Enum))]
pub enum FileTaskType {
    Copy,
    Move,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[cfg_attr(feature = "graphql", derive(async_graphql::Enum))]
pub enum FileTaskStatus {
    Queued,
    Running,
    Done,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileTaskOp {
    pub src: String,
    pub dst: String,
    pub overwrite: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletedOp {
    pub src: String,
    pub dst: String,
    pub recovery: Option<Recovery>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recovery {
    pub id: String,
    pub snapshot: serde_json::Value,
    pub final_op: bool,
    pub evidence: Option<evidence::Evidence>,
    #[serde(default)]
    pub source_evidence: Option<evidence::Evidence>,
    #[serde(default)]
    pub source_cleanup_pending: bool,
    #[serde(default)]
    pub physical_pending: bool,
    #[serde(default)]
    pub destination_before: Option<evidence::Evidence>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileTask {
    pub id: String,
    pub client_id: String,
    #[serde(rename = "type")]
    pub kind: FileTaskType,
    pub title: String,
    pub status: FileTaskStatus,
    pub error: String,
    pub total_bytes: i64,
    pub done_bytes: i64,
    pub total_items: i64,
    pub done_items: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_ops: Vec<CompletedOp>,
    #[serde(skip)]
    pub last_persist: Option<DateTime<Utc>>,
}

pub trait Store: Send + Sync + 'static {
    fn put(&self, task: &FileTask) -> Result<()>;
    fn list(&self, client_id: &str) -> Result<Vec<FileTask>>;
    fn remove(&self, client_id: &str, id: &str) -> Result<bool>;
}
pub trait Events: Send + Sync + 'static {
    fn changed(&self, task: &FileTask);
}
pub type HookResult<'a> = Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;
pub type PrepareResult<'a> = Pin<Box<dyn Future<Output = Result<serde_json::Value>> + Send + 'a>>;
pub trait Hooks: Send + Sync + 'static {
    fn prepare<'a>(&'a self, _: FileTaskType, _: &'a FileTaskOp) -> PrepareResult<'a> {
        Box::pin(async { Ok(serde_json::Value::Null) })
    }
    fn completed_with_snapshot<'a>(
        &'a self,
        kind: FileTaskType,
        op: &'a CompletedOp,
        _: &'a serde_json::Value,
    ) -> HookResult<'a> {
        self.completed(kind, op)
    }
    fn completed_receipt<'a>(&'a self, kind: FileTaskType, op: &'a CompletedOp) -> HookResult<'a> {
        Box::pin(async move {
            let recovery = op
                .recovery
                .as_ref()
                .ok_or_else(|| anyhow!("missing file recovery receipt"))?;
            self.completed_with_snapshot(kind, op, &recovery.snapshot)
                .await
        })
    }
    fn authorize<'a>(&'a self, kind: FileTaskType, ops: &'a [FileTaskOp]) -> HookResult<'a>;
    fn completed<'a>(&'a self, kind: FileTaskType, op: &'a CompletedOp) -> HookResult<'a>;
}
struct NoHooks;
impl Hooks for NoHooks {
    fn authorize<'a>(&'a self, _: FileTaskType, _: &'a [FileTaskOp]) -> HookResult<'a> {
        Box::pin(async { Ok(()) })
    }
    fn completed<'a>(&'a self, _: FileTaskType, _: &'a CompletedOp) -> HookResult<'a> {
        Box::pin(async { Ok(()) })
    }
}
struct Request {
    task: FileTask,
    ops: Vec<FileTaskOp>,
    recovery: bool,
    rename: bool,
}
type Active = Arc<Mutex<HashMap<String, FileTask>>>;
pub struct Service {
    queue: mpsc::Sender<Request>,
    active: Active,
    store: Arc<dyn Store>,
    events: Arc<dyn Events>,
}
impl Service {
    pub fn new(store: Arc<dyn Store>, events: Arc<dyn Events>) -> Self {
        Self::with_hooks(store, events, Arc::new(NoHooks))
    }
    pub fn with_hooks(
        store: Arc<dyn Store>,
        events: Arc<dyn Events>,
        hooks: Arc<dyn Hooks>,
    ) -> Self {
        let (queue, mut receiver) = mpsc::channel::<Request>(32);
        let active: Active = Arc::new(Mutex::new(HashMap::new()));
        let state = active.clone();
        let storage = store.clone();
        let emitter = events.clone();
        tokio::spawn(async move {
            while let Some(request) = receiver.recv().await {
                if request.recovery {
                    recover(request.task, &state, &storage, &emitter, &hooks).await;
                } else {
                    execute(request, &state, &storage, &emitter, &hooks).await;
                }
            }
        });
        Self {
            queue,
            active,
            store,
            events,
        }
    }
    pub fn create(
        &self,
        client_id: &str,
        kind: FileTaskType,
        title: &str,
        ops: Vec<FileTaskOp>,
    ) -> Result<FileTask> {
        self.admit(client_id, kind, title, ops, false)
    }
    pub fn rename(&self, client_id: &str, src: String, dst: String) -> Result<FileTask> {
        self.admit(
            client_id,
            FileTaskType::Move,
            "rename",
            vec![FileTaskOp {
                src,
                dst,
                overwrite: false,
            }],
            true,
        )
    }
    fn admit(
        &self,
        client_id: &str,
        kind: FileTaskType,
        title: &str,
        ops: Vec<FileTaskOp>,
        rename: bool,
    ) -> Result<FileTask> {
        if client_id.is_empty() {
            bail!("unauthorized");
        }
        if ops.is_empty() {
            bail!("no operations");
        }
        let permit = self
            .queue
            .try_reserve()
            .map_err(|e| anyhow!("file task queue: {e}"))?;
        let now = Utc::now();
        let task = FileTask {
            id: crate::utils::shortid::new_id(),
            client_id: client_id.into(),
            kind,
            title: title.into(),
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
        let mut active = self
            .active
            .lock()
            .map_err(|_| anyhow!("file task state poisoned"))?;
        self.store.put(&task)?;
        active.insert(task.id.clone(), task.clone());
        drop(active);
        self.events.changed(&task);
        permit.send(Request {
            task: task.clone(),
            ops,
            recovery: false,
            rename,
        });
        Ok(task)
    }
    pub fn recover(&self, client_id: &str, id: &str) -> Result<Option<FileTask>> {
        if client_id.is_empty() {
            bail!("unauthorized");
        }
        let permit = self
            .queue
            .try_reserve()
            .map_err(|e| anyhow!("file task queue: {e}"))?;
        let mut active = self
            .active
            .lock()
            .map_err(|_| anyhow!("file task state poisoned"))?;
        if let Some(task) = active.get(id) {
            if task.client_id != client_id {
                return Ok(None);
            }
            if matches!(
                task.status,
                FileTaskStatus::Running | FileTaskStatus::Queued
            ) {
                return Ok(Some(task.clone()));
            }
        }
        let mut task = match active
            .get(id)
            .filter(|t| t.client_id == client_id)
            .cloned()
            .or(self
                .store
                .list(client_id)?
                .into_iter()
                .find(|t| t.id == id && t.client_id == client_id))
        {
            Some(task) => task,
            None => return Ok(None),
        };
        if !task.completed_ops.iter().any(|op| op.recovery.is_some()) {
            return Ok(Some(task));
        }
        task.status = FileTaskStatus::Queued;
        task.updated_at = Utc::now();
        self.store.put(&task)?;
        active.insert(id.to_owned(), task.clone());
        drop(active);
        self.events.changed(&task);
        permit.send(Request {
            task: task.clone(),
            ops: Vec::new(),
            recovery: true,
            rename: false,
        });
        Ok(Some(task))
    }
    pub fn remove(&self, client_id: &str, id: &str) -> Result<bool> {
        if client_id.is_empty() {
            bail!("unauthorized");
        }
        let mut active = self
            .active
            .lock()
            .map_err(|_| anyhow!("file task state poisoned"))?;
        if let Some(task) = active.get(id) {
            if task.client_id != client_id {
                return Ok(false);
            }
            if matches!(
                task.status,
                FileTaskStatus::Queued | FileTaskStatus::Running
            ) {
                bail!("file task still active");
            }
            self.store.put(task)?;
        }
        let removed = self.store.remove(client_id, id)?;
        if removed {
            active.remove(id);
        }
        Ok(removed)
    }
    pub fn get(&self, id: &str) -> Result<Option<FileTask>> {
        Ok(self
            .active
            .lock()
            .map_err(|_| anyhow!("file task state poisoned"))?
            .get(id)
            .cloned())
    }
    pub fn list(&self, client_id: &str) -> Result<Vec<FileTask>> {
        if client_id.is_empty() {
            bail!("unauthorized");
        }
        let mut active = self
            .active
            .lock()
            .map_err(|_| anyhow!("file task state poisoned"))?;
        let stored = self.store.list(client_id)?;
        if stored.iter().any(|task| task.client_id != client_id) {
            bail!("file task owner mismatch");
        }
        let mut tasks = stored
            .into_iter()
            .map(|task| (task.id.clone(), task))
            .collect::<HashMap<_, _>>();
        for task in active.values().filter(|task| task.client_id == client_id) {
            tasks.insert(task.id.clone(), task.clone());
        }
        let mut tasks = tasks.into_values().collect::<Vec<_>>();
        for task in &mut tasks {
            if matches!(
                task.status,
                FileTaskStatus::Queued | FileTaskStatus::Running
            ) && (!active.contains_key(&task.id) || self.queue.is_closed())
            {
                task.status = FileTaskStatus::Error;
                task.error = "file task interrupted".into();
                task.updated_at = Utc::now();
                self.store.put(task)?;
                if active.contains_key(&task.id) {
                    active.insert(task.id.clone(), task.clone());
                }
            }
        }
        tasks.sort_by(|a, b| {
            b.updated_at
                .cmp(&a.updated_at)
                .then_with(|| a.id.cmp(&b.id))
        });
        Ok(tasks)
    }
}
fn save(
    task: &FileTask,
    active: &Active,
    store: &Arc<dyn Store>,
    events: &Arc<dyn Events>,
) -> Result<()> {
    let mut tasks = active
        .lock()
        .map_err(|_| anyhow!("file task state poisoned"))?;
    store.put(task)?;
    tasks.insert(task.id.clone(), task.clone());
    drop(tasks);
    events.changed(task);
    Ok(())
}
fn acknowledge(id: &str, receipt_id: &str, active: &Active, store: &Arc<dyn Store>) -> Result<()> {
    let mut tasks = active
        .lock()
        .map_err(|_| anyhow!("file task state poisoned"))?;
    let task = tasks
        .get_mut(id)
        .ok_or_else(|| anyhow!("file task disappeared"))?;
    let mut next = task.clone();
    for op in &mut next.completed_ops {
        if op
            .recovery
            .as_ref()
            .is_some_and(|receipt| receipt.id == receipt_id)
        {
            op.recovery = None;
        }
    }
    next.updated_at = Utc::now();
    store.put(&next)?;
    *task = next;
    Ok(())
}
fn record_completed(
    task_id: &str,
    completed: &CompletedOp,
    active: &Active,
    store: &Arc<dyn Store>,
) -> Result<()> {
    let mut tasks = active
        .lock()
        .map_err(|_| anyhow!("file task state poisoned"))?;
    let task = tasks
        .get_mut(task_id)
        .ok_or_else(|| anyhow!("file task disappeared"))?;
    task.completed_ops.push(completed.clone());
    task.updated_at = Utc::now();
    store.put(task)
}
fn record_move_intent(
    task_id: &str,
    src: String,
    dst: String,
    snapshot: serde_json::Value,
    source_evidence: evidence::Evidence,
    destination_before: Option<evidence::Evidence>,
    final_op: bool,
    active: &Active,
    store: &Arc<dyn Store>,
) -> Result<String> {
    let id = crate::utils::shortid::new_id();
    let mut tasks = active
        .lock()
        .map_err(|_| anyhow!("file task state poisoned"))?;
    let task = tasks
        .get_mut(task_id)
        .ok_or_else(|| anyhow!("file task disappeared"))?;
    let mut next = task.clone();
    next.completed_ops.push(CompletedOp {
        src,
        dst,
        recovery: Some(Recovery {
            id: id.clone(),
            snapshot,
            final_op,
            evidence: None,
            source_evidence: Some(source_evidence),
            source_cleanup_pending: false,
            physical_pending: true,
            destination_before,
        }),
    });
    next.updated_at = Utc::now();
    store.put(&next)?;
    *task = next;
    Ok(id)
}
fn update_move_intent(
    task_id: &str,
    receipt_id: &str,
    evidence: evidence::Evidence,
    source_cleanup_pending: bool,
    active: &Active,
    store: &Arc<dyn Store>,
) -> Result<CompletedOp> {
    let mut tasks = active
        .lock()
        .map_err(|_| anyhow!("file task state poisoned"))?;
    let task = tasks
        .get_mut(task_id)
        .ok_or_else(|| anyhow!("file task disappeared"))?;
    let mut next = task.clone();
    let op = next
        .completed_ops
        .iter_mut()
        .find(|op| {
            op.recovery
                .as_ref()
                .is_some_and(|recovery| recovery.id == receipt_id)
        })
        .ok_or_else(|| anyhow!("file recovery intent disappeared"))?;
    let recovery = op
        .recovery
        .as_mut()
        .ok_or_else(|| anyhow!("file recovery intent disappeared"))?;
    recovery.evidence = Some(evidence);
    recovery.source_cleanup_pending = source_cleanup_pending;
    recovery.physical_pending = false;
    next.updated_at = Utc::now();
    store.put(&next)?;
    let completed = next
        .completed_ops
        .iter()
        .find(|op| {
            op.recovery
                .as_ref()
                .is_some_and(|recovery| recovery.id == receipt_id)
        })
        .cloned()
        .ok_or_else(|| anyhow!("file recovery intent disappeared"))?;
    *task = next;
    Ok(completed)
}
fn discard_move_intent(
    task_id: &str,
    receipt_id: &str,
    active: &Active,
    store: &Arc<dyn Store>,
) -> Result<()> {
    let mut tasks = active
        .lock()
        .map_err(|_| anyhow!("file task state poisoned"))?;
    let task = tasks
        .get_mut(task_id)
        .ok_or_else(|| anyhow!("file task disappeared"))?;
    let mut next = task.clone();
    next.completed_ops.retain(|op| {
        !op.recovery
            .as_ref()
            .is_some_and(|recovery| recovery.id == receipt_id)
    });
    next.updated_at = Utc::now();
    store.put(&next)?;
    *task = next;
    Ok(())
}
fn move_intent_source_evidence(
    task_id: &str,
    receipt_id: &str,
    active: &Active,
) -> Result<evidence::Evidence> {
    active
        .lock()
        .map_err(|_| anyhow!("file task state poisoned"))?
        .get(task_id)
        .and_then(|task| {
            task.completed_ops.iter().find(|op| {
                op.recovery
                    .as_ref()
                    .is_some_and(|recovery| recovery.id == receipt_id)
            })
        })
        .and_then(|op| op.recovery.as_ref())
        .and_then(|recovery| recovery.source_evidence.clone())
        .ok_or_else(|| anyhow!("file recovery source evidence unavailable"))
}
fn set_source_cleanup_pending(
    task_id: &str,
    receipt_id: &str,
    pending: bool,
    active: &Active,
    store: &Arc<dyn Store>,
) -> Result<CompletedOp> {
    let mut tasks = active
        .lock()
        .map_err(|_| anyhow!("file task state poisoned"))?;
    let task = tasks
        .get_mut(task_id)
        .ok_or_else(|| anyhow!("file task disappeared"))?;
    let mut next = task.clone();
    let completed = next
        .completed_ops
        .iter_mut()
        .find(|op| {
            op.recovery
                .as_ref()
                .is_some_and(|recovery| recovery.id == receipt_id)
        })
        .ok_or_else(|| anyhow!("file recovery receipt disappeared"))?;
    let recovery = completed
        .recovery
        .as_mut()
        .ok_or_else(|| anyhow!("file recovery receipt disappeared"))?;
    recovery.source_cleanup_pending = pending;
    next.updated_at = Utc::now();
    store.put(&next)?;
    let completed = next
        .completed_ops
        .iter()
        .find(|op| {
            op.recovery
                .as_ref()
                .is_some_and(|recovery| recovery.id == receipt_id)
        })
        .cloned()
        .ok_or_else(|| anyhow!("file recovery receipt disappeared"))?;
    *task = next;
    Ok(completed)
}
async fn begin_move_intent(
    source: PathBuf,
    target: PathBuf,
    task_id: String,
    source_path: String,
    snapshot: serde_json::Value,
    final_op: bool,
    overwrite: bool,
    active: Active,
    store: Arc<dyn Store>,
) -> std::io::Result<String> {
    let source_evidence = evidence::capture(source.clone())
        .await
        .map_err(std::io::Error::other)?;
    let destination_before = if overwrite {
        match tokio::fs::symlink_metadata(&target).await {
            Ok(_) => Some(
                evidence::capture(target.clone())
                    .await
                    .map_err(std::io::Error::other)?,
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),
        }
    } else {
        None
    };
    record_move_intent(
        &task_id,
        source_path,
        target
            .to_str()
            .ok_or_else(|| std::io::Error::other("invalid destination encoding"))?
            .into(),
        snapshot,
        source_evidence,
        destination_before,
        final_op,
        &active,
        &store,
    )
    .map_err(std::io::Error::other)
}

async fn persist_cross_volume_move_checkpoint(
    source: PathBuf,
    target: PathBuf,
    task_id: String,
    receipt_id: String,
    active: Active,
    store: Arc<dyn Store>,
) -> std::io::Result<()> {
    let source_evidence = move_intent_source_evidence(&task_id, &receipt_id, &active)
        .map_err(std::io::Error::other)?;
    evidence::verify(source.clone(), source_evidence.clone(), true)
        .await
        .map_err(std::io::Error::other)?;
    let target_evidence = evidence::capture(target.clone())
        .await
        .map_err(std::io::Error::other)?;
    evidence::verify_copy_pair(&source_evidence, &target_evidence)
        .map_err(std::io::Error::other)?;
    update_move_intent(
        &task_id,
        &receipt_id,
        target_evidence,
        true,
        &active,
        &store,
    )
    .map_err(std::io::Error::other)?;
    evidence::verify_source_subset(source, source_evidence)
        .await
        .map_err(std::io::Error::other)
}
async fn reconcile_physical_intent(
    task_id: &str,
    op: CompletedOp,
    active: &Active,
    store: &Arc<dyn Store>,
) -> Result<CompletedOp> {
    let recovery = op
        .recovery
        .as_ref()
        .ok_or_else(|| anyhow!("file recovery intent disappeared"))?
        .clone();
    let source_evidence = recovery
        .source_evidence
        .clone()
        .ok_or_else(|| anyhow!("file recovery source evidence unavailable"))?;
    let source = Path::new(&op.src);
    let target = Path::new(&op.dst);
    let source_exists = match tokio::fs::symlink_metadata(source).await {
        Ok(_) => true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => return Err(error.into()),
    };
    let target_exists = match tokio::fs::symlink_metadata(target).await {
        Ok(_) => true,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => return Err(error.into()),
    };
    match (source_exists, target_exists) {
        (false, true) => {
            evidence::verify(target.to_owned(), source_evidence, true).await?;
            update_move_intent(
                task_id,
                &recovery.id,
                evidence::capture(target.to_owned()).await?,
                false,
                active,
                store,
            )
        }
        (true, false) => {
            evidence::verify(source.to_owned(), source_evidence, true).await?;
            discard_move_intent(task_id, &recovery.id, active, store)?;
            bail!("file move did not start before interruption")
        }
        (true, true) => {
            evidence::verify(source.to_owned(), source_evidence.clone(), true).await?;
            if let Some(previous_target) = recovery.destination_before {
                if evidence::verify(target.to_owned(), previous_target, true)
                    .await
                    .is_ok()
                {
                    discard_move_intent(task_id, &recovery.id, active, store)?;
                    bail!("file move did not replace its destination before interruption")
                }
            }
            bail!("file move intent is ambiguous because source and destination both exist")
        }
        (false, false) => bail!("file move source and destination are both missing"),
    }
}
async fn recover(
    mut task: FileTask,
    active: &Active,
    store: &Arc<dyn Store>,
    events: &Arc<dyn Events>,
    hooks: &Arc<dyn Hooks>,
) {
    let complete = task
        .completed_ops
        .last()
        .and_then(|op| op.recovery.as_ref())
        .is_some_and(|r| r.final_op);
    let result = async {
        task.status = FileTaskStatus::Running;
        save(&task, active, store, events)?;
        let recoverable = task
            .completed_ops
            .iter()
            .filter(|op| op.recovery.is_some())
            .cloned()
            .collect::<Vec<_>>();
        for mut op in recoverable {
            let paths = FileTaskOp {
                src: op.src.clone(),
                dst: op.dst.clone(),
                overwrite: false,
            };
            hooks
                .authorize(task.kind, std::slice::from_ref(&paths))
                .await?;
            let recovery = op.recovery.as_ref().unwrap().clone();
            if recovery.physical_pending {
                op = reconcile_physical_intent(&task.id, op, active, store).await?;
            }
            let recovery = op.recovery.as_ref().unwrap().clone();
            if recovery.source_cleanup_pending {
                evidence::verify(
                    Path::new(&op.dst).to_owned(),
                    recovery
                        .evidence
                        .clone()
                        .ok_or_else(|| anyhow!("file recovery evidence unavailable"))?,
                    true,
                )
                .await?;
                match tokio::fs::symlink_metadata(&op.src).await {
                    Ok(_) => {
                        evidence::verify_source_subset(
                            Path::new(&op.src).to_owned(),
                            recovery.source_evidence.clone().ok_or_else(|| {
                                anyhow!("file recovery source evidence unavailable")
                            })?,
                        )
                        .await?
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.into()),
                }
                super::remove(Path::new(&op.src)).await?;
                op = set_source_cleanup_pending(&task.id, &recovery.id, false, active, store)?;
            }
            evidence::verify(
                Path::new(&op.dst).to_owned(),
                op.recovery
                    .as_ref()
                    .unwrap()
                    .evidence
                    .clone()
                    .ok_or_else(|| anyhow!("file recovery evidence unavailable"))?,
                true,
            )
            .await?;
            validate_receipt(task.kind, &op).await?;
            hooks.completed_receipt(task.kind, &op).await?;
            acknowledge(&task.id, &op.recovery.as_ref().unwrap().id, active, store)?;
        }
        Ok::<_, anyhow::Error>(())
    }
    .await;
    if let Ok(tasks) = active.lock() {
        if let Some(current) = tasks.get(&task.id) {
            task = current.clone();
        }
    }
    match result {
        Ok(())
            if complete
                && task.total_bytes == task.done_bytes
                && task.total_items == task.done_items =>
        {
            task.status = FileTaskStatus::Done;
            task.error.clear();
        }
        Ok(()) => {
            task.status = FileTaskStatus::Error;
            task.error =
                "file postprocessing recovered; incomplete physical task was not replayed".into();
        }
        Err(error) => {
            task.status = FileTaskStatus::Error;
            task.error = error.to_string();
        }
    }
    task.updated_at = Utc::now();
    match save(&task, active, store, events) {
        Ok(()) => {
            if let Ok(mut tasks) = active.lock() {
                tasks.remove(&task.id);
            }
        }
        Err(error) => {
            task.status = FileTaskStatus::Error;
            task.error = format!("file task persistence: {error}");
            if let Ok(mut tasks) = active.lock() {
                tasks.insert(task.id.clone(), task.clone());
            }
            events.changed(&task);
        }
    }
}
async fn execute(
    request: Request,
    active: &Active,
    store: &Arc<dyn Store>,
    events: &Arc<dyn Events>,
    hooks: &Arc<dyn Hooks>,
) {
    let mut task = request.task;
    let result = async {
        hooks.authorize(task.kind, &request.ops).await?;
        let paths = request
            .ops
            .iter()
            .map(|op| op.src.clone())
            .collect::<Vec<_>>();
        let (bytes, items) = tokio::task::spawn_blocking(move || -> Result<(i64, i64)> {
            let mut bytes = 0_i64;
            let mut items = 0_i64;
            for path in paths {
                let (b, i) = super::measure(Path::new(&path))?;
                bytes = bytes
                    .checked_add(b)
                    .ok_or_else(|| anyhow!("file size overflow"))?;
                items = items
                    .checked_add(i)
                    .ok_or_else(|| anyhow!("file count overflow"))?;
            }
            Ok((bytes, items))
        })
        .await??;
        task.total_bytes = bytes;
        task.total_items = items;
        task.status = FileTaskStatus::Running;
        task.updated_at = Utc::now();
        task.last_persist = Some(task.updated_at);
        save(&task, active, store, events)?;
        let count = request.ops.len();
        for (operation_index, op) in request.ops.into_iter().enumerate() {
            hooks
                .authorize(task.kind, std::slice::from_ref(&op))
                .await?;
            let prepared = hooks.prepare(task.kind, &op).await?;
            hooks
                .authorize(task.kind, std::slice::from_ref(&op))
                .await?;
            let state = active.clone();
            let storage = store.clone();
            let emitter = events.clone();
            let id = task.id.clone();
            let progress_error = Arc::new(Mutex::new(None::<String>));
            let error_state = progress_error.clone();
            let progress = move |bytes: i64, items: i64| -> std::io::Result<()> {
                let mut tasks = state
                    .lock()
                    .map_err(|_| std::io::Error::other("file task state poisoned"))?;
                let task = tasks
                    .get_mut(&id)
                    .ok_or_else(|| std::io::Error::other("file task disappeared"))?;
                task.done_bytes = task
                    .done_bytes
                    .checked_add(bytes)
                    .ok_or_else(|| std::io::Error::other("file size overflow"))?;
                task.done_items = task
                    .done_items
                    .checked_add(items)
                    .ok_or_else(|| std::io::Error::other("file count overflow"))?;
                task.updated_at = Utc::now();
                if task
                    .last_persist
                    .is_none_or(|last| (task.updated_at - last).num_milliseconds() >= 200)
                {
                    task.last_persist = Some(task.updated_at);
                    if let Err(error) = storage.put(task) {
                        *error_state.lock().map_err(|_| {
                            std::io::Error::other("file task error state poisoned")
                        })? = Some(error.to_string());
                    }
                    let snapshot = task.clone();
                    drop(tasks);
                    emitter.changed(&snapshot);
                }
                Ok(())
            };
            let (destination, checkpointed_move, move_receipt_id) = if request.rename {
                let receipt_id = Arc::new(Mutex::new(None::<String>));
                let begin_receipt_id = receipt_id.clone();
                let begin_task_id = task.id.clone();
                let begin_source_path = op.src.clone();
                let begin_snapshot = prepared.clone();
                let begin_final = operation_index + 1 == count;
                let begin_active = active.clone();
                let begin_store = store.clone();
                let checkpoint_active = active.clone();
                let checkpoint_store = store.clone();
                let checkpoint_task_id = task.id.clone();
                let checkpointed = super::rename_with_intent(
                    Path::new(&op.src),
                    Path::new(&op.dst),
                    Some(&progress),
                    move |source, target| async move {
                        let id = begin_move_intent(
                            source,
                            target,
                            begin_task_id,
                            begin_source_path,
                            begin_snapshot,
                            begin_final,
                            false,
                            begin_active,
                            begin_store,
                        )
                        .await?;
                        *begin_receipt_id
                            .lock()
                            .map_err(|_| std::io::Error::other("file intent state poisoned"))? =
                            Some(id.clone());
                        Ok(id)
                    },
                    move |receipt_id, source, target| {
                        persist_cross_volume_move_checkpoint(
                            source,
                            target,
                            checkpoint_task_id,
                            receipt_id,
                            checkpoint_active,
                            checkpoint_store,
                        )
                    },
                )
                .await?;
                let receipt_id = receipt_id
                    .lock()
                    .map_err(|_| anyhow!("file intent state poisoned"))?
                    .clone()
                    .ok_or_else(|| anyhow!("file move intent missing"))?;
                (
                    Path::new(&op.dst).to_owned(),
                    checkpointed,
                    Some(receipt_id),
                )
            } else {
                match task.kind {
                    FileTaskType::Copy => (
                        super::copy_path_with_progress(
                            Path::new(&op.src),
                            Path::new(&op.dst),
                            op.overwrite,
                            Some(&progress),
                        )
                        .await?,
                        false,
                        None,
                    ),
                    FileTaskType::Move => {
                        let receipt_id = Arc::new(Mutex::new(None::<String>));
                        let begin_receipt_id = receipt_id.clone();
                        let begin_task_id = task.id.clone();
                        let begin_source_path = op.src.clone();
                        let begin_snapshot = prepared.clone();
                        let begin_final = operation_index + 1 == count;
                        let begin_overwrite = op.overwrite;
                        let begin_active = active.clone();
                        let begin_store = store.clone();
                        let checkpoint_active = active.clone();
                        let checkpoint_store = store.clone();
                        let checkpoint_task_id = task.id.clone();
                        let (destination, checkpointed) = super::move_path_with_intent(
                            Path::new(&op.src),
                            Path::new(&op.dst),
                            op.overwrite,
                            Some(&progress),
                            move |source, target| async move {
                                let id = begin_move_intent(
                                    source,
                                    target,
                                    begin_task_id,
                                    begin_source_path,
                                    begin_snapshot,
                                    begin_final,
                                    begin_overwrite,
                                    begin_active,
                                    begin_store,
                                )
                                .await?;
                                *begin_receipt_id.lock().map_err(|_| {
                                    std::io::Error::other("file intent state poisoned")
                                })? = Some(id.clone());
                                Ok(id)
                            },
                            move |receipt_id, source, target| {
                                persist_cross_volume_move_checkpoint(
                                    source,
                                    target,
                                    checkpoint_task_id,
                                    receipt_id,
                                    checkpoint_active,
                                    checkpoint_store,
                                )
                            },
                        )
                        .await?;
                        let receipt_id = receipt_id
                            .lock()
                            .map_err(|_| anyhow!("file intent state poisoned"))?
                            .clone()
                            .ok_or_else(|| anyhow!("file move intent missing"))?;
                        (destination, checkpointed, Some(receipt_id))
                    }
                }
            };
            let completed = if let Some(recovery_id) = move_receipt_id {
                if checkpointed_move {
                    set_source_cleanup_pending(&task.id, &recovery_id, false, active, store)?
                } else {
                    let target_evidence = evidence::capture(destination.clone()).await?;
                    update_move_intent(
                        &task.id,
                        &recovery_id,
                        target_evidence,
                        false,
                        active,
                        store,
                    )?
                }
            } else {
                let mut completed = CompletedOp {
                    src: op.src,
                    dst: destination
                        .to_str()
                        .ok_or_else(|| anyhow!("invalid destination encoding"))?
                        .into(),
                    recovery: Some(Recovery {
                        id: crate::utils::shortid::new_id(),
                        snapshot: prepared,
                        final_op: operation_index + 1 == count,
                        evidence: None,
                        source_evidence: None,
                        source_cleanup_pending: false,
                        physical_pending: false,
                        destination_before: None,
                    }),
                };
                record_completed(&task.id, &completed, active, store)?;
                completed.recovery.as_mut().unwrap().evidence =
                    Some(evidence::capture(destination).await?);
                {
                    let mut tasks = active
                        .lock()
                        .map_err(|_| anyhow!("file task state poisoned"))?;
                    let current = tasks
                        .get_mut(&task.id)
                        .ok_or_else(|| anyhow!("file task disappeared"))?;
                    *current
                        .completed_ops
                        .last_mut()
                        .ok_or_else(|| anyhow!("file receipt disappeared"))? = completed.clone();
                    store.put(current)?;
                }
                completed
            };
            validate_receipt(task.kind, &completed).await?;
            hooks.completed_receipt(task.kind, &completed).await?;
            acknowledge(
                &task.id,
                &completed.recovery.as_ref().unwrap().id,
                active,
                store,
            )?;
            if let Some(error) = progress_error
                .lock()
                .map_err(|_| anyhow!("file task error state poisoned"))?
                .take()
            {
                bail!("file task persistence: {error}");
            }
        }
        Ok::<_, anyhow::Error>(())
    }
    .await;
    let snapshot = active
        .lock()
        .ok()
        .and_then(|tasks| tasks.get(&task.id).cloned());
    if let Some(snapshot) = snapshot {
        task = snapshot;
    }
    task.updated_at = Utc::now();
    match result {
        Ok(()) if task.done_bytes == task.total_bytes && task.done_items == task.total_items => {
            task.status = FileTaskStatus::Done;
            task.error.clear();
        }
        Ok(()) => {
            task.status = FileTaskStatus::Error;
            task.error = "source changed during file task".into();
        }
        Err(error) => {
            task.status = FileTaskStatus::Error;
            task.error = error.to_string();
        }
    }
    if let Err(error) = save(&task, active, store, events) {
        task.status = FileTaskStatus::Error;
        task.error = format!("file task persistence: {error}");
        if let Ok(mut tasks) = active.lock() {
            tasks.insert(task.id.clone(), task.clone());
        }
        events.changed(&task);
    } else if let Ok(mut tasks) = active.lock() {
        tasks.remove(&task.id);
    }
}

pub async fn validate_receipt(kind: FileTaskType, op: &CompletedOp) -> Result<()> {
    let op = op.clone();
    tokio::task::spawn_blocking(move || validate_receipt_sync(kind, &op)).await?
}
pub fn validate_receipt_sync(kind: FileTaskType, op: &CompletedOp) -> Result<()> {
    let receipt = op
        .recovery
        .as_ref()
        .ok_or_else(|| anyhow!("missing file recovery receipt"))?;
    let evidence = receipt
        .evidence
        .as_ref()
        .ok_or_else(|| anyhow!("file recovery evidence unavailable"))?;
    evidence::verify_sync(Path::new(&op.dst), evidence, false)?;
    if kind == FileTaskType::Move {
        match std::fs::symlink_metadata(&op.src) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
            Ok(_) => bail!("file recovery source exists"),
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/unit/filesystem/tasks.rs"]
mod tests;
