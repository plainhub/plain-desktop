use super::{
    host::Host,
    image_index::ImageIndex,
    public_image_index::{ImageSearchStatus, ImageSearchStatusType},
    server::ServerState,
};
use crate::{prefs::Prefs, ws_event::WsEvent};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Snapshot {
    pub version: u64,
    #[serde(flatten)]
    pub status: ImageSearchStatus,
}
struct ModelState {
    version: u64,
    fingerprint: String,
    status: ImageSearchStatusType,
    progress: i32,
    error: String,
}
#[derive(Clone, Copy)]
enum Selection {
    Default,
    Active,
    Incoming,
}

pub(super) struct Runtime {
    directory: PathBuf,
    host: Arc<Host>,
    prefs: Arc<Prefs>,
    index: Arc<ImageIndex>,
    state: Mutex<ModelState>,
    engine: Mutex<Option<Arc<crate::image_inference::Engine>>>,
    task: Mutex<Option<tokio::task::JoinHandle<()>>>,
    operations: tokio::sync::Mutex<()>,
    cancelled: AtomicBool,
    observing: AtomicBool,
    events: tokio::sync::broadcast::Sender<WsEvent>,
}
impl Runtime {
    pub fn new(
        directory: PathBuf,
        host: Arc<Host>,
        prefs: Arc<Prefs>,
        index: Arc<ImageIndex>,
        events: tokio::sync::broadcast::Sender<WsEvent>,
    ) -> Arc<Self> {
        Arc::new(Self {
            directory: directory.join("ai_models"),
            host,
            prefs,
            index,
            events,
            state: Mutex::new(ModelState {
                version: 0,
                fingerprint: String::new(),
                status: ImageSearchStatusType::Unavailable,
                progress: 0,
                error: String::new(),
            }),
            engine: Mutex::new(None),
            task: Mutex::new(None),
            operations: tokio::sync::Mutex::new(()),
            cancelled: AtomicBool::new(false),
            observing: AtomicBool::new(false),
        })
    }
    pub fn snapshot(&self) -> Snapshot {
        let mut state = self.state.lock().unwrap();
        let index = self.index.status();
        let mut snapshot = Snapshot {
            version: 0,
            status: ImageSearchStatus {
                status: state.status,
                download_progress: state.progress,
                error_message: if index.error_message.is_empty() {
                    state.error.clone()
                } else {
                    index.error_message
                },
                model_size: crate::content_types::Long(
                    self.engine.lock().unwrap().as_ref().map_or_else(
                        || crate::image_inference::default_model::manifest().size(),
                        |e| e.manifest.size(),
                    ) as i64,
                ),
                model_dir: self
                    .directory
                    .join("incoming")
                    .to_string_lossy()
                    .into_owned(),
                is_indexing: index.is_running,
                total_images: index.total_images.min(i32::MAX as usize) as i32,
                indexed_images: index.indexed_images.min(i32::MAX as usize) as i32,
            },
        };
        let fingerprint = serde_json::to_string(&snapshot).unwrap();
        if state.fingerprint != fingerprint {
            state.version += 1;
            state.fingerprint = fingerprint;
        }
        snapshot.version = state.version;
        snapshot
    }
    fn publish(&self) {
        let snapshot = self.snapshot();
        let _ = self.events.send(WsEvent::broadcast(
            "IMAGE_MODELS_UPDATED",
            serde_json::to_string(&snapshot).unwrap(),
        ));
        let _ = self.events.send(WsEvent::broadcast(
            "IMAGE_SEARCH_UPDATED",
            serde_json::to_string(&snapshot.status).unwrap(),
        ));
    }
    fn set_status(&self, status: ImageSearchStatusType, error: String) {
        {
            let mut state = self.state.lock().unwrap();
            state.status = status;
            state.error = error;
        }
        self.publish();
    }
    fn available(&self) -> bool {
        self.directory.join("active/manifest.json").is_file()
    }
    pub async fn enable(self: &Arc<Self>, restore: bool) -> Result<(), String> {
        self.activate(if restore {
            Selection::Active
        } else {
            Selection::Default
        })
        .await
    }
    pub async fn import(self: &Arc<Self>) -> Result<(), String> {
        if !self.directory.join("incoming/manifest.json").is_file() {
            return Err("Upload a complete model package first".into());
        }
        self.activate(Selection::Incoming).await
    }
    async fn activate(self: &Arc<Self>, selection: Selection) -> Result<(), String> {
        let _guard = self.operations.lock().await;
        if self
            .task
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|task| !task.is_finished())
        {
            return Ok(());
        }
        if matches!(selection, Selection::Active)
            && (!self.prefs.get_user_or("ai_image_search_enabled", false) || !self.available())
        {
            return Ok(());
        }
        self.cancelled.store(false, Ordering::Release);
        let runtime = self.clone();
        *self.task.lock().unwrap() = Some(tokio::spawn(async move {
            if let Err(error) = runtime.load(selection).await {
                let status = if runtime.engine.lock().unwrap().is_some() {
                    ImageSearchStatusType::Ready
                } else {
                    ImageSearchStatusType::Error
                };
                runtime.set_status(status, error);
            }
        }));
        Ok(())
    }
    async fn load(&self, selection: Selection) -> Result<(), String> {
        let active = self.directory.join("active");
        let default = crate::image_inference::default_model::manifest();
        let active_is_default =
            crate::image_inference::manifest::Manifest::read(&active.join("manifest.json"))
                .ok()
                .is_some_and(|manifest| manifest.fingerprint().ok() == default.fingerprint().ok());
        let source = match selection {
            Selection::Incoming => self.directory.join("incoming"),
            Selection::Active => active.clone(),
            Selection::Default if active_is_default => active.clone(),
            Selection::Default => {
                self.state.lock().unwrap().progress = 0;
                self.set_status(ImageSearchStatusType::Downloading, String::new());
                self.download().await?;
                self.directory.join("download")
            }
        };
        self.set_status(ImageSearchStatusType::Loading, String::new());
        let manifest =
            crate::image_inference::manifest::Manifest::read(&source.join("manifest.json"))?;
        let fingerprint = manifest.fingerprint()?;
        self.pause_index().await?;
        if let Some(engine) = self.engine.lock().unwrap().as_ref() {
            engine.release_all();
        }
        let directory = source.clone();
        let engine = tokio::task::spawn_blocking(move || {
            crate::image_inference::Engine::open(directory, manifest)
        })
        .await
        .map_err(|e| e.to_string())??;
        if self.cancelled.load(Ordering::Acquire) {
            return Err("Model activation cancelled".into());
        }
        let previous = self.directory.join("previous");
        if source != active {
            if previous.exists() {
                std::fs::remove_dir_all(&previous).map_err(|e| e.to_string())?;
            }
            if active.exists() {
                std::fs::rename(&active, &previous).map_err(|e| e.to_string())?;
            }
            if let Err(error) = std::fs::rename(&source, &active) {
                if previous.exists() {
                    let _ = std::fs::rename(&previous, &active);
                }
                return Err(error.to_string());
            }
        }
        let engine = Arc::new(engine.relocate(active));
        if self
            .prefs
            .get_user_or("ai_image_search_model", String::new())
            != fingerprint
        {
            self.index.clear().map_err(|e| e.to_string())?;
            self.prefs
                .set_user("ai_image_search_model", fingerprint)
                .map_err(|e| e.to_string())?;
        }
        self.index.set_encoder(Some(engine.clone()));
        *self.engine.lock().unwrap() = Some(engine);
        self.prefs
            .set_user("ai_image_search_enabled", true)
            .map_err(|e| e.to_string())?;
        if !self.observing.load(Ordering::Acquire) {
            self.host
                .call("systemImageModelsObserve", json!({"enabled":true}))
                .await?;
            self.observing.store(true, Ordering::Release);
        }
        self.index.start(false).map_err(|e| e.to_string())?;
        self.set_status(ImageSearchStatusType::Ready, String::new());
        if previous.exists() {
            let _ = tokio::fs::remove_dir_all(previous).await;
        }
        Ok(())
    }
    async fn download(&self) -> Result<(), String> {
        use tokio::io::AsyncWriteExt;
        let directory = self.directory.join("download");
        tokio::fs::create_dir_all(&directory)
            .await
            .map_err(|e| e.to_string())?;
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(30))
            .read_timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| e.to_string())?;
        let manifest = crate::image_inference::default_model::manifest();
        let total = manifest.size();
        let mut downloaded = 0u64;
        for (asset, url) in crate::image_inference::default_model::downloads(&manifest) {
            if self.cancelled.load(Ordering::Acquire) {
                return Err("Model download cancelled".into());
            }
            let name = asset.name.as_str();
            let expected = asset.size;
            let mut response = client
                .get(url)
                .send()
                .await
                .map_err(|e| e.to_string())?
                .error_for_status()
                .map_err(|e| e.to_string())?;
            let partial = directory.join(format!("{name}.part"));
            let mut file = tokio::fs::File::create(&partial)
                .await
                .map_err(|e| e.to_string())?;
            let mut size = 0u64;
            while let Some(bytes) = response.chunk().await.map_err(|e| e.to_string())? {
                if self.cancelled.load(Ordering::Acquire) {
                    return Err("Model download cancelled".into());
                }
                file.write_all(&bytes).await.map_err(|e| e.to_string())?;
                size += bytes.len() as u64;
                downloaded += bytes.len() as u64;
                if size > expected {
                    return Err(format!("Unexpected model size: {name}"));
                }
                let progress = (downloaded.saturating_mul(100) / total).min(99) as i32;
                let changed = {
                    let mut state = self.state.lock().unwrap();
                    let changed = state.progress != progress;
                    state.progress = progress;
                    changed
                };
                if changed {
                    self.publish();
                }
            }
            file.flush().await.map_err(|e| e.to_string())?;
            drop(file);
            if size != expected {
                return Err(format!("Incomplete model: {name}"));
            }
            tokio::fs::rename(partial, directory.join(name))
                .await
                .map_err(|e| e.to_string())?;
        }
        tokio::fs::write(
            directory.join("LICENSE"),
            include_str!("../../../plain-inference/src/APACHE-2.0.txt"),
        )
        .await
        .map_err(|e| e.to_string())?;
        tokio::fs::write(
            directory.join("manifest.json"),
            serde_json::to_vec(&manifest).map_err(|e| e.to_string())?,
        )
        .await
        .map_err(|e| e.to_string())?;
        self.state.lock().unwrap().progress = 100;
        self.publish();
        Ok(())
    }
    pub async fn search(
        &self,
        text: &str,
        limit: usize,
    ) -> Result<Vec<crate::library::image_embeddings::SearchResult>, String> {
        let _guard = self.operations.lock().await;
        if self.snapshot().status.status != ImageSearchStatusType::Ready {
            return Err("Image search models are unavailable".into());
        }
        if limit > 500 {
            return Err("Image search limit too large".into());
        }
        let engine = self
            .engine
            .lock()
            .unwrap()
            .clone()
            .ok_or("Image search engine unavailable")?;
        let minimum_score = engine.manifest.minimum_score;
        let text = text.to_owned();
        let vector = tokio::task::spawn_blocking(move || engine.text(&text))
            .await
            .map_err(|e| e.to_string())??;
        let bytes = vector
            .iter()
            .flat_map(|value| value.to_be_bytes())
            .collect::<Vec<_>>();
        let encoded = crate::base64_encode(&bytes);
        let index = self.index.clone();
        tokio::task::spawn_blocking(move || index.search_with_score(&encoded, limit, minimum_score))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())
    }
    pub async fn release(&self) -> Result<(), String> {
        let _guard = self.operations.lock().await;
        self.pause_index().await?;
        if let Some(engine) = self.engine.lock().unwrap().as_ref() {
            engine.release_all();
        }
        self.publish();
        Ok(())
    }
    async fn stop_task(&self) {
        self.cancelled.store(true, Ordering::Release);
        let task = self.task.lock().unwrap().take();
        if let Some(task) = task {
            let _ = task.await;
        }
    }
    async fn pause_index(&self) -> Result<(), String> {
        self.index.cancel().map_err(|e| e.to_string())?;
        tokio::time::timeout(Duration::from_secs(45), async {
            while self.index.status().is_running {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .map_err(|_| "Image index cancellation timed out".to_owned())?;
        Ok(())
    }
    async fn close_models(&self) -> Result<(), String> {
        self.pause_index().await?;
        self.index.set_encoder(None);
        *self.engine.lock().unwrap() = None;
        if self.observing.swap(false, Ordering::AcqRel) {
            self.host
                .call("systemImageModelsObserve", json!({"enabled":false}))
                .await?;
        }
        Ok(())
    }
    pub async fn cancel(&self, disable: bool) -> Result<(), String> {
        let _guard = self.operations.lock().await;
        let active = self
            .task
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|task| !task.is_finished());
        let status = self.snapshot().status.status;
        if !disable
            && (matches!(
                status,
                ImageSearchStatusType::Ready | ImageSearchStatusType::Error
            ) || (!active
                && !matches!(
                    status,
                    ImageSearchStatusType::Downloading | ImageSearchStatusType::Loading
                )))
        {
            return Ok(());
        }
        self.stop_task().await;
        if !disable && self.engine.lock().unwrap().is_some() {
            let engine = self.engine.lock().unwrap().clone().unwrap();
            self.index.set_encoder(Some(engine));
            self.set_status(ImageSearchStatusType::Ready, String::new());
            return Ok(());
        }
        self.close_models().await?;
        if disable {
            self.index.clear().map_err(|e| e.to_string())?;
        }
        self.prefs
            .set_user("ai_image_search_enabled", false)
            .map_err(|e| e.to_string())?;
        if disable && self.directory.exists() {
            tokio::fs::remove_dir_all(&self.directory)
                .await
                .map_err(|e| e.to_string())?;
        } else {
            for name in ["download", "incoming"] {
                let path = self.directory.join(name);
                if path.exists() {
                    tokio::fs::remove_dir_all(path)
                        .await
                        .map_err(|e| e.to_string())?;
                }
            }
        }
        self.state.lock().unwrap().progress = 0;
        self.set_status(ImageSearchStatusType::Unavailable, String::new());
        Ok(())
    }
    pub async fn shutdown(&self) {
        let _guard = self.operations.lock().await;
        self.stop_task().await;
        if let Err(error) = self.close_models().await {
            log::warn!("Image model shutdown failed: {error}");
        }
        self.set_status(ImageSearchStatusType::Unavailable, String::new());
    }
    pub fn follow(self: &Arc<Self>, mut stop: tokio::sync::watch::Receiver<bool>) {
        let runtime = self.clone();
        tokio::spawn(async move {
            let mut previous = String::new();
            while !*stop.borrow() {
                tokio::select! {_=stop.changed()=>break,_=tokio::time::sleep(Duration::from_millis(500))=>{}}
                let current = serde_json::to_string(&runtime.snapshot()).unwrap();
                if previous != current {
                    runtime.publish();
                    previous = current;
                }
            }
            runtime.shutdown().await;
        });
    }
}
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
pub(super) enum Command {
    Snapshot,
    Search { text: String, limit: usize },
    Restore,
    Enable,
    Import,
    Disable,
    Cancel,
    Release,
    Error { message: String },
}
pub(super) async fn call(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(command): Json<Command>,
) -> Response {
    if !state.authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let runtime = &state.image_models;
    let result = match command {
        Command::Search { text, limit } => {
            return match runtime.search(&text, limit).await {
                Ok(items) => Json(json!({"items": items})).into_response(),
                Err(error) => {
                    (StatusCode::BAD_REQUEST, Json(json!({"error": error}))).into_response()
                }
            };
        }
        Command::Error { message } => {
            runtime.state.lock().unwrap().error = message;
            runtime.publish();
            Ok(())
        }
        Command::Snapshot => Ok(()),
        Command::Restore => runtime.enable(true).await,
        Command::Enable => runtime.enable(false).await,
        Command::Import => runtime.import().await,
        Command::Disable => runtime.cancel(true).await,
        Command::Cancel => runtime.cancel(false).await,
        Command::Release => runtime.release().await,
    };
    match result {
        Ok(()) => Json(json!({"result":runtime.snapshot()})).into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, Json(json!({"error":error}))).into_response(),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/content_api/image_models.rs"]
mod tests;
