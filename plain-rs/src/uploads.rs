use crate::{app_files::FileStore, ws_event::WsEvent};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use tokio::{io::AsyncWriteExt, sync::broadcast};

pub enum Kind {
    File { path: PathBuf, replace: bool },
    AppFile { name: String },
}
type Completion = Arc<
    dyn Fn(PathBuf) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
        + Send
        + Sync,
>;
#[derive(Default)]
pub struct Runtime {
    jobs: Mutex<HashMap<String, Value>>,
    tasks: Mutex<HashMap<String, tokio::task::JoinHandle<()>>>,
    lifecycle: tokio::sync::Mutex<()>,
    completed: Option<Completion>,
}
impl Drop for Runtime {
    fn drop(&mut self) {
        for handle in self.tasks.get_mut().unwrap().values() {
            handle.abort();
        }
    }
}
pub fn chunk_dir(base: &Path, id: &str) -> anyhow::Result<PathBuf> {
    anyhow::ensure!(
        !id.is_empty() && id != "." && id != ".." && !id.contains(['/', '\\', '\0']),
        "Invalid fileId"
    );
    Ok(base.join(id))
}
pub async fn chunks(base: &Path, id: &str) -> anyhow::Result<Vec<(u32, u64)>> {
    let dir = chunk_dir(base, id)?;
    let mut reader = match tokio::fs::read_dir(dir).await {
        Ok(reader) => reader,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(error) => return Err(error.into()),
    };
    let mut items = vec![];
    while let Some(entry) = reader.next_entry().await? {
        let name = entry.file_name();
        if let Some(index) = name
            .to_str()
            .and_then(|name| name.strip_prefix("chunk_"))
            .and_then(|index| index.parse::<u32>().ok())
        {
            let meta = entry.metadata().await?;
            if meta.is_file() {
                items.push((index, meta.len()));
            }
        }
    }
    items.sort_by_key(|item| item.0);
    Ok(items)
}
pub async fn list(base: &Path, id: &str) -> anyhow::Result<Vec<String>> {
    Ok(chunks(base, id)
        .await?
        .into_iter()
        .map(|(index, size)| format!("{index}:{size}"))
        .collect())
}
pub fn chunk_path(base: &Path, id: &str, index: i64) -> anyhow::Result<PathBuf> {
    anyhow::ensure!(
        (0..=i64::from(i32::MAX)).contains(&index),
        "Invalid chunk index"
    );
    Ok(chunk_dir(base, id)?.join(format!("chunk_{index}")))
}
pub async fn save_chunk(
    base: &Path,
    id: &str,
    index: i64,
    bytes: &[u8],
) -> anyhow::Result<PathBuf> {
    let path = chunk_path(base, id, index)?;
    tokio::fs::create_dir_all(path.parent().unwrap()).await?;
    tokio::fs::write(&path, bytes).await?;
    Ok(path)
}

impl Runtime {
    pub fn with_completion(
        completed: impl Fn(PathBuf) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        let mut runtime = Self::default();
        runtime.completed = Some(Arc::new(completed));
        runtime
    }
    pub fn status(&self, id: &str) -> Value {
        self.jobs
            .lock()
            .unwrap()
            .get(id)
            .cloned()
            .unwrap_or_else(|| json!({"status":"NONE"}))
    }
    pub async fn delete(&self, base: &Path, id: &str) -> anyhow::Result<bool> {
        let dir = chunk_dir(base, id)?;
        let _guard = self.lifecycle.lock().await;
        let task = self.tasks.lock().unwrap().remove(id);
        if let Some(task) = task {
            task.abort();
            let _ = task.await;
        }
        self.jobs.lock().unwrap().remove(id);
        match tokio::fs::remove_dir_all(dir).await {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(true),
            Err(error) => Err(error.into()),
        }
    }
    pub async fn start(
        self: &Arc<Self>,
        store: Arc<FileStore>,
        base: PathBuf,
        id: String,
        count: i32,
        size: i64,
        kind: Kind,
        events: broadcast::Sender<WsEvent>,
    ) -> anyhow::Result<Value> {
        let dir = chunk_dir(&base, &id)?;
        anyhow::ensure!(count > 0 && size >= 0, "Invalid merge arguments");
        if let Kind::File { path, .. } = &kind {
            anyhow::ensure!(!path.as_os_str().is_empty(), "Destination is required");
        }
        let _guard = self.lifecycle.lock().await;
        let exists = tokio::fs::try_exists(&dir).await?;
        {
            let mut jobs = self.jobs.lock().unwrap();
            if let Some(value) = jobs
                .get(&id)
                .filter(|value| matches!(value["status"].as_str(), Some("DONE" | "MERGING")))
            {
                return Ok(value.clone());
            }
            anyhow::ensure!(exists, "No chunks found for {id}");
            if jobs.len() > 1024 {
                jobs.retain(|_, value| value["status"] == "MERGING");
            }
            jobs.insert(id.clone(), json!({"status":"MERGING"}));
        }
        let completed = self.completed.clone();
        let parent = match &kind {
            Kind::File { path, .. } => path.parent().map(Path::to_path_buf),
            Kind::AppFile { .. } => None,
        };
        let runtime = Arc::downgrade(self);
        let task_id = id.clone();
        let (ready, registered) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            if registered.await.is_err() {
                return;
            }
            let outcome = merge(Some(store), &dir, count, size, kind).await;
            if let (Ok((value, _)), Some(parent), Some(completed)) = (&outcome, parent, completed) {
                completed(parent.join(value)).await;
            }
            let (status, event) = match outcome {
                Ok((value, size)) => (
                    json!({"status":"DONE","value":value,"mergedSize":size}),
                    json!({"fileId":id,"ok":true,"value":value,"mergedSize":size}),
                ),
                Err(error) => (
                    json!({"status":"FAILED","error":error.to_string()}),
                    json!({"fileId":id,"ok":false,"error":error.to_string()}),
                ),
            };
            if let Some(runtime) = runtime.upgrade() {
                let _guard = runtime.lifecycle.lock().await;
                runtime.tasks.lock().unwrap().remove(&id);
                runtime.jobs.lock().unwrap().insert(id, status);
                let _ = events.send(WsEvent::broadcast(38, event.to_string()));
            }
        });
        self.tasks.lock().unwrap().insert(task_id, task);
        let _ = ready.send(());
        Ok(json!({"status":"STARTED"}))
    }
}
struct Temp(PathBuf);
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
pub async fn concatenate(dir: &Path, count: i32, output: &Path) -> anyhow::Result<u64> {
    let mut out = tokio::fs::File::create(output).await?;
    let mut written = 0;
    for index in 0..count {
        let mut input = tokio::fs::File::open(dir.join(format!("chunk_{index}")))
            .await
            .map_err(|error| anyhow::anyhow!("Missing chunk {index}: {error}"))?;
        written += tokio::io::copy(&mut input, &mut out).await?;
    }
    out.flush().await?;
    Ok(written)
}
pub async fn merge(
    store: Option<Arc<FileStore>>,
    dir: &Path,
    count: i32,
    size: i64,
    kind: Kind,
) -> anyhow::Result<(String, u64)> {
    anyhow::ensure!(count > 0, "Invalid totalChunks");
    let mut expected = 0u64;
    for index in 0..count {
        let metadata = tokio::fs::metadata(dir.join(format!("chunk_{index}")))
            .await
            .map_err(|error| anyhow::anyhow!("Missing chunk {index}: {error}"))?;
        anyhow::ensure!(metadata.is_file(), "Invalid chunk {index}");
        expected = expected
            .checked_add(metadata.len())
            .ok_or_else(|| anyhow::anyhow!("Chunk size overflow"))?;
    }
    if expected != size as u64 {
        // A stale chunk set must be discarded, matching the mobile upload contract.
        let _ = tokio::fs::remove_dir_all(dir).await;
        anyhow::bail!("Chunk total size {expected} != file size {size}");
    }
    let temp = Temp(dir.join(format!(".merge_{}", uuid::Uuid::new_v4())));
    let written = concatenate(dir, count, &temp.0).await?;
    anyhow::ensure!(
        written == expected,
        "Merge integrity failed: expected {expected}, got {written}"
    );
    let value = match kind {
        Kind::AppFile { name } => {
            let record = store
                .ok_or_else(|| anyhow::anyhow!("App file store is required"))?
                .import(temp.0.clone(), name, String::new(), true)
                .await
                .map_err(anyhow::Error::msg)?;
            Path::new(&record.real_path)
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| anyhow::anyhow!("Invalid imported path"))?
                .to_owned()
        }
        Kind::File { path, replace } => {
            if let Some(parent) = path.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            let path = if replace {
                path
            } else {
                crate::utils::unique_path::unique_sibling(&path)?
            };
            let staging =
                Temp(path.with_file_name(format!(".merge_destination_{}", uuid::Uuid::new_v4())));
            tokio::fs::copy(&temp.0, &staging.0).await?;
            tokio::fs::rename(&staging.0, &path).await?;
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_owned()
        }
    };
    tokio::fs::remove_dir_all(dir).await?;
    Ok((value, written))
}

#[cfg(test)]
#[path = "../tests/unit/uploads.rs"]
mod tests;
