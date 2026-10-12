use crate::assembly::server::ServerState;
use axum::{
    extract::{
        Query, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::json;
use std::{sync::Arc, time::Duration};

pub(crate) struct Runtime {
    pub(crate) connections: plain_rs::peers::status::Connections,
    capacity: Arc<tokio::sync::Semaphore>,
    pub(crate) outgoing: Arc<crate::services::peers::status_outgoing::Outgoing>,
    pub(crate) control: tokio::sync::Mutex<()>,
    pub(crate) public_active: std::sync::atomic::AtomicBool,
}
impl Runtime {
    fn snapshot(&self, db: &plain_rs::db::Db) -> serde_json::Value {
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
            outgoing: Arc::new(crate::services::peers::status_outgoing::Outgoing::new()),
            capacity: Arc::new(tokio::sync::Semaphore::new(128)),
        }
    }
}
#[derive(Clone)]
pub(crate) struct PublicState {
    pub(crate) state: ServerState,
    pub(crate) stop: tokio::sync::watch::Receiver<bool>,
}
#[derive(Deserialize)]
pub(crate) struct Parameters {
    cid: Option<String>,
}
pub(crate) async fn public(
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
pub(crate) fn emit(state: &ServerState, id: &str, online: bool) {
    if !id.is_empty() {
        let _ = state.events.send(plain_rs::events::WsEvent::broadcast(
            plain_rs::chat::events::WS_PEER_STATUS_UPDATED,
            json!({"id":id,"online":online}).to_string(),
        ));
    }
    let _=state.events.send(plain_rs::events::WsEvent::broadcast("PEER_CONNECTIONS_UPDATED",json!({"id":id,"online":online,"runtimeId":state.peer_status.snapshot(&state.db)["runtimeId"]}).to_string()));
}
pub(crate) struct ConnectionGuard {
    pub(crate) state: ServerState,
    pub(crate) lease: String,
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
pub(crate) enum Request {
    Snapshot {},
    Start {},
    Stop {},
    Reconnect {},
    EnsureAware {},
}
#[cfg(all(test, feature = "http_transport"))]
#[path = "../../../tests/unit/services/peers/peer_status.rs"]
mod tests;
