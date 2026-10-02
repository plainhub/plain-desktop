use super::schema::{self, ContentSchema};
use crate::{db::Db, ws_event::WsEvent};
use async_graphql::Request;
use axum::{
    Json, Router,
    extract::{
        DefaultBodyLimit, Query, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
};
use std::{
    net::{Ipv4Addr, TcpListener},
    path::Path,
    sync::Arc,
};
use subtle::ConstantTimeEq;
use tokio::{
    sync::{broadcast, watch},
    task::JoinHandle,
};
#[derive(Clone)]
struct ServerState {
    schema: ContentSchema,
    host: Arc<super::host::Host>,
    db: Arc<Db>,
    prefs: Arc<crate::prefs::Prefs>,
    directory: std::path::PathBuf,
    token: Arc<str>,
    events: broadcast::Sender<WsEvent>,
    stop: watch::Receiver<bool>,
}
impl ServerState {
    fn authenticated(&self, headers: &HeaderMap) -> bool {
        let candidate = headers
            .get("authorization")
            .and_then(|h| h.to_str().ok())
            .and_then(|h| h.strip_prefix("Bearer "))
            .unwrap_or_default();
        candidate.as_bytes().ct_eq(self.token.as_bytes()).into()
    }
}
pub struct ContentServer {
    pub port: u16,
    task: JoinHandle<()>,
    stop: watch::Sender<bool>,
}
impl ContentServer {
    pub fn start(
        path: &Path,
        token: &str,
        prefs: Arc<crate::prefs::Prefs>,
    ) -> Result<Self, String> {
        if crate::base64_decode(token).len() != 32 {
            return Err("token must contain 32 random bytes".into());
        }
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).map_err(|e| e.to_string())?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let db = Arc::new(Db::open(path).map_err(|e| e.to_string())?);
        let (events, _) = broadcast::channel(256);
        let (stop, receiver) = watch::channel(false);
        let host = Arc::new(super::host::Host::default());
        let schema = schema::build_with_host(
            db.clone(),
            events.clone(),
            prefs.clone(),
            path.parent().unwrap_or(Path::new(".")).to_path_buf(),
            host.clone(),
        );
        let state = ServerState {
            schema,
            host,
            db,
            prefs,
            directory: path.parent().unwrap_or(Path::new(".")).to_path_buf(),
            token: Arc::from(token),
            events,
            stop: receiver.clone(),
        };
        let router = Router::new()
            .route("/graphql", post(graphql))
            .route("/events", get(upgrade))
            .route("/host", get(host_upgrade))
            .route("/health", get(health))
            .route("/fs", get(files::file))
            .layer(DefaultBodyLimit::max(32 * 1024 * 1024))
            .with_state(state);
        let listener = tokio::net::TcpListener::from_std(listener).map_err(|e| e.to_string())?;
        let task = tokio::spawn(async move {
            let mut stop = receiver;
            let _ = axum::serve(listener, router)
                .with_graceful_shutdown(async move {
                    let _ = stop.changed().await;
                })
                .await;
        });
        Ok(Self { port, task, stop })
    }
    pub async fn shutdown(mut self) {
        let _ = self.stop.send(true);
        let _ = (&mut self.task).await;
    }
}
impl Drop for ContentServer {
    fn drop(&mut self) {
        let _ = self.stop.send(true);
        self.task.abort();
    }
}
async fn health(State(s): State<ServerState>, headers: HeaderMap) -> impl IntoResponse {
    if s.authenticated(&headers) {
        StatusCode::OK
    } else {
        StatusCode::UNAUTHORIZED
    }
}
fn content_changes(request: &Request) -> bool {
    use async_graphql::parser::{
        parse_query,
        types::{OperationType, Selection},
    };
    let Ok(document) = parse_query(&request.query) else {
        return false;
    };
    document
        .operations
        .iter()
        .filter(|(name, _)| {
            request
                .operation_name
                .as_deref()
                .is_none_or(|selected| name.is_some_and(|n| n.as_str() == selected))
        })
        .any(|(_, operation)| {
            operation.node.ty == OperationType::Mutation
                && operation
                    .node
                    .selection_set
                    .node
                    .items
                    .iter()
                    .any(|selection| match &selection.node {
                        Selection::Field(field) => !matches!(
                            field.node.name.node.as_str(),
                            "broadcastImageEditorUpdate"
                                | "configurePomodoroDay"
                                | "tickPomodoro"
                                | "skipPomodoro"
                                | "adjustPomodoro"
                                | "startPomodoro"
                                | "pausePomodoro"
                                | "stopPomodoro"
                        ),
                        _ => true,
                    })
        })
}
async fn graphql(
    State(s): State<ServerState>,
    headers: HeaderMap,
    Json(request): Json<Request>,
) -> axum::response::Response {
    if !s.authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let mutation = content_changes(&request);
    let response = s.schema.execute(request).await;
    if mutation && response.errors.is_empty() {
        let _ = s.events.send(WsEvent::broadcast(47, "{}".into()));
    }
    Json(response).into_response()
}
async fn upgrade(
    State(s): State<ServerState>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> axum::response::Response {
    if !s.authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let receiver = s.events.subscribe();
    ws.on_upgrade(move |socket| events(socket, s, receiver))
        .into_response()
}
async fn events(
    mut socket: WebSocket,
    mut state: ServerState,
    mut receiver: broadcast::Receiver<WsEvent>,
) {
    if socket
        .send(Message::Text("{\"type\":47,\"payload\":\"{}\"}".into()))
        .await
        .is_err()
    {
        return;
    }
    loop {
        tokio::select! {
            _=state.stop.changed()=>break,
            message=socket.recv()=>match message {Some(Ok(Message::Ping(data)))=>{if socket.send(Message::Pong(data)).await.is_err(){break;}},Some(Ok(Message::Close(_)))|None|Some(Err(_))=>break,_=>{}},
            event=receiver.recv()=>{
                let message = match event {
                    Ok(ref e) => match &e.binary_payload {
                        Some(payload) => {
                            let mut frame = Vec::with_capacity(4 + payload.len());
                            frame.extend_from_slice(&e.event_type.to_le_bytes());
                            frame.extend_from_slice(&payload);
                            Message::Binary(frame)
                        }
                        None => Message::Text(serde_json::json!({"type":e.event_type,"payload":e.payload}).to_string()),
                    },
                    Err(broadcast::error::RecvError::Lagged(_)) => Message::Text("{\"type\":47,\"payload\":\"{}\"}".into()),
                    Err(_) => break,
                };
                if socket.send(message).await.is_err(){break;}

            }
        }
    }
}
async fn host_upgrade(
    State(state): State<ServerState>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> axum::response::Response {
    if !state.authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    upgrade
        .max_message_size(8 * 1024 * 1024)
        .max_frame_size(8 * 1024 * 1024)
        .on_upgrade(move |socket| host_socket(socket, state))
        .into_response()
}
async fn host_socket(mut socket: WebSocket, mut state: ServerState) {
    let (generation, mut outgoing) = state.host.connect();
    loop {
        tokio::select! {
            _ = state.stop.changed() => break,
            request = outgoing.recv() => match request {
                Some(request) => if socket.send(Message::Text(request.to_string())).await.is_err() { break; },
                None => break,
            },
            message = socket.recv() => match message {
                Some(Ok(Message::Text(text))) => {
                    match serde_json::from_str(&text) {
                        Ok(value) => if state.host.reply(generation, value).is_err() { break; },
                        Err(_) => break,
                    }
                },
                Some(Ok(Message::Ping(data))) => if socket.send(Message::Pong(data)).await.is_err() { break; },
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                _ => {},
            },
        }
    }
    state.host.disconnect(generation);
}
#[cfg(test)]
#[path = "../../tests/unit/content_api/server.rs"]
mod tests;

#[path = "files.rs"]
mod files;
