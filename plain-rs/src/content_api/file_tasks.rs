use super::host::Host;
use crate::{
    db::Db,
    filesystem::tasks::{
        self, CompletedOp, Events, FileTask, FileTaskOp, FileTaskStatus, FileTaskType, HookResult,
        Hooks, Service,
    },
    ws_event::WsEvent,
};
use anyhow::{Result, anyhow, bail};
use serde_json::json;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tokio::sync::broadcast;
pub struct FileTasks {
    db: Arc<Db>,
    host: Arc<Host>,
    events: broadcast::Sender<WsEvent>,
    service: Mutex<Option<Arc<Service>>>,
}
impl FileTasks {
    pub fn new(db: Arc<Db>, host: Arc<Host>, events: broadcast::Sender<WsEvent>) -> Self {
        Self {
            db,
            host,
            events,
            service: Mutex::new(None),
        }
    }
    fn service(&self) -> Result<Arc<Service>> {
        let mut service = self
            .service
            .lock()
            .map_err(|_| anyhow!("file task service poisoned"))?;
        if let Some(service) = service.as_ref() {
            return Ok(service.clone());
        }
        tokio::runtime::Handle::try_current()?;
        let next = Arc::new(Service::with_hooks(
            Arc::new(tasks::sqlite::SqliteStore(self.db.clone())),
            Arc::new(Changes(self.events.clone())),
            Arc::new(NativeHooks(self.host.clone())),
        ));
        *service = Some(next.clone());
        Ok(next)
    }
    pub async fn create(
        &self,
        client_id: &str,
        kind: FileTaskType,
        title: &str,
        ops: Vec<FileTaskOp>,
    ) -> Result<FileTask> {
        if client_id.is_empty() {
            bail!("unauthorized");
        }
        if ops.is_empty()
            || ops.iter().any(|op| {
                !PathBuf::from(&op.src).is_absolute() || !PathBuf::from(&op.dst).is_absolute()
            })
        {
            bail!("absolute file paths required");
        }
        NativeHooks(self.host.clone()).authorize(kind, &ops).await?;
        self.service()?.create(client_id, kind, title, ops)
    }
    pub async fn remove(&self, client_id: String, id: String) -> Result<bool> {
        let service = self.service()?;
        tokio::task::spawn_blocking(move || service.remove(&client_id, &id)).await?
    }
    pub async fn list(&self, client_id: String) -> Result<Vec<FileTask>> {
        let service = self.service()?;
        tokio::task::spawn_blocking(move || service.list(&client_id)).await?
    }
}
struct Changes(broadcast::Sender<WsEvent>);
impl Events for Changes {
    fn changed(&self, task: &FileTask) {
        if matches!(task.status, FileTaskStatus::Done | FileTaskStatus::Error) {
            let _ = self.0.send(WsEvent::broadcast(47, "{}".into()));
        }
    }
}
struct NativeHooks(Arc<Host>);
impl NativeHooks {
    async fn call(&self, method: &str, params: serde_json::Value) -> Result<()> {
        if self
            .0
            .call(method, params)
            .await
            .map_err(anyhow::Error::msg)?
            .as_bool()
            != Some(true)
        {
            bail!("invalid file host receipt");
        }
        Ok(())
    }
}
impl Hooks for NativeHooks {
    fn authorize<'a>(&'a self, kind: FileTaskType, ops: &'a [FileTaskOp]) -> HookResult<'a> {
        Box::pin(async move {
            self.call("fileTaskAuthorize",json!({"type":kind,"paths":ops.iter().flat_map(|op| [&op.src,&op.dst]).collect::<Vec<_>>()})).await
        })
    }
    fn completed<'a>(&'a self, kind: FileTaskType, op: &'a CompletedOp) -> HookResult<'a> {
        Box::pin(async move {
            let root = PathBuf::from(&op.dst);
            let mut directories = Vec::<tokio::fs::ReadDir>::new();
            let mut next = Some(root.clone());
            let mut batch = Vec::new();
            loop {
                let path = if let Some(path) = next.take() {
                    Some(path)
                } else {
                    loop {
                        let Some(directory) = directories.last_mut() else {
                            break None;
                        };
                        if let Some(entry) = directory.next_entry().await? {
                            break Some(entry.path());
                        }
                        directories.pop();
                    }
                };
                let Some(path) = path else {
                    break;
                };
                let info = tokio::fs::symlink_metadata(&path).await?;
                if info.is_dir() {
                    directories.push(tokio::fs::read_dir(&path).await?);
                } else if info.is_file() {
                    batch.push(
                        path.to_str()
                            .ok_or_else(|| anyhow!("invalid path encoding"))?
                            .to_owned(),
                    );
                    if kind == FileTaskType::Move {
                        let relative = path.strip_prefix(&root)?;
                        let source = if relative.as_os_str().is_empty() {
                            PathBuf::from(&op.src)
                        } else {
                            PathBuf::from(&op.src).join(relative)
                        };
                        batch.push(
                            source
                                .to_str()
                                .ok_or_else(|| anyhow!("invalid path encoding"))?
                                .to_owned(),
                        );
                    }
                    if batch.len() >= 128 {
                        self.call("fileTaskScan", json!({"paths":batch})).await?;
                        batch.clear();
                    }
                }
            }
            if !batch.is_empty() {
                self.call("fileTaskScan", json!({"paths":batch})).await?;
            }
            Ok(())
        })
    }
}
#[cfg(test)]
#[path = "../../tests/unit/content_api/file_tasks.rs"]
mod tests;
