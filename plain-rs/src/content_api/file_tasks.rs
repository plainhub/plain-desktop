use super::host::Host;
use crate::{
    db::Db,
    filesystem::tasks::{
        self, CompletedOp, Events, FileTask, FileTaskOp, FileTaskStatus, FileTaskType, HookResult,
        Hooks, PrepareResult, Service,
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
    events: broadcast::Sender<WsEvent>,
    service: Mutex<Option<Arc<Service>>>,
    hooks: Arc<NativeHooks>,
}
impl FileTasks {
    pub fn new(
        db: Arc<Db>,
        host: Arc<Host>,
        events: broadcast::Sender<WsEvent>,
        audio: Arc<super::audio::Audio>,
        index: Arc<super::image_index::ImageIndex>,
        prefs: Arc<crate::prefs::Prefs>,
    ) -> Self {
        Self {
            hooks: Arc::new(NativeHooks {
                host: host.clone(),
                audio,
                index,
                prefs,
            }),
            db,
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
            self.hooks.clone(),
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
        self.hooks.authorize(kind, &ops).await?;
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
struct NativeHooks {
    host: Arc<Host>,
    audio: Arc<super::audio::Audio>,
    index: Arc<super::image_index::ImageIndex>,
    prefs: Arc<crate::prefs::Prefs>,
}
impl NativeHooks {
    async fn call(&self, method: &str, params: serde_json::Value) -> Result<()> {
        if self
            .host
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
    fn prepare<'a>(&'a self, kind: FileTaskType, op: &'a FileTaskOp) -> PrepareResult<'a> {
        Box::pin(async move {
            if kind == FileTaskType::Move {
                super::file_task_media::prepare(&self.host, &op.src).await
            } else {
                Ok(serde_json::Value::Null)
            }
        })
    }
    fn completed_with_snapshot<'a>(
        &'a self,
        kind: FileTaskType,
        op: &'a CompletedOp,
        snapshot: &'a serde_json::Value,
    ) -> HookResult<'a> {
        Box::pin(async move {
            self.completed(kind, op).await?;
            if kind == FileTaskType::Move {
                let change = super::file_task_media::resolve(&self.host, &op.dst, snapshot).await?;
                let raw_source = op.src.clone();
                let raw_destination = op.dst.clone();
                let prefs = self.prefs.clone();
                let index = self.index.clone();
                self.audio
                    .run(move |db, engine| {
                        use crate::library::{audio_commands, audio_playback, media_moves};
                        let before = audio_playback::snapshot(db)?;
                        let source_prefix =
                            format!("{}/", change.source_root.trim_end_matches('/'));
                        let moved_audio = !before.path.is_empty()
                            && (before.path == change.source_root
                                || before.path.starts_with(&source_prefix)
                                || before.path == raw_source
                                || before.path.starts_with(&format!(
                                    "{}/",
                                    raw_source.trim_end_matches('/')
                                )));
                        let rebind = |db: &Db| {
                            media_moves::rebind_with_aliases(
                                db,
                                &change.bindings,
                                &change.source_root,
                                &change.destination_root,
                                &[(raw_source, raw_destination)],
                            )
                        };
                        index.update_cache(rebind)?;
                        if moved_audio {
                            audio_commands::command(
                                db,
                                &prefs,
                                engine,
                                audio_commands::Action::Clear,
                                0,
                                1.0,
                            )?;
                        }
                        Ok(())
                    })
                    .await
                    .map_err(|e| anyhow!(e.message))?;
            }
            Ok(())
        })
    }
    fn authorize<'a>(&'a self, kind: FileTaskType, ops: &'a [FileTaskOp]) -> HookResult<'a> {
        Box::pin(async move {
            self.call("fileTaskAuthorize",json!({"type":kind,"paths":ops.iter().flat_map(|op| [&op.src,&op.dst]).collect::<Vec<_>>()})).await
        })
    }
    fn completed<'a>(&'a self, kind: FileTaskType, op: &'a CompletedOp) -> HookResult<'a> {
        Box::pin(async move {
            let root = PathBuf::from(&op.dst);
            let mut walker = super::file_task_walk::FileWalker::new(&root);
            let mut batch = Vec::new();
            while let Some(path) = walker.next().await? {
                let info = tokio::fs::symlink_metadata(&path).await?;
                if info.is_file() {
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

#[path = "file_mutations.rs"]
mod mutations;
