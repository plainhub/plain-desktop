pub use crate::filesystem::tasks::{FileTask, FileTaskOp, FileTaskStatus, FileTaskType};
use crate::{
    filesystem::tasks::{Events, Service, Store},
    media::{
        eventbus::{EVENT_FILE_TASK_PROGRESS, EventBus},
        kv::get_default,
    },
};
use anyhow::{Result, anyhow, bail};
use std::sync::{Arc, Mutex};

const NS: &str = "filetask:";
fn client_prefix(client_id: &str) -> String {
    format!(
        "{NS}{}:",
        crate::utils::hex::bytes_to_hex(client_id.as_bytes())
    )
}
fn db_key(client_id: &str, task_id: &str) -> String {
    format!("{}{task_id}", client_prefix(client_id))
}
struct KvStore;
impl Store for KvStore {
    fn put(&self, task: &FileTask) -> Result<()> {
        get_default().insert(db_key(&task.client_id, &task.id), serde_json::to_vec(task)?)?;
        Ok(())
    }
    fn list(&self, client_id: &str) -> Result<Vec<FileTask>> {
        let mut tasks = Vec::new();
        for row in get_default().scan_prefix(client_prefix(client_id)) {
            let (_, value) = row?;
            let task: FileTask = serde_json::from_slice(&value)?;
            if task.client_id != client_id {
                bail!("file task owner mismatch");
            }
            tasks.push(task);
        }
        Ok(tasks)
    }
}
struct ProgressEvents;
impl Events for ProgressEvents {
    fn changed(&self, t: &FileTask) {
        EventBus::global().publish_with_cid(EVENT_FILE_TASK_PROGRESS,&t.client_id,serde_json::json!({
            "id":t.id,"type":t.kind,"title":t.title,"status":t.status,"error":t.error,
            "totalBytes":t.total_bytes,"doneBytes":t.done_bytes,"totalItems":t.total_items,"doneItems":t.done_items,"createdAt":t.created_at,"updatedAt":t.updated_at,
        }));
    }
}
static MGR: Mutex<Option<Arc<Service>>> = Mutex::new(None);
fn get_manager() -> Result<Arc<Service>> {
    let mut manager = MGR
        .lock()
        .map_err(|_| anyhow!("file task manager poisoned"))?;
    if let Some(service) = manager.as_ref() {
        return Ok(service.clone());
    }
    tokio::runtime::Handle::try_current()?;
    let service = Arc::new(Service::new(Arc::new(KvStore), Arc::new(ProgressEvents)));
    *manager = Some(service.clone());
    Ok(service)
}
pub fn create(
    client_id: &str,
    kind: FileTaskType,
    title: &str,
    ops: Vec<FileTaskOp>,
) -> Result<FileTask> {
    get_manager()?.create(client_id, kind, title, ops)
}
pub fn create_copy_task(client_id: &str, ops: Vec<FileTaskOp>) -> Result<FileTask> {
    create(client_id, FileTaskType::Copy, "Copy files", ops)
}
pub fn create_move_task(client_id: &str, ops: Vec<FileTaskOp>) -> Result<FileTask> {
    create(client_id, FileTaskType::Move, "Move files", ops)
}
pub fn list_tasks(client_id: &str) -> Result<Vec<FileTask>> {
    get_manager()?.list(client_id)
}

#[cfg(test)]
#[path = "../../tests/unit/media/file_tasks.rs"]
mod tests;
