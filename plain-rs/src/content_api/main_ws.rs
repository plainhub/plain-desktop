use super::server::ServerState;
use axum::{
    Json,
    extract::{
        ConnectInfo, Query, State, WebSocketUpgrade,
        ws::{CloseFrame, Message, WebSocket},
    },
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{collections::HashMap, net::SocketAddr, sync::Mutex, time::Duration};
use tokio::sync::mpsc;

#[derive(Default)]
pub(super) struct Runtime {
    connections: Mutex<HashMap<String, Connection>>,
}
struct Connection {
    client_id: String,
    registered: bool,
    outgoing: mpsc::Sender<Message>,
    pending: Option<String>,
    closing: tokio::sync::watch::Sender<Option<(u16, &'static str)>>,
}
impl Runtime {
    pub(super) fn snapshot(&self) -> Vec<String> {
        let mut ids: Vec<_> = self
            .connections
            .lock()
            .unwrap()
            .values()
            .filter(|c| c.registered && c.closing.borrow().is_none())
            .map(|c| c.client_id.clone())
            .collect();
        ids.sort();
        ids.dedup();
        ids
    }
    fn changed(&self, state: &ServerState) {
        let _ = state.events.send(crate::ws_event::WsEvent::broadcast(
            10006,
            json!(self.snapshot()).to_string(),
        ));
    }
    pub(super) fn close_client(&self, state: &ServerState, client_id: Option<&str>) {
        let connections = self.connections.lock().unwrap();
        for c in connections
            .values()
            .filter(|c| client_id.is_none_or(|id| c.client_id == id))
        {
            let _ = c.closing.send(Some((1000, "")));
        }
        drop(connections);
        self.changed(state);
    }
    pub(super) async fn complete(
        &self,
        state: &ServerState,
        request_id: &str,
        result: &Value,
    ) -> anyhow::Result<()> {
        let outgoing = self
            .connections
            .lock()
            .unwrap()
            .values()
            .find(|c| c.pending.as_deref() == Some(request_id))
            .map(|c| c.outgoing.clone());
        if let Some(outgoing) = outgoing {
            tokio::time::timeout(
                Duration::from_secs(2),
                outgoing.send(Message::Binary(crate::base64_decode(
                    result["frame"].as_str().unwrap_or_default(),
                ))),
            )
            .await??;
            completed(state, result).await;
        }
        Ok(())
    }
    pub(super) fn reject(&self, request_id: &str) {
        if let Some(c) = self
            .connections
            .lock()
            .unwrap()
            .values()
            .find(|c| c.pending.as_deref() == Some(request_id))
        {
            let _ = c.closing.send(Some((1013, "rejected")));
        }
    }
}
fn close(code: u16, reason: &'static str) -> Message {
    Message::Close(Some(CloseFrame {
        code,
        reason: reason.into(),
    }))
}
fn enabled(state: &ServerState) -> bool {
    state.prefs.get_user_or("service", false) && state.prefs.get_user_or("desktop_access", true)
}
#[derive(Deserialize)]
pub(super) struct Params {
    #[serde(default)]
    cid: String,
    auth: Option<String>,
}
pub(super) async fn upgrade(
    State(state): State<ServerState>,
    Query(params): Query<Params>,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
    ws: Option<WebSocketUpgrade>,
    request: axum::extract::Request,
) -> Response {
    let Some(ws) = ws else {
        return super::public_static::fallback(State(state), request).await;
    };
    ws.on_upgrade(move |socket| run(socket, state, params, remote.ip().to_string()))
        .into_response()
}
async fn completed(state: &ServerState, result: &Value) {
    let _ = state.host.call("systemWebLoginCompleted", json!({"request":result["request"],"chatPaired":result["response"]["chatPaired"],"clientIp":result["clientIp"]})).await;
}
async fn run(mut socket: WebSocket, mut state: ServerState, params: Params, ip: String) {
    if !enabled(&state) || params.cid.is_empty() {
        let reason = if params.cid.is_empty() {
            "`cid` is missing"
        } else {
            "desktop_access_disabled"
        };
        let _ = socket.send(close(1008, reason)).await;
        return;
    }
    let id = uuid::Uuid::new_v4().to_string();
    let (sender, mut outgoing) = mpsc::channel(8);
    let (closing, mut closed) = tokio::sync::watch::channel(None);
    state.main_ws.connections.lock().unwrap().insert(
        id.clone(),
        Connection {
            client_id: params.cid.clone(),
            registered: false,
            outgoing: sender,
            pending: None,
            closing,
        },
    );
    let mut registered = false;
    let mut receiver = state.events.subscribe();
    let mut pending: Option<String> = None;
    loop {
        let message = tokio::select! {
            _ = state.stop.changed() => break,
            _ = closed.changed() => { let (code, reason) = (*closed.borrow()).unwrap_or((1000, "")); let _ = socket.send(close(code, reason)).await; break; },
            message = outgoing.recv() => match message { Some(message) => message, None => break },
            incoming = socket.next() => {
                let frame = match incoming {
                    Some(Ok(Message::Binary(frame))) => frame,
                    Some(Ok(Message::Ping(bytes))) => { if socket.send(Message::Pong(bytes)).await.is_err() { break; } continue; },
                    Some(Ok(Message::Text(_) | Message::Pong(_))) => continue,
                    _ => break,
                };
                if !enabled(&state) { let _ = socket.send(close(1008,"desktop_access_disabled")).await; break; }
                if params.auth.as_deref() == Some("1") {
                    if let Some(request_id) = pending.take() { let _ = super::ws_login::execute(&state, super::ws_login::Request::Cancel { request_id }).await; }
                    match super::ws_login::execute(&state, super::ws_login::Request::Issue { client_id: params.cid.clone(), client_ip: ip.clone(), frame: crate::base64_encode(&frame) }).await {
                        Ok(result) => {
                            if socket.send(Message::Binary(crate::base64_decode(result["frame"].as_str().unwrap_or_default()))).await.is_err() { break; }
                            if result["status"] == "PENDING" {
                                let request_id = result["requestId"].as_str().unwrap().to_owned();
                                pending = Some(request_id.clone());
                                if let Some(c) = state.main_ws.connections.lock().unwrap().get_mut(&id) { c.pending = Some(request_id.clone()); }
                                if state.host.call("systemWebLoginRequest", json!({"clientId":params.cid,"clientIp":ip,"request":result["request"],"requestId":request_id})).await.is_err() { break; }
                            } else {
                                let mut result = result;
                                result["clientIp"] = json!(ip);
                                completed(&state, &result).await;
                            }
                            continue;
                        },
                        Err(error) => {
                            let reason = if error.to_string() == "too_many_login_attempts" { "too_many_login_attempts" } else { "invalid_password" };
                            let _ = socket.send(close(1013,reason)).await;
                            break;
                        }
                    }
                }
                let plain = super::sessions::key(&state.db, &params.cid).ok().flatten().and_then(|key| crate::crypto::xchacha_decrypt_raw(&key,&frame));
                let Some(plain) = plain else { let _ = socket.send(close(1013,"invalid_request")).await; break; };
                if !registered {
                    registered = true;
                    if let Some(c) = state.main_ws.connections.lock().unwrap().get_mut(&id) { c.registered = true; }
                    state.main_ws.changed(&state);
                    let _ = state.mms.replay();
                    let _ = state.host.call("systemWebSocketRegistered", json!({})).await;
                } else {
                    let controls = controls(&plain);
                    if !controls.is_empty() { let _ = state.host.call("systemScreenMirrorControls", json!({"inputs":controls})).await; }
                }
                continue;
            },
            event = receiver.recv(), if registered => {
                let event = match event { Ok(event) => event, Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => { let _ = socket.send(close(1013,"slow_client")).await; break; }, Err(_) => break };
                if event.event_type >= 10000 || event.target_cid.as_ref().is_some_and(|cid| cid != &params.cid) { continue; }
                if !enabled(&state) { break; }
                let Some(key) = super::sessions::key(&state.db, &params.cid).ok().flatten() else { break; };
                let bytes = if let Some(payload) = event.binary_payload {
                    let mut bytes = event.event_type.to_be_bytes().to_vec();
                    bytes.extend(payload);
                    bytes
                } else {
                    let payload = if event.event_type == crate::chat::events::WS_CHANNELS_UPDATED {
                        serde_json::from_str::<Value>(&event.payload).ok().and_then(|value|value.get("channels").cloned()).map(|value|value.to_string()).unwrap_or(event.payload)
                    } else { event.payload };
                    let Some(bytes) = crate::ws_frame::encode(event.event_type, payload.as_bytes(), &key) else { continue; };
                    bytes
                };
                Message::Binary(bytes)
            }
        };
        let closing = matches!(message, Message::Close(_));
        if !matches!(
            tokio::time::timeout(Duration::from_secs(2), socket.send(message)).await,
            Ok(Ok(()))
        ) || closing
        {
            break;
        }
    }
    if let Some(request_id) = pending {
        let _ =
            super::ws_login::execute(&state, super::ws_login::Request::Cancel { request_id }).await;
    }
    state.main_ws.connections.lock().unwrap().remove(&id);
    state.main_ws.changed(&state);
    if state.main_ws.snapshot().is_empty() {
        let _ = state
            .host
            .call("systemScreenMirrorResetTouch", json!({}))
            .await;
    }
    let _ = tokio::time::timeout(Duration::from_secs(1), socket.close()).await;
}
fn controls(bytes: &[u8]) -> Vec<Value> {
    if bytes.first() == Some(&0x54) {
        let Some(&count) = bytes.get(1) else {
            return vec![];
        };
        if count == 0 || bytes.len() < 4 + usize::from(count) * 8 {
            return vec![];
        }
        bytes[4..4 + usize::from(count)*8].chunks_exact(8).map(|p| json!({
            "action":match p[0] { 0=>"TOUCH_DOWN",1=>"TOUCH_MOVE",_=>"TOUCH_UP" },
            "pointerId":p[1],"x":f32::from(u16::from_le_bytes([p[2],p[3]]))/65535.0,"y":f32::from(u16::from_le_bytes([p[4],p[5]]))/65535.0
        })).collect()
    } else {
        let Ok(value) = serde_json::from_slice::<Value>(bytes) else {
            return vec![];
        };
        if value["type"] != "screenMirrorControl" {
            return vec![];
        }
        let input = value["input"].clone();
        if !matches!(
            input["action"].as_str(),
            Some(
                "TAP"
                    | "LONG_PRESS"
                    | "SWIPE"
                    | "SCROLL"
                    | "BACK"
                    | "HOME"
                    | "RECENTS"
                    | "LOCK_SCREEN"
                    | "KEY"
                    | "TOUCH"
                    | "TOUCH_DOWN"
                    | "TOUCH_MOVE"
                    | "TOUCH_UP"
            )
        ) {
            return vec![];
        }
        vec![input]
    }
}
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
pub(super) enum Request {
    Snapshot,
    CloseAll,
}
pub(super) async fn call(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(request): Json<Request>,
) -> Response {
    if !state.authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    match request {
        Request::Snapshot => Json(json!({"result":state.main_ws.snapshot()})).into_response(),
        Request::CloseAll => {
            state.main_ws.close_client(&state, None);
            Json(json!({"result":true})).into_response()
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/content_api/main_ws.rs"]
mod tests;
