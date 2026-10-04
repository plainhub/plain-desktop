use super::{host::Host, server::ServerState};
use crate::{
    chat::download_queue::{Effect, Queue},
    ws_event::WsEvent,
};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::{broadcast, watch};
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
pub(super) fn start(
    queue: &Arc<Queue>,
    host: Arc<Host>,
    events: broadcast::Sender<WsEvent>,
    stop: watch::Receiver<bool>,
) {
    let weak = Arc::downgrade(queue);
    let mut changed = queue.changed.subscribe();
    let mut stop_worker = stop.clone();
    tokio::spawn(async move {
        loop {
            let effect = match weak.upgrade() {
                Some(queue) => queue.effect(),
                None => break,
            };
            if let Some(effect) = effect {
                let (method, params) = match &effect {
                    Effect::Start { task, ticket } => (
                        "attachmentTransferStart",
                        json!({"task":task,"path":ticket.path}),
                    ),
                    Effect::Cancel { token } => {
                        ("attachmentTransferCancel", json!({"token":token}))
                    }
                };
                let result = tokio::select! {result=host.call(method,params)=>result, _=stop_worker.changed()=>break};
                if let Effect::Start { task, .. } = effect {
                    let error = match result {
                        Ok(value) if value == json!(true) => None,
                        Ok(_) => Some("Attachment driver did not accept transfer".into()),
                        Err(error) => Some(error),
                    };
                    if let Some(error) = error {
                        if let Some(queue) = weak.upgrade() {
                            let _ = queue.finish(&task.id, &task.generation, Some(error));
                        }
                    }
                }
                continue;
            }
            tokio::select! {result=changed.changed()=>if result.is_err(){break},_=stop_worker.changed()=>break};
        }
    });
    let weak = Arc::downgrade(queue);
    let mut changed = queue.changed.subscribe();
    let mut stop = stop;
    tokio::spawn(async move {
        loop {
            tokio::select! {result=changed.changed()=>if result.is_err(){break},_=stop.changed()=>break};
            let Some(queue) = weak.upgrade() else { break };
            let _ = events.send(WsEvent::broadcast(
                crate::chat::events::WS_DOWNLOAD_PROGRESS,
                queue.public_progress(),
            ));
        }
    });
}

#[cfg(test)]
#[path = "../../tests/unit/content_api/download_queue.rs"]
mod tests;
