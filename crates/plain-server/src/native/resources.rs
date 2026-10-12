use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio::sync::watch;

pub const MAX_RESOURCE_READ_BYTES: usize = 64 * 1024;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceInfo {
    pub id: String,
    pub name: String,
    pub mime: String,
    pub size: u64,
}

struct Resource {
    file: tokio::sync::Mutex<File>,
    size: u64,
    closed: watch::Sender<bool>,
}

#[derive(Default)]
struct State {
    closed: bool,
    resources: HashMap<String, Arc<Resource>>,
}

#[derive(Default)]
pub struct NativeFileResources {
    state: Mutex<State>,
}

impl NativeFileResources {
    pub async fn open(&self, path: &Path) -> Result<ResourceInfo, String> {
        if self.state.lock().unwrap().closed {
            return Err("resource owner closed".into());
        }
        let file = File::open(path).await.map_err(|error| error.to_string())?;
        let metadata = file.metadata().await.map_err(|error| error.to_string())?;
        if !metadata.is_file() {
            return Err("resource must be a file".into());
        }
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let info = ResourceInfo {
            id: uuid::Uuid::new_v4().to_string(),
            mime: plain_rs::utils::mime::mime_from_ext(&name).into(),
            name,
            size: metadata.len(),
        };
        let (closed, _) = watch::channel(false);
        let resource = Arc::new(Resource {
            file: tokio::sync::Mutex::new(file),
            size: info.size,
            closed,
        });
        let mut state = self.state.lock().unwrap();
        if state.closed {
            return Err("resource owner closed".into());
        }
        state.resources.insert(info.id.clone(), resource);
        Ok(info)
    }

    pub async fn read(&self, id: &str, offset: u64, length: usize) -> Result<Vec<u8>, String> {
        if length == 0 || length > MAX_RESOURCE_READ_BYTES {
            return Err("invalid resource read length".into());
        }
        let resource = self
            .state
            .lock()
            .unwrap()
            .resources
            .get(id)
            .cloned()
            .ok_or("resource unavailable")?;
        if offset > resource.size {
            return Err("resource offset exceeds size".into());
        }
        let length = (resource.size - offset).min(length as u64) as usize;
        let mut closed = resource.closed.subscribe();
        if *closed.borrow() {
            return Err("resource closed".into());
        }
        let read = async {
            let mut file = resource.file.lock().await;
            file.seek(std::io::SeekFrom::Start(offset))
                .await
                .map_err(|error| error.to_string())?;
            let mut bytes = vec![0; length];
            file.read_exact(&mut bytes)
                .await
                .map_err(|error| error.to_string())?;
            Ok::<Vec<u8>, String>(bytes)
        };
        let bytes = tokio::select! {
            _ = closed.changed() => return Err("resource closed".into()),
            result = read => result?,
        };
        if *closed.borrow() {
            return Err("resource closed".into());
        }
        Ok(bytes)
    }

    pub fn close(&self, id: &str) {
        if let Some(resource) = self.state.lock().unwrap().resources.remove(id) {
            resource.closed.send_replace(true);
        }
    }

    pub fn close_all(&self) {
        let mut state = self.state.lock().unwrap();
        state.closed = true;
        for (_, resource) in state.resources.drain() {
            resource.closed.send_replace(true);
        }
    }
}

impl Drop for NativeFileResources {
    fn drop(&mut self) {
        self.close_all();
    }
}
