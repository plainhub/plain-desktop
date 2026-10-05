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
pub(super) struct ServerState {
    schema: ContentSchema,
    pub(super) peer_status: Arc<super::peer_status::Runtime>,
    pub(super) mdns: Arc<super::mdns_runtime::Runtime>,
    pub(super) guest_replay: Arc<super::request_replay::Replay>,
    pub(super) main_replay: Arc<super::request_replay::Replay>,
    pub(super) peer_schema: super::peer_graphql::PeerSchema,
    pub(super) files: Arc<super::file_tasks::FileTasks>,
    pub(super) host: Arc<super::host::Host>,
    pub(super) thumbnails: Arc<super::thumbnails::Thumbnails>,
    pub(super) db: Arc<Db>,
    pub(super) lan: Arc<super::peer_lan::Lan>,
    pub(super) transport: Arc<crate::chat::transport_router::Router>,
    pub(super) previews: Arc<crate::link_preview::Schedule>,
    pub(super) prewarmer: Arc<crate::chat::prewarm::Prewarmer>,
    pub(super) shared_batches: Arc<super::shared_batch::Runtime>,
    pub(super) download_runtime: Arc<super::download_queue::Runtime>,
    pub(super) downloads: Arc<crate::chat::download_queue::Queue>,
    pub(super) attachments: Arc<crate::chat::attachment_imports::Imports>,
    pub(super) delivery: Arc<crate::chat::delivery::Delivery>,
    pub(super) channel_delivery: Arc<tokio::sync::Semaphore>,
    pub(super) ble_pairing: Arc<super::ble_pairing::Pairings>,
    pub(super) pairing_runtime: Arc<super::pairing_runtime::Runtime>,
    pub(super) nearby_devices: Arc<crate::chat::nearby_devices::Devices>,
    pub(super) ble_scans: Arc<crate::chat::nearby_scan::Scans>,
    pub(super) pairing: Arc<crate::chat::pairing::sessions::Sessions>,
    pub(super) prefs: Arc<crate::prefs::Prefs>,
    pub(super) directory: std::path::PathBuf,
    token: Arc<str>,
    pub(super) events: broadcast::Sender<WsEvent>,
    pub(super) stop: watch::Receiver<bool>,
    #[cfg(feature = "http_transport")]
    pub(super) bridge: Arc<super::http_bridge::HttpBridge>,
}
impl ServerState {
    pub(super) fn authenticated(&self, headers: &HeaderMap) -> bool {
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
    #[cfg(feature = "http_transport")]
    state: ServerState,
    task: JoinHandle<()>,
    stop: watch::Sender<bool>,
    #[cfg(feature = "http_transport")]
    bridge: Arc<super::http_bridge::HttpBridge>,
    #[cfg(feature = "http_transport")]
    public: tokio::sync::Mutex<Option<PublicServer>>,
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
        let lan = Arc::new(super::peer_lan::Lan::new().map_err(|e| e.to_string())?);
        let host = Arc::new(super::host::Host::default());
        let services =
            super::services::Services::new(db.clone(), host.clone(), events.clone(), prefs.clone());
        let schema = schema::build_with_services(
            db.clone(),
            events.clone(),
            prefs.clone(),
            path.parent().unwrap_or(Path::new(".")).to_path_buf(),
            host.clone(),
            services.clone(),
        );
        #[cfg(feature = "http_transport")]
        let bridge = Arc::new(super::http_bridge::HttpBridge::new(host.clone()));
        let attachments = Arc::new(crate::chat::attachment_imports::Imports::default());
        let downloads = Arc::new(crate::chat::download_queue::Queue::new(
            (*db).clone(),
            path.parent().unwrap_or(Path::new(".")).to_path_buf(),
            attachments.clone(),
        ));
        let preview_events = events.clone();
        let previews = Arc::new(crate::link_preview::Schedule::new(
            (*db).clone(),
            path.parent().unwrap_or(Path::new(".")).to_path_buf(),
            Arc::new(move |row| {
                let _ = preview_events.send(WsEvent::broadcast(
                    crate::chat::events::WS_MESSAGE_UPDATED,
                    serde_json::json!([crate::chat::service::chat_to_json(&row)]).to_string(),
                ));
            }),
            receiver.clone(),
        ));
        let state = ServerState {
            lan,
            schema,
            thumbnails: Arc::new(super::thumbnails::Thumbnails::default()),
            peer_status: Arc::new(super::peer_status::Runtime::default()),
            mdns: Arc::new(super::mdns_runtime::Runtime::default()),
            peer_schema: super::peer_graphql::schema(),
            guest_replay: Arc::new(super::request_replay::Replay::default()),
            main_replay: Arc::new(super::request_replay::Replay::default()),
            transport: Arc::new(crate::chat::transport_router::Router::default()),
            previews,
            prewarmer: Arc::new(crate::chat::prewarm::Prewarmer::default()),
            shared_batches: Arc::new(super::shared_batch::Runtime::default()),
            download_runtime: Arc::new(super::download_queue::Runtime::default()),
            downloads,
            attachments,
            delivery: Arc::new(crate::chat::delivery::Delivery::new((*db).clone())),
            channel_delivery: Arc::new(tokio::sync::Semaphore::new(4)),
            ble_pairing: Arc::new(super::ble_pairing::Pairings::default()),
            pairing_runtime: Arc::new(super::pairing_runtime::Runtime::default()),
            nearby_devices: Arc::new(crate::chat::nearby_devices::Devices::default()),
            ble_scans: Arc::new(crate::chat::nearby_scan::Scans::default()),
            pairing: Arc::new(crate::chat::pairing::sessions::Sessions::default()),
            files: services.files,
            host,
            db,
            prefs,
            directory: path.parent().unwrap_or(Path::new(".")).to_path_buf(),
            token: Arc::from(token),
            events,
            stop: receiver.clone(),
            #[cfg(feature = "http_transport")]
            bridge: bridge.clone(),
        };
        super::download_queue::start(state.clone());
        super::shared_batch::start(state.clone());
        super::pairing_timeout::start(
            state.pairing.clone(),
            state.events.clone(),
            receiver.clone(),
        );
        super::nearby_devices::start(state.clone());
        let mdns = state.mdns.clone();
        let mdns_host = state.host.clone();
        let mut mdns_stop = receiver.clone();
        tokio::spawn(async move {
            if !*mdns_stop.borrow() {
                let _ = mdns_stop.changed().await;
            }
            mdns.close(&mdns_host).await;
        });
        let router = Router::new()
            .route("/graphql", post(graphql))
            .route("/events", get(upgrade))
            .route("/host", get(host_upgrade))
            .route("/health", get(health))
            .route("/fs", get(files::file))
            .route(
                "/chat/discovery",
                post(super::discovery_advertisement::call),
            )
            .route("/chat/peer-status", post(super::peer_status::call))
            .route("/chat/mdns", post(super::mdns_runtime::call))
            .route("/chat/channel", post(super::channel_runtime::call))
            .route("/chat/pairing", post(super::pairing_runtime::call))
            .route("/chat/peer-graphql", post(super::peer_graphql::call))
            .route("/chat/lan/file", post(super::peer_lan::file))
            .route("/shares/client", post(super::shared_client::call))
            .route("/shares/client/file", post(super::shared_download::file))
            .route("/shares/client/zip", post(super::shared_zip::call))
            .route("/shares/batch", post(super::shared_batch::call))
            .route("/chat/transport", post(super::peer_transport::call))
            .route("/chat/prewarm", post(super::prewarm::call))
            .route("/chat/nearby-devices", post(super::nearby_devices::call))
            .route("/chat/nearby", post(super::nearby_http::call))
            .route("/chat/link-preview", post(super::link_preview::call))
            .route("/chat/download", post(super::download_queue::call))
            .route("/chat/attachment", post(super::attachment_imports::call))
            .route("/chat/send", post(super::chat_delivery::call))
            .route("/chat/service", post(super::chat_service::call))
            .route("/files/thumbnail", post(super::thumbnails::call))
            .route(
                "/files/thumbnail/output/:token",
                post(super::thumbnails::output),
            )
            .route("/chat/ble-http", post(super::ble_http::call))
            .route("/chat/store", post(super::chat_store_routes::call))
            .route("/files/write", post(super::file_writes::write))
            .route("/files/read", post(super::file_reads::read))
            .route("/files/stat", post(super::file_reads::stat))
            .route("/files/mutate", post(super::file_mutation_routes::mutate))
            .route("/system/sms-state", post(super::sms_state::call))
            .route("/system/sms", post(super::sms_query::call))
            .route("/system/providers", post(super::system_providers::call))
            .route("/system/media-buckets", post(super::media_buckets::call))
            .route("/system/permissions", post(super::system_permissions::call))
            .route("/system/provider-plan", post(super::provider_plan::call))
            .route(
                "/system/provider-delete",
                post(super::provider_deletes::call),
            )
            .route(
                "/system/notification",
                post(super::notification_actions::call),
            )
            .route("/http/guest", post(super::guest_graphql::call));
        #[cfg(feature = "http_transport")]
        let router = router.route("/http_host/:id", get(http_host_upgrade));
        let router = router
            .layer(DefaultBodyLimit::max(32 * 1024 * 1024))
            .with_state(state.clone());
        let listener = tokio::net::TcpListener::from_std(listener).map_err(|e| e.to_string())?;
        let task = tokio::spawn(async move {
            let mut stop = receiver;
            let _ = axum::serve(listener, router)
                .with_graceful_shutdown(async move {
                    let _ = stop.changed().await;
                })
                .await;
        });
        Ok(Self {
            port,
            #[cfg(feature = "http_transport")]
            state,
            task,
            stop,
            #[cfg(feature = "http_transport")]
            bridge,
            #[cfg(feature = "http_transport")]
            public: tokio::sync::Mutex::new(None),
        })
    }
    #[cfg(all(test, feature = "http_transport"))]
    pub(super) fn runtime_state(&self) -> ServerState {
        self.state.clone()
    }
    #[cfg(feature = "http_transport")]
    pub async fn start_public(
        &self,
        http: u16,
        https: u16,
        cert: Vec<u8>,
        key: Vec<u8>,
    ) -> Result<(u16, u16), String> {
        let mut guard = self.public.lock().await;
        if guard.is_some() {
            return Err("Public HTTP server is already running".into());
        }
        let (stop, receiver) = watch::channel(false);
        let peer_router = peer_routes(self.state.clone());
        let status_router = Router::new()
            .route("/status", get(super::peer_status::public))
            .with_state(super::peer_status::PublicState {
                state: self.state.clone(),
                stop: receiver.clone(),
            });
        let nearby_router = Router::new()
            .route("/nearby", post(super::nearby_public::call))
            .with_state(self.state.clone());
        let main_graphql_router = Router::new()
            .route("/graphql", post(super::main_graphql::call))
            .route("/health", get(super::main_graphql::health))
            .layer(DefaultBodyLimit::max(32 * 1024 * 1024))
            .with_state(self.state.clone());
        let router = Router::new()
            .fallback(super::http_bridge::handle)
            .layer(DefaultBodyLimit::max(64 * 1024 * 1024 * 1024))
            .with_state(super::http_bridge::HttpBridgeState {
                bridge: self.bridge.clone(),
                stop: receiver,
            })
            .merge(peer_router)
            .merge(status_router)
            .merge(nearby_router)
            .merge(main_graphql_router);
        let listeners =
            crate::http_transport::HttpListeners::start(router, http, https, cert, key).await?;
        let ports = (listeners.http_port, listeners.https_port);
        {
            let _control = self.state.peer_status.control.lock().await;
            self.state
                .peer_status
                .public_active
                .store(true, std::sync::atomic::Ordering::SeqCst);
            self.state.peer_status.outgoing.start(self.state.clone());
        }
        *guard = Some(PublicServer { listeners, stop });
        Ok(ports)
    }
    #[cfg(feature = "http_transport")]
    pub async fn stop_public(&self) {
        let mut guard = self.public.lock().await;
        let _control = self.state.peer_status.control.lock().await;
        self.state
            .peer_status
            .public_active
            .store(false, std::sync::atomic::Ordering::SeqCst);
        self.state.peer_status.outgoing.stop().await;
        if let Some(public) = guard.take() {
            let _ = public.stop.send(true);
            let PublicServer { listeners, stop: _ } = public;
            listeners.shutdown().await;
        }
    }
    pub async fn shutdown(mut self) {
        #[cfg(feature = "http_transport")]
        self.stop_public().await;
        #[cfg(feature = "http_transport")]
        self.state.shared_batches.shutdown(&self.state.host).await;
        self.state.download_runtime.shutdown().await;
        if self.state.host.connected() && self.state.host.needs_socket_cleanup() {
            let _ = self
                .state
                .host
                .call("peerTransportSocketCloseAll", serde_json::json!({}))
                .await;
        }
        let _ = self.stop.send(true);
        #[cfg(feature = "http_transport")]
        self.state.mdns.close(&self.state.host).await;
        let _ = (&mut self.task).await;
    }
}
impl Drop for ContentServer {
    fn drop(&mut self) {
        let _ = self.stop.send(true);
        self.task.abort();
    }
}
fn peer_routes(state: ServerState) -> Router {
    Router::new()
        .route("/peer_graphql", post(super::peer_graphql::public))
        .route("/guest_graphql", post(super::guest_graphql::public))
        .route("/fs", get(super::peer_files::file))
        .layer(DefaultBodyLimit::max(32 * 1024 * 1024))
        .with_state(state)
}
pub(super) fn peer_router(state: ServerState) -> Router {
    let routes = peer_routes(state.clone());
    #[cfg(feature = "http_transport")]
    {
        routes.merge(
            Router::new()
                .fallback(super::http_bridge::handle)
                .with_state(super::http_bridge::HttpBridgeState {
                    bridge: state.bridge.clone(),
                    stop: state.stop.clone(),
                }),
        )
    }
    #[cfg(not(feature = "http_transport"))]
    {
        routes
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
    tokio::spawn(super::mdns_runtime::Runtime::host_connected(
        state.clone(),
        generation,
    ));
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
pub(super) mod files;

#[cfg(feature = "http_transport")]
struct PublicServer {
    listeners: crate::http_transport::HttpListeners,
    stop: watch::Sender<bool>,
}
#[cfg(feature = "http_transport")]
async fn http_host_upgrade(
    State(state): State<ServerState>,
    axum::extract::Path(id): axum::extract::Path<String>,
    headers: HeaderMap,
    socket: WebSocketUpgrade,
) -> axum::response::Response {
    if !state.authenticated(&headers) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    socket
        .on_upgrade(move |socket| async move { state.bridge.attach(&id, socket).await })
        .into_response()
}
