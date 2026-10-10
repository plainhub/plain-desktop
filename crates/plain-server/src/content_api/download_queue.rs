use super::server::ServerState;
use crate::{chat::download_queue::Effect, ws_event::WsEvent};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::json;
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
pub(super) enum Request {
    Enqueue {
        message_id: String,
        id: String,
        peer_id: String,
    },
    Control {
        id: String,
        command: String,
    },
    Progress {
        id: String,
        generation: String,
        downloaded: u64,
    },
    Finish {
        id: String,
        generation: String,
        error: Option<String>,
    },
    Snapshot,
}
pub(super) async fn call(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(request): Json<Request>,
) -> Response {
    if !state.authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let result = tokio::task::spawn_blocking(move || -> anyhow::Result<serde_json::Value> {
        Ok(match request {
            Request::Enqueue {
                message_id,
                id,
                peer_id,
            } => json!(state.downloads.enqueue(&message_id, &id, &peer_id)?),
            Request::Control { id, command } => json!(state.downloads.control(&id, &command)?),
            Request::Progress {
                id,
                generation,
                downloaded,
            } => json!(state.downloads.progress(&id, &generation, downloaded)?),
            Request::Finish {
                id,
                generation,
                error,
            } => json!(state.downloads.finish(&id, &generation, error)?),
            Request::Snapshot => state.downloads.snapshot(),
        })
    })
    .await;
    match result {
        Ok(Ok(value))=>Json(json!({"result":value})).into_response(),
        error=>(StatusCode::BAD_REQUEST,Json(json!({"error":match error {Ok(Err(e))=>e.to_string(),Err(e)=>e.to_string(),_=>unreachable!()}}))).into_response(),
    }
}
pub(super) struct Runtime {
    job: tokio::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
    stop: tokio::sync::watch::Sender<bool>,
}
impl Default for Runtime {
    fn default() -> Self {
        Self {
            job: tokio::sync::Mutex::new(None),
            stop: tokio::sync::watch::channel(false).0,
        }
    }
}
impl Runtime {
    pub(super) async fn shutdown(&self) {
        self.stop.send_replace(true);
        if let Some(job) = self.job.lock().await.take() {
            let _ = job.await;
        }
    }
}
pub(super) fn start(state: ServerState) {
    let runtime = state.download_runtime.clone();
    let mut changed = state.downloads.changed.subscribe();
    let mut stop = state.stop.clone();
    let mut runtime_stop = runtime.stop.subscribe();
    let job = tokio::spawn(async move {
        let mut workers: std::collections::HashMap<String, tokio::task::JoinHandle<()>> =
            std::collections::HashMap::new();
        loop {
            workers.retain(|_, job| !job.is_finished());
            if let Some(effect) = state.downloads.effect() {
                match effect {
                    Effect::Cancel { token } => {
                        if let Some(job) = workers.remove(&token) {
                            job.abort();
                            let _ = job.await;
                        }
                    }
                    Effect::Start { task, ticket } => {
                        let state = state.clone();
                        let token = task.generation.clone();
                        let job = tokio::spawn(async move {
                            let result =
                                super::peer_download::run(&state, &task, &ticket.path).await;
                            let error = result.err().map(|e| e.to_string());
                            let db = state.db.clone();
                            let id = task.message_id.clone();
                            let queue = state.downloads.clone();
                            let generation = task.generation;
                            let file_id = task.id;
                            let committed = tokio::task::spawn_blocking(move || {
                                queue.finish(&file_id, &generation, error)
                            })
                            .await;
                            if matches!(committed, Ok(Ok(true))) {
                                if let Ok(Some(chat)) =
                                    crate::db::chat_store::messages::get(&db, &id)
                                {
                                    let _ = state.events.send(WsEvent::broadcast(
                                        crate::chat::events::WS_MESSAGE_UPDATED,
                                        serde_json::json!([crate::chat::service::chat_to_json(
                                            &chat
                                        )])
                                        .to_string(),
                                    ));
                                }
                            }
                        });
                        workers.insert(token, job);
                    }
                }
                continue;
            }
            tokio::select! {_=runtime_stop.changed()=>break,_=stop.changed()=>break,result=changed.changed()=>if result.is_err(){break}};
            let _ = state.events.send(WsEvent::broadcast(
                crate::chat::events::WS_DOWNLOAD_PROGRESS,
                state.downloads.public_progress(),
            ));
        }
        for task in state.downloads.snapshot()["tasks"]
            .as_array()
            .into_iter()
            .flatten()
        {
            if let Some(id) = task["id"].as_str() {
                let _ = state.downloads.control(id, "cancel");
            }
        }
        for (_, job) in workers {
            job.abort();
            let _ = job.await;
        }
    });
    *runtime.job.try_lock().expect("New download runtime") = Some(job);
}

#[cfg(test)]
#[path = "../../tests/unit/content_api/download_queue.rs"]
mod tests;
