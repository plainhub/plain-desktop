use super::server::ServerState;
use axum::{
    Json,
    extract::{
        Query, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::json;
use std::{sync::Arc, time::Duration};

pub(super) struct Runtime {
    pub(super) connections: crate::chat::peer_status::Connections,
    capacity: Arc<tokio::sync::Semaphore>,
    pub(super) outgoing: Arc<super::status_outgoing::Outgoing>,
    pub(super) control: tokio::sync::Mutex<()>,
    pub(super) public_active: std::sync::atomic::AtomicBool,
}
impl Runtime {
    fn snapshot(&self, db: &crate::db::Db) -> serde_json::Value {
        let mut row = self.connections.snapshot(db);
        row["outgoing"] = self.outgoing.snapshot();
        row
    }
}
impl Default for Runtime {
    fn default() -> Self {
        Self {
            connections: Default::default(),
            control: tokio::sync::Mutex::new(()),
            public_active: std::sync::atomic::AtomicBool::new(false),
            outgoing: Arc::new(super::status_outgoing::Outgoing::new()),
            capacity: Arc::new(tokio::sync::Semaphore::new(128)),
        }
    }
}
#[derive(Clone)]
pub(super) struct PublicState {
    pub(super) state: ServerState,
    pub(super) stop: tokio::sync::watch::Receiver<bool>,
}
#[derive(Deserialize)]
pub(super) struct Parameters {
    cid: Option<String>,
}
pub(super) async fn public(
    State(public): State<PublicState>,
    Query(query): Query<Parameters>,
    ws: WebSocketUpgrade,
) -> Response {
    let state = public.state.clone();
    if *state.stop.borrow() {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Some(id) = query.cid.filter(|id| !id.is_empty() && id.len() <= 1024) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let Ok(permit) = state.peer_status.capacity.clone().try_acquire_owned() else {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    };
    ws.max_message_size(64 * 1024)
        .max_frame_size(64 * 1024)
        .on_upgrade(move |socket| async move {
            let _permit = permit;
            connected(state, public.stop, id, socket).await;
        })
}
pub(super) fn emit(state: &ServerState, id: &str, online: bool) {
    if !id.is_empty() {
        let _ = state.events.send(crate::ws_event::WsEvent::broadcast(
            crate::chat::events::WS_PEER_STATUS_UPDATED,
            json!({"id":id,"online":online}).to_string(),
        ));
    }
    let _=state.events.send(crate::ws_event::WsEvent::broadcast(10002,json!({"id":id,"online":online,"runtimeId":state.peer_status.snapshot(&state.db)["runtimeId"]}).to_string()));
}
pub(super) struct ConnectionGuard {
    pub(super) state: ServerState,
    pub(super) lease: String,
}
impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        if let Some((peer, online)) = self
            .state
            .peer_status
            .connections
            .close(&self.state.db, &self.lease)
        {
            emit(&self.state, &peer, online);
        }
    }
}
async fn connected(
    state: ServerState,
    mut public_stop: tokio::sync::watch::Receiver<bool>,
    id: String,
    mut socket: WebSocket,
) {
    let mut stop = state.stop.clone();
    let authenticated = tokio::select! {
        _=stop.changed()=>None,
        _=public_stop.changed()=>None,
        frame=tokio::time::timeout(Duration::from_secs(10),socket.recv())=>match frame {
            Ok(Some(Ok(Message::Binary(body))))=>state.peer_status.connections.open(&state.db,&id,&body).ok(),
            _=>None,
        }
    };
    let Some((lease, changed)) = authenticated else {
        let _ = socket.close().await;
        return;
    };
    let _guard = ConnectionGuard {
        state: state.clone(),
        lease: lease.clone(),
    };
    if changed {
        emit(&state, &id, true);
    }
    let mut check = tokio::time::interval(Duration::from_secs(1));
    let sent = tokio::select! {_=stop.changed()=>false,_=public_stop.changed()=>false,result=socket.send(Message::Text("ok".into()))=>result.is_ok()};
    if sent {
        loop {
            tokio::select! {
                _=stop.changed()=>break,
                _=public_stop.changed()=>break,
                _=check.tick()=>if !state.peer_status.connections.valid(&state.db,&lease) { break; },
                frame=socket.recv()=>match frame {
                    Some(Ok(Message::Close(_)))|None|Some(Err(_))=>break,
                    _=>{},
                }
            }
        }
    }
    let _ = tokio::time::timeout(Duration::from_secs(1), socket.close()).await;
}
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
pub(super) enum Request {
    Snapshot {},
    Start {},
    Stop {},
    Reconnect {},
    EnsureAware {},
}
pub(super) async fn call(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(request): Json<Request>,
) -> Response {
    if !state.authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    if matches!(request, Request::EnsureAware {}) {
        return match super::status_outgoing::ensure_aware(&state).await {
            Ok(()) => Json(json!({"result":state.peer_status.snapshot(&state.db)})).into_response(),
            Err(error) => (
                StatusCode::BAD_REQUEST,
                Json(json!({"error":error.to_string()})),
            )
                .into_response(),
        };
    }
    let _control = state.peer_status.control.lock().await;
    let result = match request {
        Request::Snapshot {} => Ok(()),
        Request::Start {} => {
            if state
                .peer_status
                .public_active
                .load(std::sync::atomic::Ordering::SeqCst)
            {
                state.peer_status.outgoing.start(state.clone());
                Ok(())
            } else {
                Err(anyhow::anyhow!("Public server inactive"))
            }
        }
        Request::Stop {} => {
            state.peer_status.outgoing.stop().await;
            let previous = state.peer_status.connections.snapshot(&state.db);
            state.peer_status.connections.clear_hints();
            let current = state.peer_status.connections.snapshot(&state.db);
            if let Some(ids) = previous["online"].as_array() {
                for id in ids.iter().filter_map(|id| id.as_str()) {
                    if !current["online"]
                        .as_array()
                        .is_some_and(|ids| ids.iter().any(|current| current.as_str() == Some(id)))
                    {
                        emit(&state, id, false);
                    }
                }
            }
            emit(&state, "", false);
            Ok(())
        }
        Request::Reconnect {} => state.peer_status.outgoing.reconnect(&state),
        Request::EnsureAware {} => Ok(()),
    };
    match result {
        Ok(()) => Json(json!({"result":state.peer_status.snapshot(&state.db)})).into_response(),
        Err(error) => (
            StatusCode::BAD_REQUEST,
            Json(json!({"error":error.to_string()})),
        )
            .into_response(),
    }
}
#[cfg(all(test, feature = "http_transport"))]
#[path = "../../tests/unit/content_api/peer_status.rs"]
mod tests;
