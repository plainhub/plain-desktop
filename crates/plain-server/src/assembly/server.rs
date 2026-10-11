use crate::graphql::schemas::host as schema;
use crate::graphql::schemas::host::ContentSchema;
use async_graphql::Request;
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, State, WebSocketUpgrade},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
};
use plain_rs::db::Db;
use plain_rs::events::WsEvent;
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
pub(crate) struct ServerState {
    #[cfg(feature = "http_transport")]
    pub(crate) public_server: Arc<tokio::sync::Mutex<Option<PublicServer>>>,
    #[cfg(feature = "http_transport")]
    pub(crate) build_debug: Arc<std::sync::atomic::AtomicBool>,
    #[cfg(feature = "http_transport")]
    pub(crate) web_root: Arc<std::sync::RwLock<Option<PathBuf>>>,
    pub(crate) schema: ContentSchema,
    /// The contract schema the public `/graphql` executes. Built once next
    /// to the app's own schema so both share one host and one database.
    pub(crate) public: crate::graphql::schemas::public::PublicSchema,
    pub(crate) peer_status: Arc<crate::services::peers::peer_status::Runtime>,
    pub(crate) mdns: Arc<crate::services::discovery::mdns_runtime::Runtime>,
    pub(crate) guest_replay: Arc<crate::http::auth::replay::Replay>,
    pub(crate) main_replay: Arc<crate::http::auth::replay::Replay>,
    pub(crate) peer_schema: crate::graphql::schemas::peer_host::PeerSchema,
    pub(crate) files: Arc<crate::services::files::file_tasks::FileTasks>,
    pub(crate) host: Arc<crate::host::bridge::Host>,
    pub(crate) thumbnails: Arc<crate::services::images::thumbnails::Thumbnails>,
    pub(crate) db: Arc<Db>,
    pub(crate) lan: Arc<crate::services::peers::peer_lan::Lan>,
    pub(crate) transport: Arc<plain_rs::transport::router::Router>,
    pub(crate) previews: Arc<crate::services::link_preview::Schedule>,
    pub(crate) prewarmer: Arc<plain_rs::chat::attachments::prewarm::Prewarmer>,
    pub(crate) shared_batches: Arc<crate::services::peers::shared_batch::Runtime>,
    pub(crate) download_runtime: Arc<crate::services::chat::download_queue::Runtime>,
    pub(crate) downloads: Arc<plain_rs::chat::attachments::download_queue::Queue>,
    pub(crate) delivery: Arc<plain_rs::chat::delivery::Delivery>,
    pub(crate) channel_delivery: Arc<tokio::sync::Semaphore>,
    pub(crate) ble_transport: Arc<crate::services::discovery::ble_http::Runtime>,
    pub(crate) ble_pairing: Arc<crate::services::discovery::ble_pairing::Pairings>,
    pub(crate) pairing_runtime: Arc<crate::services::discovery::pairing_runtime::Runtime>,
    pub(crate) nearby_devices: Arc<plain_rs::discovery::nearby_devices::Devices>,
    pub(crate) ble_scans: Arc<plain_rs::discovery::nearby_scan::Scans>,
    pub(crate) ble_scan: Arc<crate::services::discovery::ble_scan::Runtime>,
    pub(crate) pairing: Arc<plain_rs::peers::pairing::sessions::Sessions>,
    pub(crate) prefs: Arc<plain_rs::prefs::Prefs>,
    pub(crate) dlna: Arc<crate::services::casting::receiver::receiver_engine::DlnaEngine>,
    pub(crate) image_models: Arc<crate::services::images::models::Runtime>,
    #[cfg(feature = "mobile")]
    pub(crate) mobile: Arc<crate::mobile::Runtime>,
    pub(crate) cast: Arc<crate::services::casting::dlna_sender_runtime::Runtime>,
    pub(crate) audio: Arc<crate::services::audio::host::Audio>,
    pub(crate) main_ws: Arc<crate::http::websocket::host::Runtime>,
    pub(crate) login_attempts: Arc<std::sync::Mutex<crate::http::auth::ws_login::LoginAttempts>>,
    pub(crate) directory: std::path::PathBuf,
    pub(crate) token: Arc<str>,
    pub(crate) events: broadcast::Sender<WsEvent>,
    pub(crate) stop: watch::Receiver<bool>,
    #[cfg(feature = "http_transport")]
    pub(crate) bridge: Arc<crate::host::resources::NativeResources>,
}
impl ServerState {
    pub(crate) fn authenticated(&self, headers: &HeaderMap) -> bool {
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
    state: ServerState,
    task: Option<JoinHandle<()>>,
    #[cfg(feature = "http_transport")]
    local_router: Option<Router>,
    stop: watch::Sender<bool>,
}
impl ContentServer {
    pub fn graphql_executor(&self) -> crate::graphql::executor::GraphqlExecutor {
        crate::graphql::executor::GraphqlExecutor::new(self.state.clone())
    }
    pub fn host_events(&self) -> crate::host::events::HostEvents {
        crate::host::events::HostEvents::new(self.state.clone())
    }

    /// The user-settings store as JSON, defaults merged — the shape the App's
    /// native bridge reads to seed its flows.
    pub fn user_settings_json(&self) -> Result<String, String> {
        let snapshot =
            plain_rs::prefs::user::snapshot(&self.state.prefs).map_err(|e| e.to_string())?;
        serde_json::to_string(&snapshot).map_err(|e| e.to_string())
    }

    /// Applies one user-settings patch on behalf of the App's native (JNI/C
    /// ABI) caller. Runs the same [`apply_user_patch`] the GraphQL resolvers
    /// use, so a preference written from Kotlin and one written by a remote
    /// client validate, persist and broadcast identically.
    pub async fn update_user_prefs(&self, patch_json: &str) -> Result<bool, String> {
        use crate::graphql::resolvers::settings::state::{UserPatchEnv, apply_user_patch};
        let patch: plain_rs::prefs::user::UserSettingsPatch =
            serde_json::from_str(patch_json).map_err(|e| e.to_string())?;
        apply_user_patch(UserPatchEnv::from_server(&self.state), patch)
            .await
            .map_err(|error| error.message)
    }

    /// Runs one user-settings action on behalf of the App's native (JNI/C
    /// ABI) caller, through the same write path as [Self::update_user_prefs].
    pub async fn run_user_action(&self, action_json: &str) -> Result<bool, String> {
        use crate::graphql::resolvers::settings::state::{UserPatchEnv, apply_user_action};
        let action: plain_rs::prefs::user::Action =
            serde_json::from_str(action_json).map_err(|e| e.to_string())?;
        apply_user_action(UserPatchEnv::from_server(&self.state), action)
            .await
            .map_err(|error| error.message)
    }

    fn build(
        path: &Path,
        token: &str,
        prefs: Arc<plain_rs::prefs::Prefs>,
    ) -> Result<
        (
            ServerState,
            Router,
            watch::Sender<bool>,
            watch::Receiver<bool>,
        ),
        String,
    > {
        if plain_rs::utils::base64::base64_decode(token).len() != 32 {
            return Err("token must contain 32 random bytes".into());
        }
        let db = Arc::new(Db::open(path).map_err(|e| e.to_string())?);
        let (events, _) = broadcast::channel(256);
        let (stop, receiver) = watch::channel(false);
        let lan =
            Arc::new(crate::services::peers::peer_lan::Lan::new().map_err(|e| e.to_string())?);
        let host = Arc::new(crate::host::bridge::Host::default());
        let services = crate::assembly::services::Services::new(
            db.clone(),
            host.clone(),
            events.clone(),
            prefs.clone(),
        );
        let directory = path.parent().unwrap_or(Path::new(".")).to_path_buf();
        let schema = schema::build_with_services(
            db.clone(),
            events.clone(),
            prefs.clone(),
            directory.clone(),
            host.clone(),
            services.clone(),
        );
        #[cfg(feature = "mobile")]
        let mobile = crate::mobile::Runtime::new(
            host.clone(),
            prefs.clone(),
            db.clone(),
            directory.clone(),
            events.clone(),
        );
        let image_models = crate::services::images::models::Runtime::new(
            directory.clone(),
            host.clone(),
            prefs.clone(),
            services.index.clone(),
            events.clone(),
        );
        let public = crate::graphql::schemas::public::build_with_runtime(
            host.clone(),
            events.clone(),
            prefs.clone(),
            db.clone(),
            directory,
            services.clone(),
            image_models.clone(),
            #[cfg(feature = "mobile")]
            mobile.clone(),
        );
        #[cfg(feature = "http_transport")]
        let bridge = Arc::new(crate::host::resources::NativeResources::new(host.clone()));
        let attachments =
            Arc::new(plain_rs::chat::attachments::attachment_imports::Imports::default());
        let downloads = Arc::new(plain_rs::chat::attachments::download_queue::Queue::new(
            (*db).clone(),
            path.parent().unwrap_or(Path::new(".")).to_path_buf(),
            attachments.clone(),
        ));
        let preview_events = events.clone();
        let previews = Arc::new(crate::services::link_preview::Schedule::new(
            (*db).clone(),
            path.parent().unwrap_or(Path::new(".")).to_path_buf(),
            Arc::new(move |row| {
                let _ = preview_events.send(WsEvent::broadcast(
                    plain_rs::chat::events::WS_MESSAGE_UPDATED,
                    serde_json::json!([plain_rs::chat::service::chat_to_json(&row)]).to_string(),
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
            thumbnails: Arc::new(crate::services::images::thumbnails::Thumbnails::default()),
            peer_status: Arc::new(crate::services::peers::peer_status::Runtime::default()),
            mdns: Arc::new(crate::services::discovery::mdns_runtime::Runtime::default()),
            peer_schema: crate::graphql::schemas::peer_host::schema(),
            guest_replay: Arc::new(crate::http::auth::replay::Replay::default()),
            main_replay: Arc::new(crate::http::auth::replay::Replay::default()),
            transport: Arc::new(plain_rs::transport::router::Router::default()),
            previews,
            prewarmer: Arc::new(plain_rs::chat::attachments::prewarm::Prewarmer::default()),
            shared_batches: Arc::new(crate::services::peers::shared_batch::Runtime::default()),
            download_runtime: Arc::new(crate::services::chat::download_queue::Runtime::default()),
            downloads,
            delivery: Arc::new(plain_rs::chat::delivery::Delivery::new((*db).clone())),
            channel_delivery: Arc::new(tokio::sync::Semaphore::new(4)),
            ble_transport: Arc::new(crate::services::discovery::ble_http::Runtime::default()),
            ble_pairing: Arc::new(crate::services::discovery::ble_pairing::Pairings::default()),
            pairing_runtime: Arc::new(
                crate::services::discovery::pairing_runtime::Runtime::default(),
            ),
            nearby_devices: Arc::new(plain_rs::discovery::nearby_devices::Devices::default()),
            ble_scans: Arc::new(plain_rs::discovery::nearby_scan::Scans::default()),
            ble_scan: Arc::new(crate::services::discovery::ble_scan::Runtime::default()),
            pairing: Arc::new(plain_rs::peers::pairing::sessions::Sessions::default()),
            dlna: Arc::new(crate::services::casting::receiver::receiver_engine::DlnaEngine::new()),
            files: services.files,
            audio: services.audio,
            host,
            db,
            prefs,
            image_models: image_models.clone(),
            #[cfg(feature = "mobile")]
            mobile,
            cast: Arc::new(crate::services::casting::dlna_sender_runtime::Runtime::default()),
            main_ws: Arc::new(crate::http::websocket::host::Runtime::default()),
            login_attempts: Arc::new(std::sync::Mutex::new(
                crate::http::auth::ws_login::LoginAttempts::default(),
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
        #[cfg(feature = "mobile")]
        state
            .mobile
            .mms
            .set_resources(bridge.clone(), receiver.clone());
        crate::services::chat::download_queue::start(state.clone());
        crate::services::peers::shared_batch::start(state.clone());
        crate::services::discovery::pairing_timeout::start(
            state.pairing.clone(),
            state.events.clone(),
            receiver.clone(),
        );
        crate::services::discovery::nearby_devices::start(state.clone());
        crate::services::discovery::ble_scan::start(state.clone());
        image_models.follow(receiver.clone());
        #[cfg(feature = "mobile")]
        state.mobile.mms.follow(receiver.clone());
        crate::graphql::resolvers::casting::control::follow_changes(&state);
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
            .route("/health", get(health))
            .route("/fs", get(crate::http::routes::files::file))
            .route(
                "/chat/discovery",
                post(crate::services::discovery::discovery_advertisement::call),
            )
            .route(
                "/chat/peer-status",
                post(crate::services::peers::peer_status::call),
            )
            .route(
                "/chat/mdns",
                post(crate::services::discovery::mdns_runtime::call),
            )
            .route(
                "/chat/channel",
                post(crate::services::chat::channel_runtime::call),
            )
            .route(
                "/chat/pairing",
                post(crate::services::discovery::pairing_runtime::call),
            )
            .route(
                "/shares/client",
                post(crate::services::peers::shared_client::call),
            )
            .route(
                "/shares/client/file",
                post(crate::services::peers::shared_download::file),
            )
            .route(
                "/shares/client/zip",
                post(crate::services::peers::shared_zip::call),
            )
            .route(
                "/shares/batch",
                post(crate::services::peers::shared_batch::call),
            )
            .route(
                "/chat/transport",
                post(crate::services::peers::peer_transport::call),
            )
            .route("/chat/prewarm", post(crate::services::chat::prewarm::call))
            .route(
                "/chat/nearby-devices",
                post(crate::services::discovery::nearby_devices::call),
            )
            .route(
                "/chat/nearby",
                post(crate::services::discovery::nearby_http::call),
            )
            .route(
                "/chat/link-preview",
                post(crate::services::chat::link_preview::call),
            )
            .route(
                "/chat/download",
                post(crate::services::chat::download_queue::call),
            )
            .route(
                "/chat/send",
                post(crate::services::chat::chat_delivery::call),
            )
            .route(
                "/chat/service",
                post(crate::services::chat::chat_service::call),
            )
            .route(
                "/files/thumbnail/output/:token",
                post(crate::services::images::thumbnails::output),
            )
            .route(
                "/chat/ble-incoming",
                get(crate::services::discovery::ble_http::incoming),
            )
            .route(
                "/chat/store",
                post(crate::services::chat::chat_store_routes::call),
            )
            .route(
                "/files/write",
                post(crate::http::routes::file_writes::write),
            )
            .route("/files/read", post(crate::http::routes::file_reads::read))
            .route(
                "/files/mutate",
                post(crate::http::routes::file_mutations::mutate),
            )
            .route("/system/ws-login", post(crate::http::auth::ws_login::call))
            .route(
                "/system/ws-runtime",
                post(crate::http::websocket::host::call),
            )
            .route(
                "/system/dlna-sender",
                post(crate::services::casting::dlna_sender_runtime::call),
            )
            .route("/system/sessions", post(crate::http::sessions::host::call))
            .route("/system/providers", post(crate::http::routes::system::call))
            .route(
                "/system/media-buckets",
                post(crate::services::files::media_buckets::call),
            )
            .route(
                "/system/provider-plan",
                post(crate::http::routes::provider_query::call),
            )
            .route(
                "/dlna/receiver",
                post(crate::graphql::resolvers::casting::control::call),
            )
            .route(
                "/system/image-models",
                post(crate::services::images::models::call),
            )
            .route(
                "/system/image-search",
                post(crate::services::images::search::call),
            );
        #[cfg(feature = "http_transport")]
        let router = router
            .route("/system/tls", post(crate::http::tls_identity_host::call))
            .route("/resources/:id", get(resource_upgrade))
            .route(
                "/system/notification-event",
                post(crate::graphql::resolvers::notifications::public::publish),
            );
        let router = router
            .layer(DefaultBodyLimit::max(32 * 1024 * 1024))
            .with_state(state.clone());
        #[cfg(feature = "mobile")]
        let router = router.merge(crate::mobile::http::routes(state.clone()));
        Ok((state, router, stop, receiver))
    }
    pub fn start(
        path: &Path,
        token: &str,
        prefs: Arc<plain_rs::prefs::Prefs>,
    ) -> Result<Self, String> {
        let (state, router, stop, mut receiver) = Self::build(path, token, prefs)?;
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).map_err(|e| e.to_string())?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let listener = tokio::net::TcpListener::from_std(listener).map_err(|e| e.to_string())?;
        let task = tokio::spawn(async move {
            let _ = axum::serve(listener, router)
                .with_graceful_shutdown(async move {
                    let _ = receiver.changed().await;
                })
                .await;
        });
        Ok(Self {
            port,
            state,
            task: Some(task),
            stop,
            #[cfg(feature = "http_transport")]
            local_router: None,
        })
    }
    #[cfg(feature = "http_transport")]
    pub async fn start_http(
        path: &Path,
        token: &str,
        prefs: Arc<plain_rs::prefs::Prefs>,
        http: u16,
        https: u16,
        cert: Vec<u8>,
        key: Vec<u8>,
        debug: bool,
        web_root: &str,
    ) -> Result<Self, String> {
        let (state, router, stop, _) = Self::build(path, token, prefs)?;
        let mut server = Self {
            port: 0,
            state,
            task: None,
            stop,
            local_router: Some(router),
        };
        server.set_build_debug(debug);
        server.set_web_root(web_root);
        server.port = server.start_public(http, https, cert, key).await?.0;
        Ok(server)
    }
    #[cfg(all(test, feature = "http_transport"))]
    pub(crate) fn runtime_state(&self) -> ServerState {
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
            .route("/status", get(crate::services::peers::peer_status::public))
            .with_state(crate::services::peers::peer_status::PublicState {
                state: self.state.clone(),
                stop: receiver.clone(),
            });
        let nearby_router = Router::new()
            .route("/nearby", post(crate::http::routes::discovery::call))
            .with_state(self.state.clone());
        let main_graphql_router = Router::new()
            .route("/graphql", post(crate::http::routes::graphql::call))
            .route("/health", get(crate::http::routes::graphql::health))
            .route("/proxyfs", get(crate::http::proxy::host::call))
            .route("/zip/dir", get(crate::http::routes::zip::dir))
            .route("/zip/files", get(crate::http::routes::zip::files))
            .route(
                "/upload",
                post(crate::graphql::resolvers::uploads::public::upload)
                    .layer(DefaultBodyLimit::max(15 * 60 * 1000 * 1000)),
            )
            .route(
                "/upload_chunk",
                post(crate::graphql::resolvers::uploads::public::upload_chunk)
                    .layer(DefaultBodyLimit::max(15 * 60 * 1000 * 1000)),
            )
            .route("/", get(crate::http::websocket::host::upgrade))
            .route(
                "/media/:id",
                get(crate::services::casting::cast_runtime::media),
            )
            .route(
                "/callback/cast",
                axum::routing::any(crate::services::casting::cast_runtime::callback),
            )
            .route("/init", post(crate::http::routes::graphql::init))
            .route("/shutdown", get(crate::http::routes::graphql::shutdown))
            .layer(DefaultBodyLimit::max(32 * 1024 * 1024))
            .with_state(self.state.clone());
        let router = Router::new()
            .fallback(crate::http::routes::static_host::fallback)
            .layer(DefaultBodyLimit::max(64 * 1024 * 1024 * 1024))
            .with_state(self.state.clone())
            .merge(peer_router)
            .merge(status_router)
            .merge(nearby_router)
            .merge(main_graphql_router)
            .merge(crate::graphql::resolvers::casting::control::router(
                self.state.clone(),
            ))
            .layer(axum::middleware::from_fn_with_state(
                self.state.clone(),
                crate::http::routes::cors_host::apply,
            ));
        let router = if let Some(local) = &self.local_router {
            unified_router(self.state.clone(), local.clone(), router)
        } else {
            router
        };
        let listeners =
            crate::http::listeners::HttpListeners::start(router, http, https, cert, key).await?;
        let ports = (listeners.http_port, listeners.https_port);
        {
            let _control = self.state.peer_status.control.lock().await;
            self.state
                .peer_status
                .public_active
                .store(true, std::sync::atomic::Ordering::SeqCst);
            self.state.peer_status.outgoing.start(self.state.clone());
        }
        let generation = crate::assembly::lifecycle::next_generation();
        let failure = listeners.failures();
        *guard = Some(PublicServer {
            listeners,
            stop,
            generation,
        });
        crate::assembly::lifecycle::monitor(self.state.clone(), failure, receiver, generation);
        Ok(ports)
    }
    #[cfg(feature = "http_transport")]
    pub async fn stop_public(&self) {
        let mut guard = self.state.public_server.lock().await;
        crate::assembly::lifecycle::stop_locked(&self.state, &mut guard).await;
    }
    #[cfg(feature = "http_transport")]
    pub async fn http_ports(&self) -> Option<(u16, u16)> {
        self.state
            .public_server
            .lock()
            .await
            .as_ref()
            .map(|server| (server.listeners.http_port, server.listeners.https_port))
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
        #[cfg(feature = "mobile")]
        self.state.mobile.mms.shutdown().await;
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
        if let Some(task) = &mut self.task {
            let _ = task.await;
        }
    }
}
impl Drop for ContentServer {
    fn drop(&mut self) {
        let _ = self.stop.send(true);
        if let Some(task) = &self.task {
            task.abort();
        }
        #[cfg(feature = "http_transport")]
        if let Ok(mut guard) = self.state.public_server.try_lock() {
            guard.take();
        }
    }
}
fn peer_routes(state: ServerState) -> Router {
    Router::new()
        .route(
            "/peer_graphql",
            post(crate::graphql::schemas::peer_host::public),
        )
        .route(
            "/guest_graphql",
            post(crate::http::auth::guest_graphql::public),
        )
        .route("/fs", get(crate::services::peers::peer_files::file))
        .layer(DefaultBodyLimit::max(32 * 1024 * 1024))
        .with_state(state)
}
pub(crate) fn peer_router(state: ServerState) -> Router {
    peer_routes(state)
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
    let response = crate::graphql::executor::execute_graphql(
        &s.schema,
        request,
        crate::graphql::executor::RequestContext::local(s.clone()),
    )
    .await;
    Json(response).into_response()
}
#[cfg(test)]
#[path = "../../tests/unit/assembly/server.rs"]
mod tests;

#[cfg(feature = "http_transport")]
pub(crate) struct PublicServer {
    pub(crate) listeners: crate::http::listeners::HttpListeners,
    pub(crate) stop: watch::Sender<bool>,
    pub(crate) generation: u64,
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

#[cfg(feature = "http_transport")]
fn unified_router(state: ServerState, local: Router, remote: Router) -> Router {
    Router::new().fallback(move |request: axum::extract::Request| {
        use tower::ServiceExt;
        let local = local.clone();
        let remote = remote.clone();
        let state = state.clone();
        async move {
            if state.authenticated(request.headers()) {
                let loopback = request
                    .extensions()
                    .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
                    .is_some_and(|peer| peer.0.ip().is_loopback());
                if !loopback {
                    return StatusCode::FORBIDDEN.into_response();
                }
                local.oneshot(request).await.unwrap()
            } else {
                remote.oneshot(request).await.unwrap()
            }
        }
    })
}

#[cfg(all(test, feature = "http_transport"))]
#[path = "../../tests/unit/assembly/server.rs"]
mod unified_http_tests;
