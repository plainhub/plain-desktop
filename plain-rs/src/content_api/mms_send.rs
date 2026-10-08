use super::host::Host;
use crate::{db::Db, prefs::Prefs, ws_event::WsEvent};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::{broadcast, watch};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Attachment {
    path: String,
    content_type: String,
    name: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Pending {
    id: String,
    number: String,
    body: String,
    thread_id: String,
    attachments: Vec<Attachment>,
    launch_time_sec: i64,
    minimum_id: i64,
    created_at: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Candidate {
    id: i64,
    address: String,
    body: String,
    thread_id: String,
    attachment_content_types: Vec<String>,
}
#[derive(Default)]
struct PendingState {
    pending: HashMap<String, Pending>,
    cancels: HashMap<String, watch::Sender<bool>>,
    claimed: VecDeque<i64>,
    tasks: HashMap<String, tokio::task::JoinHandle<()>>,
}
pub(super) struct Runtime {
    outbox: Mutex<()>,
    state: Mutex<PendingState>,
    send: tokio::sync::Mutex<()>,
    host: Arc<Host>,
    prefs: Arc<Prefs>,
    db: Arc<Db>,
    directory: std::path::PathBuf,
    events: broadcast::Sender<WsEvent>,
    #[cfg(feature = "http_transport")]
    resources: Mutex<
        Option<(
            Arc<super::native_resources::NativeResources>,
            watch::Receiver<bool>,
        )>,
    >,
}
impl Runtime {
    pub fn new(
        host: Arc<Host>,
        prefs: Arc<Prefs>,
        db: Arc<Db>,
        directory: std::path::PathBuf,
        events: broadcast::Sender<WsEvent>,
    ) -> Arc<Self> {
        Arc::new(Self {
            outbox: Mutex::new(()),
            state: Mutex::new(PendingState::default()),
            send: tokio::sync::Mutex::new(()),
            host,
            prefs,
            db,
            directory,
            events,
            #[cfg(feature = "http_transport")]
            resources: Mutex::new(None),
        })
    }
    #[cfg(feature = "http_transport")]
    pub(super) fn set_resources(
        &self,
        resources: Arc<super::native_resources::NativeResources>,
        stop: watch::Receiver<bool>,
    ) {
        *self.resources.lock().unwrap() = Some((resources, stop));
    }
    async fn copy_attachment(
        &self,
        path: &str,
        destination: &std::path::Path,
    ) -> anyhow::Result<String> {
        if path.starts_with("content://") {
            #[cfg(feature = "http_transport")]
            {
                use futures_util::StreamExt;
                use tokio::io::AsyncWriteExt;
                let (resources, stop) =
                    self.resources.lock().unwrap().clone().ok_or_else(|| {
                        anyhow::anyhow!("Native attachment resources are unavailable")
                    })?;
                let facts = self
                    .host
                    .call("fileMetadataFacts", json!({"path": path}))
                    .await
                    .map_err(anyhow::Error::msg)?;
                let mime = facts["mimeType"]
                    .as_str()
                    .filter(|value| !value.is_empty())
                    .unwrap_or("application/octet-stream")
                    .to_owned();
                let body = resources
                    .resource(path, stop)
                    .await
                    .map_err(|status| anyhow::anyhow!("Attachment resource returned {status}"))?;
                let mut stream = body.into_data_stream();
                let mut file = tokio::fs::File::create(destination).await?;
                while let Some(chunk) = stream.next().await {
                    file.write_all(&chunk?).await?;
                }
                file.flush().await?;
                return Ok(mime);
            }
            #[cfg(not(feature = "http_transport"))]
            anyhow::bail!("Native attachment resources are unavailable");
        }
        anyhow::ensure!(
            tokio::fs::try_exists(path).await?,
            "Attachment file not found: {path}"
        );
        tokio::fs::copy(path, destination).await?;
        Ok(crate::utils::mime::mime_from_ext(path).to_owned())
    }
    pub fn snapshot(&self) -> Value {
        json!(
            self.state
                .lock()
                .unwrap()
                .pending
                .values()
                .cloned()
                .collect::<Vec<_>>()
        )
    }
    pub async fn cancel_all(&self) {
        let _guard = self.send.lock().await;
        for cancel in self.state.lock().unwrap().cancels.values() {
            let _ = cancel.send(true);
        }
    }
    pub async fn shutdown(&self) {
        let _guard = self.send.lock().await;
        let tasks = {
            let mut state = self.state.lock().unwrap();
            for cancel in state.cancels.values() {
                let _ = cancel.send(true);
            }
            std::mem::take(&mut state.tasks)
        };
        for (_, task) in tasks {
            task.abort();
            let _ = task.await;
        }
        let mut state = self.state.lock().unwrap();
        state.pending.clear();
        state.cancels.clear();
    }
    pub fn follow(self: &Arc<Self>, mut stop: watch::Receiver<bool>) {
        let runtime = self.clone();
        tokio::spawn(async move {
            if !*stop.borrow() {
                let _ = stop.changed().await;
            }
            runtime.shutdown().await;
        });
    }
    pub fn replay(&self) -> anyhow::Result<()> {
        let _guard = self.outbox.lock().unwrap();
        let now = chrono::Utc::now().timestamp_millis();
        let mut terminal: Vec<Value> = self.prefs.get_or("mms_terminal_results", Vec::new());
        terminal.retain(|value| {
            now.saturating_sub(value["terminalAtMillis"].as_i64().unwrap_or_default()) < 300_000
        });
        terminal.sort_by_key(|value| value["terminalAtMillis"].as_i64().unwrap_or_default());
        self.prefs.set("mms_terminal_results", &terminal)?;
        for value in terminal {
            let _ = self
                .events
                .send(WsEvent::broadcast(37, value["result"].to_string()));
        }
        Ok(())
    }
    pub async fn send(
        self: &Arc<Self>,
        number: String,
        body: String,
        paths: Vec<String>,
        thread_id: String,
    ) -> anyhow::Result<String> {
        anyhow::ensure!(!number.trim().is_empty(), "MMS recipient is required");
        let _guard = self.send.lock().await;
        let id = format!("pending_mms_{}", uuid::Uuid::new_v4());
        let temporary = OwnedAttachments(self.directory.join("mms_attachments").join(&id));
        tokio::fs::create_dir_all(&temporary.0).await?;
        let mut attachments = vec![];
        for (index, path) in paths.into_iter().enumerate() {
            let path = super::mobile_files::resolve_reference(
                &self.host,
                self.db.clone(),
                self.directory.clone(),
                &path,
            )
            .await?;
            let name = std::path::Path::new(&path)
                .file_name()
                .and_then(|v| v.to_str())
                .unwrap_or("attachment");
            let copied = temporary.0.join(format!("{index}_{name}"));
            let content_type = self.copy_attachment(&path, &copied).await?;
            attachments.push(Attachment {
                name: std::path::Path::new(&path)
                    .file_name()
                    .and_then(|v| v.to_str())
                    .unwrap_or_default()
                    .to_owned(),
                content_type,
                path: copied.to_string_lossy().into_owned(),
            });
        }
        {
            let state = self.state.lock().unwrap();
            let types = normalized_types(attachments.iter().map(|a| a.content_type.as_str()));
            anyhow::ensure!(
                !state
                    .pending
                    .values()
                    .any(|pending| super::sms_query::addresses_match(
                        &pending.number,
                        &number,
                        &[pending.number.clone()]
                    ) && (body.is_empty()
                        || pending.body.is_empty()
                        || body.trim() == pending.body.trim())
                        && (thread_id.is_empty()
                            || pending.thread_id.is_empty()
                            || thread_id == pending.thread_id)
                        && types
                            == normalized_types(
                                pending.attachments.iter().map(|a| a.content_type.as_str())
                            )),
                "An indistinguishable MMS send is already pending"
            );
        }
        let latest = self
            .host
            .call("systemMmsLatest", json!({}))
            .await
            .map_err(anyhow::Error::msg)?
            .as_i64()
            .unwrap_or_default();
        let launched = self
            .host
            .call(
                "systemMmsLaunch",
                json!({"number":number,"body":body,"attachments":attachments}),
            )
            .await
            .map_err(anyhow::Error::msg)?
            .as_i64()
            .ok_or_else(|| anyhow::anyhow!("Invalid MMS launch receipt"))?;
        anyhow::ensure!(launched > 0, "MMS sending is unavailable on this platform");
        let pending = Pending {
            id: id.clone(),
            number,
            body,
            thread_id,
            attachments,
            launch_time_sec: launched,
            minimum_id: latest,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        let (cancel, receiver) = watch::channel(false);
        {
            let mut state = self.state.lock().unwrap();
            state.pending.insert(id.clone(), pending.clone());
            state.cancels.insert(id.clone(), cancel);
        }
        let runtime = self.clone();
        let task_id = id.clone();
        let (ready, registered) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            if registered.await.is_err() {
                return;
            }
            runtime.clone().poll(pending, receiver, temporary).await;
            runtime.state.lock().unwrap().tasks.remove(&task_id);
        });
        self.state.lock().unwrap().tasks.insert(id.clone(), task);
        let _ = ready.send(());
        Ok(id)
    }
    async fn poll(
        self: Arc<Self>,
        pending: Pending,
        mut cancel: watch::Receiver<bool>,
        _temporary: OwnedAttachments,
    ) {
        let mut result_code = -1000;
        for _ in 0..150 {
            if *cancel.borrow() {
                result_code = -1001;
                break;
            }
            tokio::select! {_=tokio::time::sleep(Duration::from_secs(2))=>{},_=cancel.changed()=>{result_code=-1001;break;}}
            let facts = tokio::select! {result=self.host.call("systemMmsCandidates",json!({"minimumId":pending.minimum_id,"launchTimeSec":pending.launch_time_sec}))=>result,_=cancel.changed()=>{result_code=-1001;break;}};
            if let Ok(facts) = facts {
                if let Ok(candidates) = serde_json::from_value::<Vec<Candidate>>(facts) {
                    let matches = matching(&pending, candidates);
                    let mut state = self.state.lock().unwrap();
                    if let Some(id) = matches.into_iter().find(|id| !state.claimed.contains(id)) {
                        state.claimed.push_back(id);
                        if state.claimed.len() > 500 {
                            state.claimed.pop_front();
                        }
                        result_code = 0;
                        break;
                    }
                }
            }
        }
        {
            let mut state = self.state.lock().unwrap();
            state.pending.remove(&pending.id);
            state.cancels.remove(&pending.id);
        }
        let result =
            json!({"pendingId":pending.id,"success":result_code==0,"resultCode":result_code});
        {
            let _guard = self.outbox.lock().unwrap();
            let mut terminal: Vec<Value> = self.prefs.get_or("mms_terminal_results", Vec::new());
            terminal.push(
                json!({"terminalAtMillis":chrono::Utc::now().timestamp_millis(),"result":result}),
            );
            if terminal.len() > 500 {
                terminal.drain(..terminal.len() - 500);
            }
            if let Err(error) = self.prefs.set("mms_terminal_results", &terminal) {
                log::warn!("MMS result persistence failed: {error}");
            }
        }
        let event = if result_code == 0 {
            WsEvent::broadcast(17, json!(pending.id).to_string())
        } else {
            WsEvent::broadcast(37, result.to_string())
        };
        let _ = self.events.send(event);
        if result_code == -1001 {
            tokio::time::sleep(Duration::from_secs(300)).await;
        }
    }
}
struct OwnedAttachments(std::path::PathBuf);
impl Drop for OwnedAttachments {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn normalized_types<'a>(types: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut types: Vec<_> = types
        .filter(|value| {
            !value
                .split(';')
                .next()
                .unwrap_or_default()
                .trim()
                .eq_ignore_ascii_case("application/smil")
        })
        .map(|value| {
            let value = value
                .split(';')
                .next()
                .unwrap_or_default()
                .trim()
                .to_lowercase();
            if let Some((family, _)) = value.split_once('/') {
                if matches!(family, "image" | "audio" | "video") {
                    return format!("{family}/*");
                }
            }
            value
        })
        .collect();
    types.sort();
    types
}
fn matching(pending: &Pending, candidates: Vec<Candidate>) -> Vec<i64> {
    let types = normalized_types(pending.attachments.iter().map(|a| a.content_type.as_str()));
    let candidates: Vec<_> = candidates
        .into_iter()
        .filter(|candidate| {
            (pending.thread_id.is_empty() || candidate.thread_id == pending.thread_id)
                && (pending.body.is_empty() || candidate.body.trim() == pending.body.trim())
                && normalized_types(
                    candidate
                        .attachment_content_types
                        .iter()
                        .map(String::as_str),
                ) == types
        })
        .collect();
    let context: Vec<_> = candidates
        .iter()
        .map(|c| c.address.clone())
        .filter(|address| !address.trim().is_empty())
        .collect();
    candidates
        .into_iter()
        .filter(|candidate| {
            pending.number.is_empty()
                || super::sms_query::addresses_match(&candidate.address, &pending.number, &context)
        })
        .map(|c| c.id)
        .collect()
}
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
pub(super) enum Command {
    Snapshot,
    CancelAll,
    Replay,
}
pub(super) async fn call(
    State(state): State<super::server::ServerState>,
    headers: HeaderMap,
    Json(command): Json<Command>,
) -> Response {
    if !state.authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let result = match command {
        Command::Snapshot => state.mms.snapshot(),
        Command::CancelAll => {
            state.mms.cancel_all().await;
            json!(true)
        }
        Command::Replay => {
            if let Err(error) = state.mms.replay() {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error":error.to_string()})),
                )
                    .into_response();
            }
            json!(true)
        }
    };
    Json(json!({"result":result})).into_response()
}

#[cfg(test)]
#[path = "../../tests/unit/content_api/mms_send.rs"]
mod tests;
