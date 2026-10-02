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
        let schema = schema::build(
            db.clone(),
            events.clone(),
            prefs.clone(),
            path.parent().unwrap_or(Path::new(".")).to_path_buf(),
        );
        let state = ServerState {
            schema,
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
            .route("/health", get(health))
            .route("/fs", get(file))
            .layer(DefaultBodyLimit::max(8 * 1024 * 1024))
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
async fn graphql(
    State(s): State<ServerState>,
    headers: HeaderMap,
    Json(request): Json<Request>,
) -> axum::response::Response {
    if !s.authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let mutation = request.query.trim_start().starts_with("mutation");
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
                let message=match event {Ok(e)=>serde_json::json!({"type":e.event_type,"payload":e.payload}).to_string(),Err(broadcast::error::RecvError::Lagged(_))=>"{\"type\":47,\"payload\":\"{}\"}".into(),Err(_)=>break};
                if socket.send(Message::Text(message)).await.is_err(){break;}
            }
        }
    }
}
#[cfg(test)]
#[path = "../../tests/unit/content_api/server.rs"]
mod tests;

#[derive(serde::Deserialize)]
struct FileQuery {
    id: String,
}
async fn file(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Query(query): Query<FileQuery>,
) -> axum::response::Response {
    if !state.authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Some(decoded) = crate::xchacha_decrypt(
        &crate::prefs::ensure_url_token(&state.prefs),
        &crate::base64_decode(&query.id),
    ) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Ok(uri) = String::from_utf8(decoded) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let path = Path::new(&uri);
    let Some(id) = path.file_stem().and_then(|v| v.to_str()) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let Some(record) = state.db.get_app_file(id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let owned = state.directory.join(record.real_path);
    if path != owned || !owned.starts_with(state.directory.join("files")) {
        return StatusCode::NOT_FOUND.into_response();
    }
    match tokio::fs::read(owned).await {
        Ok(bytes) => (
            [
                (axum::http::header::CONTENT_TYPE, record.mime_type),
                (
                    axum::http::header::CACHE_CONTROL,
                    "private, max-age=3600".into(),
                ),
            ],
            bytes,
        )
            .into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}
