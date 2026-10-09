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
    path::{Path, PathBuf},
    sync::Arc,
};
use subtle::ConstantTimeEq;
use tokio::{
    sync::{broadcast, watch},
    task::JoinHandle,
};
#[derive(Clone)]
pub(super) struct ServerState {
    #[cfg(feature = "http_transport")]
    pub(super) public_server: Arc<tokio::sync::Mutex<Option<PublicServer>>>,
    #[cfg(feature = "http_transport")]
    pub(super) build_debug: Arc<std::sync::atomic::AtomicBool>,
    #[cfg(feature = "http_transport")]
    pub(super) web_root: Arc<std::sync::RwLock<Option<PathBuf>>>,
    schema: ContentSchema,
    /// The contract schema the public `/graphql` executes. Built once next
    /// to the app's own schema so both share one host and one database.
    pub(super) public: super::public_schema::PublicSchema,
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
    pub(super) ble_transport: Arc<super::ble_http::Runtime>,
    pub(super) ble_pairing: Arc<super::ble_pairing::Pairings>,
    pub(super) pairing_runtime: Arc<super::pairing_runtime::Runtime>,
    pub(super) nearby_devices: Arc<crate::chat::nearby_devices::Devices>,
    pub(super) ble_scans: Arc<crate::chat::nearby_scan::Scans>,
    pub(super) pairing: Arc<crate::chat::pairing::sessions::Sessions>,
    pub(super) prefs: Arc<crate::prefs::Prefs>,
    pub(super) dlna: Arc<crate::dlna_receiver::receiver_engine::DlnaEngine>,
    pub(super) image_models: Arc<super::image_models::Runtime>,
    pub(super) mms: Arc<super::mms_send::Runtime>,
    pub(super) cast: Arc<super::dlna_sender_runtime::Runtime>,
    pub(super) audio: Arc<super::audio::Audio>,
    pub(super) main_ws: Arc<super::main_ws::Runtime>,
    pub(super) login_attempts: Arc<std::sync::Mutex<super::ws_login::LoginAttempts>>,
    pub(super) directory: std::path::PathBuf,
    pub(super) token: Arc<str>,
    pub(super) events: broadcast::Sender<WsEvent>,
    pub(super) stop: watch::Receiver<bool>,
    #[cfg(feature = "http_transport")]
    pub(super) bridge: Arc<super::native_resources::NativeResources>,
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
        let directory = path.parent().unwrap_or(Path::new(".")).to_path_buf();
        let schema = schema::build_with_services(
            db.clone(),
            events.clone(),
            prefs.clone(),
            directory.clone(),
            host.clone(),
            services.clone(),
        );
        let mms = super::mms_send::Runtime::new(
            host.clone(),
            prefs.clone(),
            db.clone(),
            directory.clone(),
            events.clone(),
        );
        let image_models = super::image_models::Runtime::new(
            directory.clone(),
            host.clone(),
            prefs.clone(),
            services.index.clone(),
            events.clone(),
        );
        let public = super::public_schema::build_with_runtime(
            host.clone(),
            events.clone(),
            prefs.clone(),
            db.clone(),
            directory,
            mms.clone(),
            services.clone(),
            image_models.clone(),
        );
        #[cfg(feature = "http_transport")]
        let bridge = Arc::new(super::native_resources::NativeResources::new(host.clone()));
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
            #[cfg(feature = "http_transport")]
            public_server: Arc::new(tokio::sync::Mutex::new(None)),
            public,
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
            ble_transport: Arc::new(super::ble_http::Runtime::default()),
            ble_pairing: Arc::new(super::ble_pairing::Pairings::default()),
            pairing_runtime: Arc::new(super::pairing_runtime::Runtime::default()),
            nearby_devices: Arc::new(crate::chat::nearby_devices::Devices::default()),
            ble_scans: Arc::new(crate::chat::nearby_scan::Scans::default()),
            pairing: Arc::new(crate::chat::pairing::sessions::Sessions::default()),
            dlna: Arc::new(crate::dlna_receiver::receiver_engine::DlnaEngine::new()),
            files: services.files,
            audio: services.audio,
            host,
            db,
            prefs,
            image_models: image_models.clone(),
            mms,
            cast: Arc::new(super::dlna_sender_runtime::Runtime::default()),
            main_ws: Arc::new(super::main_ws::Runtime::default()),
            login_attempts: Arc::new(std::sync::Mutex::new(
                super::ws_login::LoginAttempts::default(),
            )),
            directory: path.parent().unwrap_or(Path::new(".")).to_path_buf(),
            build_debug: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            web_root: Arc::new(std::sync::RwLock::new(None)),
            token: Arc::from(token),
            events,
            stop: receiver.clone(),
            #[cfg(feature = "http_transport")]
            bridge: bridge.clone(),
        };
        #[cfg(feature = "http_transport")]
        state.mms.set_resources(bridge.clone(), receiver.clone());
        super::download_queue::start(state.clone());
        super::shared_batch::start(state.clone());
        super::pairing_timeout::start(
            state.pairing.clone(),
            state.events.clone(),
            receiver.clone(),
        );
        super::nearby_devices::start(state.clone());
        image_models.follow(receiver.clone());
        state.mms.follow(receiver.clone());
        super::public_dlna::follow_changes(&state);
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
            .route("/chat/ble-exchange/:token", get(super::ble_http::outgoing))
            .route("/chat/ble-incoming", get(super::ble_http::incoming))
            .route("/chat/store", post(super::chat_store_routes::call))
            .route("/files/write", post(super::file_writes::write))
            .route("/files/read", post(super::file_reads::read))
            .route("/files/stat", post(super::file_reads::stat))
            .route("/files/mutate", post(super::file_mutation_routes::mutate))
            .route("/system/sms-state", post(super::sms_state::call))
            .route("/system/sms", post(super::sms_query::call))
            .route("/system/sms-send", post(super::sms_send::call))
            .route("/system/contact-write", post(super::contact_write::call))
            .route("/system/ws-login", post(super::ws_login::call))
            .route("/system/ws-runtime", post(super::main_ws::call))
            .route(
                "/system/dlna-sender",
                post(super::dlna_sender_runtime::call),
            )
            .route("/system/mms-runtime", post(super::mms_send::call))
            .route("/system/sessions", post(super::sessions::call))
            .route("/system/providers", post(super::system_providers::call))
            .route("/system/preferences", post(super::preferences::call))
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
            .route("/dlna/receiver", post(super::public_dlna::call))
            .route("/system/image-models", post(super::image_models::call))
            .route("/system/image-search", post(super::image_search::call))
            .route("/http/guest", post(super::guest_graphql::call));
        #[cfg(feature = "http_transport")]
        let router = router
            .route(
                "/system/http-server/health",
                post(super::public_lifecycle::health),
            )
            .route("/system/tls", post(super::tls_identity::call))
            .route("/resources/:id", get(resource_upgrade))
            .route(
                "/system/notification-event",
                post(super::public_notifications::publish),
            );
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
        let mut guard = self.state.public_server.lock().await;
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
            .route("/proxyfs", get(super::public_proxy::call))
            .route("/zip/dir", get(super::public_zip::dir))
            .route("/zip/files", get(super::public_zip::files))
            .route(
                "/upload",
                post(super::public_upload::upload)
                    .layer(DefaultBodyLimit::max(15 * 60 * 1000 * 1000)),
            )
            .route(
                "/upload_chunk",
                post(super::public_upload::upload_chunk)
                    .layer(DefaultBodyLimit::max(15 * 60 * 1000 * 1000)),
            )
            .route("/", get(super::main_ws::upgrade))
            .route("/media/:id", get(super::cast_runtime::media))
            .route(
                "/callback/cast",
                axum::routing::any(super::cast_runtime::callback),
            )
            .route("/init", post(super::main_graphql::init))
            .route("/shutdown", get(super::main_graphql::shutdown))
            .layer(DefaultBodyLimit::max(32 * 1024 * 1024))
            .with_state(self.state.clone());
        let router = Router::new()
            .fallback(super::public_static::fallback)
            .layer(DefaultBodyLimit::max(64 * 1024 * 1024 * 1024))
            .with_state(self.state.clone())
            .merge(peer_router)
            .merge(status_router)
            .merge(nearby_router)
            .merge(main_graphql_router)
            .merge(super::public_dlna::router(self.state.clone()))
            .layer(axum::middleware::from_fn_with_state(
                self.state.clone(),
                super::public_cors::apply,
            ));
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
        let generation = super::public_lifecycle::next_generation();
        let failure = listeners.failures();
        *guard = Some(PublicServer {
            listeners,
            stop,
            generation,
        });
        super::public_lifecycle::monitor(self.state.clone(), failure, receiver, generation);
        Ok(ports)
    }
    #[cfg(feature = "http_transport")]
    pub async fn stop_public(&self) {
        let mut guard = self.state.public_server.lock().await;
        super::public_lifecycle::stop_locked(&self.state, &mut guard).await;
    }
    #[cfg(feature = "http_transport")]
    pub async fn public_generation(&self) -> Option<u64> {
        self.state
            .public_server
            .lock()
            .await
            .as_ref()
            .map(|server| server.generation)
    }
    #[cfg(feature = "http_transport")]
    pub fn set_build_debug(&self, debug: bool) {
        self.state
            .build_debug
            .store(debug, std::sync::atomic::Ordering::Relaxed);
    }
    /// The bundle root the host unpacked this run. Empty means the SPA is not
    /// served, which is the desktop case where the bundle is loaded by the
    /// webview itself.
    #[cfg(feature = "http_transport")]
    pub fn set_web_root(&self, root: &str) {
        if let Ok(mut guard) = self.state.web_root.write() {
            *guard = (!root.is_empty()).then(|| PathBuf::from(root));
        }
    }
    pub async fn shutdown(mut self) {
        #[cfg(feature = "http_transport")]
        self.stop_public().await;
        #[cfg(feature = "http_transport")]
        self.state.shared_batches.shutdown(&self.state.host).await;
        self.state.download_runtime.shutdown().await;
        self.state.mms.shutdown().await;
        self.state.image_models.shutdown().await;
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
        #[cfg(feature = "http_transport")]
        if let Ok(mut guard) = self.state.public_server.try_lock() {
            guard.take();
        }
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
    peer_routes(state)
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
            message=socket.recv()=>match message {Some(Ok(Message::Ping(data)))=>{if socket.send(Message::Pong(data)).await.is_err(){break;}},Some(Ok(Message::Close(_)))|None|Some(Err(_))=>break,
            Some(Ok(Message::Text(text))) => {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) {
                    if let (Some(kind),Some(payload))=(value["type"].as_i64().and_then(|v|i32::try_from(v).ok()),value["payload"].as_str()) {
                        if (0..10000).contains(&kind) { let _=state.events.send(WsEvent::broadcast(kind,payload.to_owned())); }
                    }
                }
            },
            Some(Ok(Message::Binary(bytes))) if bytes.len()>=4 => {
                let kind=i32::from_le_bytes(bytes[..4].try_into().unwrap());
                if (0..10000).contains(&kind) { let _=state.events.send(WsEvent::broadcast_binary(kind,bytes[4..].to_vec())); }
            },_=>{}},
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
pub(super) struct PublicServer {
    pub(super) listeners: crate::http_transport::HttpListeners,
    pub(super) stop: watch::Sender<bool>,
    pub(super) generation: u64,
}
#[cfg(feature = "http_transport")]
async fn resource_upgrade(
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
