use anyhow::{Result, anyhow, bail};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, Mutex},
};
use tokio::sync::mpsc;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FileTaskType {
    Copy,
    Move,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
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
    #[serde(skip)]
    pub last_persist: Option<DateTime<Utc>>,
}

pub trait Store: Send + Sync + 'static {
    fn put(&self, task: &FileTask) -> Result<()>;
    fn list(&self, client_id: &str) -> Result<Vec<FileTask>>;
}
pub trait Events: Send + Sync + 'static {
    fn changed(&self, task: &FileTask);
}
struct Request {
    task: FileTask,
    ops: Vec<FileTaskOp>,
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
        let (queue, mut receiver) = mpsc::channel::<Request>(32);
        let active: Active = Arc::new(Mutex::new(HashMap::new()));
        let state = active.clone();
        let storage = store.clone();
        let emitter = events.clone();
        tokio::spawn(async move {
            while let Some(request) = receiver.recv().await {
                execute(request, &state, &storage, &emitter).await;
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
        });
        Ok(task)
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
async fn execute(
    request: Request,
    active: &Active,
    store: &Arc<dyn Store>,
    events: &Arc<dyn Events>,
) {
    let mut task = request.task;
    let result = async {
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
        for op in request.ops {
            let state = active.clone();
            let storage = store.clone();
            let emitter = events.clone();
            let id = task.id.clone();
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
                    storage.put(task).map_err(std::io::Error::other)?;
                    let snapshot = task.clone();
                    drop(tasks);
                    emitter.changed(&snapshot);
                }
                Ok(())
            };
            match task.kind {
                FileTaskType::Copy => {
                    super::copy_path_with_progress(
                        Path::new(&op.src),
                        Path::new(&op.dst),
                        op.overwrite,
                        Some(&progress),
                    )
                    .await?;
                }
                FileTaskType::Move => {
                    super::move_path_with_progress(
                        Path::new(&op.src),
                        Path::new(&op.dst),
                        op.overwrite,
                        Some(&progress),
                    )
                    .await?;
                }
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

#[cfg(test)]
#[path = "../../tests/unit/filesystem/tasks.rs"]
mod tests;
