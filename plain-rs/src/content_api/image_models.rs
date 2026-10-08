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
    sync::{Arc, Mutex},
    time::Duration,
};

const FILES: [(&str, u64); 3] = [
    ("mobileclip_s2_image.tflite", 144120668),
    ("mobileclip_s2_text.tflite", 253874828),
    ("tokenizer.json", 1708304),
];
const BASE: &str = "https://huggingface.co/plainhub/mobileclip-s2-tflite/resolve/main";
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Snapshot {
    pub version: u64,
    #[serde(flatten)]
    pub status: ImageSearchStatus,
    pub image_model: String,
    pub text_model: String,
    pub tokenizer: String,
}
struct ModelState {
    version: u64,
    fingerprint: String,
    status: ImageSearchStatusType,
    progress: i32,
    error: String,
}
pub(super) struct Runtime {
    directory: PathBuf,
    host: Arc<Host>,
    prefs: Arc<Prefs>,
    index: Arc<ImageIndex>,
    state: Mutex<ModelState>,
    tokenizer: Mutex<Option<Arc<crate::clip_tokenizer::ClipTokenizer>>>,
    task: Mutex<Option<tokio::task::JoinHandle<()>>>,
    operations: tokio::sync::Mutex<()>,
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
            tokenizer: Mutex::new(None),
            task: Mutex::new(None),
            operations: tokio::sync::Mutex::new(()),
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
                    FILES.iter().map(|(_, size)| *size).sum::<u64>() as i64,
                ),
                model_dir: self.directory.to_string_lossy().into_owned(),
                is_indexing: index.is_running,
                total_images: index.total_images.min(i32::MAX as usize) as i32,
                indexed_images: index.indexed_images.min(i32::MAX as usize) as i32,
            },
            image_model: self
                .directory
                .join(FILES[0].0)
                .to_string_lossy()
                .into_owned(),
            text_model: self
                .directory
                .join(FILES[1].0)
                .to_string_lossy()
                .into_owned(),
            tokenizer: self
                .directory
                .join(FILES[2].0)
                .to_string_lossy()
                .into_owned(),
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
            10009,
            serde_json::to_string(&snapshot).unwrap(),
        ));
        let _ = self.events.send(WsEvent::broadcast(
            19,
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
        FILES.iter().all(|(name, size)| {
            std::fs::metadata(self.directory.join(name))
                .is_ok_and(|meta| meta.is_file() && meta.len() == *size)
        })
    }
    pub async fn enable(self: &Arc<Self>, restore: bool) -> Result<(), String> {
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
        if self
            .host
            .call("systemImageModelAvailability", json!({}))
            .await?
            .as_bool()
            != Some(true)
        {
            return Ok(());
        }
        if restore
            && (!self.prefs.get_user_or("ai_image_search_enabled", false) || !self.available())
        {
            return Ok(());
        }
        let runtime = self.clone();
        *self.task.lock().unwrap() = Some(tokio::spawn(async move {
            if let Err(error) = runtime.load().await {
                let _ = runtime.host.call("systemImageModelsClose", json!({})).await;
                runtime.set_status(ImageSearchStatusType::Error, error);
            }
        }));
        Ok(())
    }
    async fn load(&self) -> Result<(), String> {
        if !self.available() {
            self.state.lock().unwrap().progress = 0;
            self.set_status(ImageSearchStatusType::Downloading, String::new());
            if let Err(error) = self.download().await {
                let _ = tokio::fs::remove_dir_all(&self.directory).await;
                return Err(error);
            }
        }
        self.set_status(ImageSearchStatusType::Loading, String::new());
        let snapshot = self.snapshot();
        let tokenizer = Arc::new(crate::clip_tokenizer::ClipTokenizer::parse(
            &tokio::fs::read(&snapshot.tokenizer)
                .await
                .map_err(|e| e.to_string())?,
        )?);
        self.host.call("systemImageModelsLoad",json!({"imageModel":snapshot.image_model,"textModel":snapshot.text_model,"tokenizer":snapshot.tokenizer})).await?;
        *self.tokenizer.lock().unwrap() = Some(tokenizer);
        self.prefs
            .set_user("ai_image_search_enabled", true)
            .map_err(|e| e.to_string())?;
        self.index.start(false).map_err(|e| e.to_string())?;
        self.set_status(ImageSearchStatusType::Ready, String::new());
        Ok(())
    }
    async fn download(&self) -> Result<(), String> {
        use tokio::io::AsyncWriteExt;
        tokio::fs::create_dir_all(&self.directory)
            .await
            .map_err(|e| e.to_string())?;
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(30))
            .read_timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| e.to_string())?;
        let total = FILES.iter().map(|(_, size)| *size).sum::<u64>();
        let mut downloaded = 0u64;
        for (name, expected) in FILES {
            let mut response = client
                .get(format!("{BASE}/{name}"))
                .send()
                .await
                .map_err(|e| e.to_string())?
                .error_for_status()
                .map_err(|e| e.to_string())?;
            let partial = self.directory.join(format!("{name}.part"));
            let mut file = tokio::fs::File::create(&partial)
                .await
                .map_err(|e| e.to_string())?;
            let mut size = 0u64;
            while let Some(bytes) = response.chunk().await.map_err(|e| e.to_string())? {
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
            tokio::fs::rename(partial, self.directory.join(name))
                .await
                .map_err(|e| e.to_string())?;
        }
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
        let tokenizer = self
            .tokenizer
            .lock()
            .unwrap()
            .clone()
            .ok_or("Image tokenizer is unavailable")?;
        let value = self
            .host
            .call(
                "systemImageTextEmbed",
                json!({"tokenIds": tokenizer.encode(text)}),
            )
            .await?;
        let mut vector: Vec<f32> = serde_json::from_value(value).map_err(|e| e.to_string())?;
        if vector.is_empty() || vector.len() > 4096 || vector.iter().any(|v| !v.is_finite()) {
            return Err("Invalid image query embedding".into());
        }
        let norm = vector
            .iter()
            .fold(0.0f32, |sum, value| sum + value * value)
            .sqrt();
        if !norm.is_finite() {
            return Err("Invalid image query embedding norm".into());
        }
        if norm > 0.0 {
            for value in &mut vector {
                *value /= norm;
            }
        }
        let bytes = vector
            .iter()
            .flat_map(|value| value.to_be_bytes())
            .collect::<Vec<_>>();
        let encoded = crate::base64_encode(&bytes);
        let index = self.index.clone();
        tokio::task::spawn_blocking(move || index.search(&encoded, limit))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())
    }
    async fn stop_task(&self) {
        let task = self.task.lock().unwrap().take();
        if let Some(task) = task {
            task.abort();
            let _ = task.await;
        }
    }
    async fn close_models(&self) -> Result<(), String> {
        self.index.cancel().map_err(|e| e.to_string())?;
        tokio::time::timeout(Duration::from_secs(45), async {
            while self.index.status().is_running {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .map_err(|_| "Image index cancellation timed out".to_owned())?;
        self.host.call("systemImageModelsClose", json!({})).await?;
        *self.tokenizer.lock().unwrap() = None;
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
        self.close_models().await?;
        if disable {
            self.index.clear().map_err(|e| e.to_string())?;
        }
        self.prefs
            .set_user("ai_image_search_enabled", false)
            .map_err(|e| e.to_string())?;
        if self.directory.exists() {
            tokio::fs::remove_dir_all(&self.directory)
                .await
                .map_err(|e| e.to_string())?;
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
    Disable,
    Cancel,
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
        Command::Disable => runtime.cancel(true).await,
        Command::Cancel => runtime.cancel(false).await,
    };
    match result {
        Ok(()) => Json(json!({"result":runtime.snapshot()})).into_response(),
        Err(error) => (StatusCode::BAD_REQUEST, Json(json!({"error":error}))).into_response(),
    }
}
