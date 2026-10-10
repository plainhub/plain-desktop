use crate::adapters::mobile::graphql::schema::{self, ContentSchema};
use async_graphql::Request;
use axum::{
    Json, Router,
    extract::{
        DefaultBodyLimit, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
};
use plain_rs::{db::Db, ws_event::WsEvent};
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
pub(in crate::adapters::mobile) struct ServerState {
    #[cfg(feature = "http_transport")]
    pub(in crate::adapters::mobile) public_server: Arc<tokio::sync::Mutex<Option<PublicServer>>>,
    #[cfg(feature = "http_transport")]
    pub(in crate::adapters::mobile) build_debug: Arc<std::sync::atomic::AtomicBool>,
    #[cfg(feature = "http_transport")]
    pub(in crate::adapters::mobile) web_root: Arc<std::sync::RwLock<Option<PathBuf>>>,
    schema: ContentSchema,
    /// The contract schema the public `/graphql` executes. Built once next
    /// to the app's own schema so both share one host and one database.
    pub(in crate::adapters::mobile) public:
        crate::adapters::mobile::graphql::public_schema::PublicSchema,
    pub(in crate::adapters::mobile) peer_status:
        Arc<crate::adapters::mobile::peers::peer_status::Runtime>,
    pub(in crate::adapters::mobile) mdns:
        Arc<crate::adapters::mobile::discovery::mdns_runtime::Runtime>,
    pub(in crate::adapters::mobile) guest_replay:
        Arc<crate::adapters::mobile::http::request_replay::Replay>,
    pub(in crate::adapters::mobile) main_replay:
        Arc<crate::adapters::mobile::http::request_replay::Replay>,
    pub(in crate::adapters::mobile) peer_schema:
        crate::adapters::mobile::peers::peer_graphql::PeerSchema,
    pub(in crate::adapters::mobile) files:
        Arc<crate::adapters::mobile::files::file_tasks::FileTasks>,
    pub(in crate::adapters::mobile) host: Arc<crate::adapters::mobile::runtime::host::Host>,
    pub(in crate::adapters::mobile) thumbnails:
        Arc<crate::adapters::mobile::media::thumbnails::Thumbnails>,
    pub(in crate::adapters::mobile) db: Arc<Db>,
    pub(in crate::adapters::mobile) lan: Arc<crate::adapters::mobile::peers::peer_lan::Lan>,
    pub(in crate::adapters::mobile) transport: Arc<plain_rs::chat::transport_router::Router>,
    pub(in crate::adapters::mobile) previews: Arc<crate::services::link_preview::Schedule>,
    pub(in crate::adapters::mobile) prewarmer: Arc<plain_rs::chat::prewarm::Prewarmer>,
    pub(in crate::adapters::mobile) shared_batches:
        Arc<crate::adapters::mobile::peers::shared_batch::Runtime>,
    pub(in crate::adapters::mobile) download_runtime:
        Arc<crate::adapters::mobile::chat::download_queue::Runtime>,
    pub(in crate::adapters::mobile) downloads: Arc<plain_rs::chat::download_queue::Queue>,
    pub(in crate::adapters::mobile) attachments: Arc<plain_rs::chat::attachment_imports::Imports>,
    pub(in crate::adapters::mobile) delivery: Arc<plain_rs::chat::delivery::Delivery>,
    pub(in crate::adapters::mobile) channel_delivery: Arc<tokio::sync::Semaphore>,
    pub(in crate::adapters::mobile) ble_transport:
        Arc<crate::adapters::mobile::discovery::ble_http::Runtime>,
    pub(in crate::adapters::mobile) ble_pairing:
        Arc<crate::adapters::mobile::discovery::ble_pairing::Pairings>,
    pub(in crate::adapters::mobile) pairing_runtime:
        Arc<crate::adapters::mobile::discovery::pairing_runtime::Runtime>,
    pub(in crate::adapters::mobile) nearby_devices: Arc<plain_rs::chat::nearby_devices::Devices>,
    pub(in crate::adapters::mobile) ble_scans: Arc<plain_rs::chat::nearby_scan::Scans>,
    pub(in crate::adapters::mobile) pairing: Arc<plain_rs::chat::pairing::sessions::Sessions>,
    pub(in crate::adapters::mobile) prefs: Arc<plain_rs::prefs::Prefs>,
    pub(in crate::adapters::mobile) dlna:
        Arc<crate::services::dlna_receiver::receiver_engine::DlnaEngine>,
    pub(in crate::adapters::mobile) image_models:
        Arc<crate::adapters::mobile::media::image_models::Runtime>,
    pub(in crate::adapters::mobile) mms: Arc<crate::adapters::mobile::providers::mms_send::Runtime>,
    pub(in crate::adapters::mobile) cast:
        Arc<crate::adapters::mobile::casting::dlna_sender_runtime::Runtime>,
    pub(in crate::adapters::mobile) audio: Arc<crate::adapters::mobile::media::audio::Audio>,
    pub(in crate::adapters::mobile) main_ws: Arc<crate::adapters::mobile::http::main_ws::Runtime>,
    pub(in crate::adapters::mobile) login_attempts:
        Arc<std::sync::Mutex<crate::adapters::mobile::http::ws_login::LoginAttempts>>,
    pub(in crate::adapters::mobile) directory: std::path::PathBuf,
    pub(in crate::adapters::mobile) token: Arc<str>,
    pub(in crate::adapters::mobile) events: broadcast::Sender<WsEvent>,
    pub(in crate::adapters::mobile) stop: watch::Receiver<bool>,
    #[cfg(feature = "http_transport")]
    pub(in crate::adapters::mobile) bridge:
        Arc<crate::adapters::mobile::runtime::native_resources::NativeResources>,
}
impl ServerState {
    pub(in crate::adapters::mobile) fn authenticated(&self, headers: &HeaderMap) -> bool {
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
    task: Option<JoinHandle<()>>,
    #[cfg(feature = "http_transport")]
    local_router: Option<Router>,
    stop: watch::Sender<bool>,
}
impl ContentServer {
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
        let lan = Arc::new(
            crate::adapters::mobile::peers::peer_lan::Lan::new().map_err(|e| e.to_string())?,
        );
        let host = Arc::new(crate::adapters::mobile::runtime::host::Host::default());
        let services = crate::adapters::mobile::runtime::services::Services::new(
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
        let mms = crate::adapters::mobile::providers::mms_send::Runtime::new(
            host.clone(),
            prefs.clone(),
            db.clone(),
            directory.clone(),
            events.clone(),
        );
        let image_models = crate::adapters::mobile::media::image_models::Runtime::new(
            directory.clone(),
            host.clone(),
            prefs.clone(),
            services.index.clone(),
            events.clone(),
        );
        let public = crate::adapters::mobile::graphql::public_schema::build_with_runtime(
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
        let bridge = Arc::new(
            crate::adapters::mobile::runtime::native_resources::NativeResources::new(host.clone()),
        );
        let attachments = Arc::new(plain_rs::chat::attachment_imports::Imports::default());
        let downloads = Arc::new(plain_rs::chat::download_queue::Queue::new(
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
            thumbnails: Arc::new(crate::adapters::mobile::media::thumbnails::Thumbnails::default()),
            peer_status: Arc::new(crate::adapters::mobile::peers::peer_status::Runtime::default()),
            mdns: Arc::new(crate::adapters::mobile::discovery::mdns_runtime::Runtime::default()),
            peer_schema: crate::adapters::mobile::peers::peer_graphql::schema(),
            guest_replay: Arc::new(
                crate::adapters::mobile::http::request_replay::Replay::default(),
            ),
            main_replay: Arc::new(crate::adapters::mobile::http::request_replay::Replay::default()),
            transport: Arc::new(plain_rs::chat::transport_router::Router::default()),
            previews,
            prewarmer: Arc::new(plain_rs::chat::prewarm::Prewarmer::default()),
            shared_batches: Arc::new(
                crate::adapters::mobile::peers::shared_batch::Runtime::default(),
            ),
            download_runtime: Arc::new(
                crate::adapters::mobile::chat::download_queue::Runtime::default(),
            ),
            downloads,
            attachments,
            delivery: Arc::new(plain_rs::chat::delivery::Delivery::new((*db).clone())),
            channel_delivery: Arc::new(tokio::sync::Semaphore::new(4)),
            ble_transport: Arc::new(
                crate::adapters::mobile::discovery::ble_http::Runtime::default(),
            ),
            ble_pairing: Arc::new(
                crate::adapters::mobile::discovery::ble_pairing::Pairings::default(),
            ),
            pairing_runtime: Arc::new(
                crate::adapters::mobile::discovery::pairing_runtime::Runtime::default(),
            ),
            nearby_devices: Arc::new(plain_rs::chat::nearby_devices::Devices::default()),
            ble_scans: Arc::new(plain_rs::chat::nearby_scan::Scans::default()),
            pairing: Arc::new(plain_rs::chat::pairing::sessions::Sessions::default()),
            dlna: Arc::new(crate::services::dlna_receiver::receiver_engine::DlnaEngine::new()),
            files: services.files,
            audio: services.audio,
            host,
            db,
            prefs,
            image_models: image_models.clone(),
            mms,
            cast: Arc::new(
                crate::adapters::mobile::casting::dlna_sender_runtime::Runtime::default(),
            ),
            main_ws: Arc::new(crate::adapters::mobile::http::main_ws::Runtime::default()),
            login_attempts: Arc::new(std::sync::Mutex::new(
                crate::adapters::mobile::http::ws_login::LoginAttempts::default(),
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
        crate::adapters::mobile::chat::download_queue::start(state.clone());
        crate::adapters::mobile::peers::shared_batch::start(state.clone());
        crate::adapters::mobile::discovery::pairing_timeout::start(
            state.pairing.clone(),
            state.events.clone(),
            receiver.clone(),
        );
        crate::adapters::mobile::discovery::nearby_devices::start(state.clone());
        image_models.follow(receiver.clone());
        state.mms.follow(receiver.clone());
        crate::adapters::mobile::casting::public_dlna::follow_changes(&state);
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
            .route(
                "/events",
                get(crate::adapters::mobile::runtime::events::subscribe),
            )
            .route("/host", get(host_upgrade))
            .route("/health", get(health))
            .route("/fs", get(crate::adapters::mobile::files::routes::file))
            .route(
                "/chat/discovery",
                post(crate::adapters::mobile::discovery::discovery_advertisement::call),
            )
            .route(
                "/chat/peer-status",
                post(crate::adapters::mobile::peers::peer_status::call),
            )
            .route(
                "/chat/mdns",
                post(crate::adapters::mobile::discovery::mdns_runtime::call),
            )
            .route(
                "/chat/channel",
                post(crate::adapters::mobile::chat::channel_runtime::call),
            )
            .route(
                "/chat/pairing",
                post(crate::adapters::mobile::discovery::pairing_runtime::call),
            )
            .route(
                "/chat/peer-graphql",
                post(crate::adapters::mobile::peers::peer_graphql::call),
            )
            .route(
                "/chat/lan/file",
                post(crate::adapters::mobile::peers::peer_lan::file),
            )
            .route(
                "/shares/client",
                post(crate::adapters::mobile::peers::shared_client::call),
            )
            .route(
                "/shares/client/file",
                post(crate::adapters::mobile::peers::shared_download::file),
            )
            .route(
                "/shares/client/zip",
                post(crate::adapters::mobile::peers::shared_zip::call),
            )
            .route(
                "/shares/batch",
                post(crate::adapters::mobile::peers::shared_batch::call),
            )
            .route(
                "/chat/transport",
                post(crate::adapters::mobile::peers::peer_transport::call),
            )
            .route(
                "/chat/prewarm",
                post(crate::adapters::mobile::chat::prewarm::call),
            )
            .route(
                "/chat/nearby-devices",
                post(crate::adapters::mobile::discovery::nearby_devices::call),
            )
            .route(
                "/chat/nearby",
                post(crate::adapters::mobile::discovery::nearby_http::call),
            )
            .route(
                "/chat/link-preview",
                post(crate::adapters::mobile::chat::link_preview::call),
            )
            .route(
                "/chat/download",
                post(crate::adapters::mobile::chat::download_queue::call),
            )
            .route(
                "/chat/attachment",
                post(crate::adapters::mobile::chat::attachment_imports::call),
            )
            .route(
                "/chat/send",
                post(crate::adapters::mobile::chat::chat_delivery::call),
            )
            .route(
                "/chat/service",
                post(crate::adapters::mobile::chat::chat_service::call),
            )
            .route(
                "/files/thumbnail",
                post(crate::adapters::mobile::media::thumbnails::call),
            )
            .route(
                "/files/thumbnail/output/:token",
                post(crate::adapters::mobile::media::thumbnails::output),
            )
            .route(
                "/chat/ble-exchange/:token",
                get(crate::adapters::mobile::discovery::ble_http::outgoing),
            )
            .route(
                "/chat/ble-incoming",
                get(crate::adapters::mobile::discovery::ble_http::incoming),
            )
            .route(
                "/chat/store",
                post(crate::adapters::mobile::chat::chat_store_routes::call),
            )
            .route(
                "/files/write",
                post(crate::adapters::mobile::files::file_writes::write),
            )
            .route(
                "/files/read",
                post(crate::adapters::mobile::files::file_reads::read),
            )
            .route(
                "/files/stat",
                post(crate::adapters::mobile::files::file_reads::stat),
            )
            .route(
                "/files/mutate",
                post(crate::adapters::mobile::files::file_mutation_routes::mutate),
            )
            .route(
                "/system/sms-state",
                post(crate::adapters::mobile::providers::sms_state::call),
            )
            .route(
                "/system/sms",
                post(crate::adapters::mobile::providers::sms_query::call),
            )
            .route(
                "/system/sms-send",
                post(crate::adapters::mobile::providers::sms_send::call),
            )
            .route(
                "/system/contact-write",
                post(crate::adapters::mobile::providers::contact_write::call),
            )
            .route(
                "/system/ws-login",
                post(crate::adapters::mobile::http::ws_login::call),
            )
            .route(
                "/system/ws-runtime",
                post(crate::adapters::mobile::http::main_ws::call),
            )
            .route(
                "/system/dlna-sender",
                post(crate::adapters::mobile::casting::dlna_sender_runtime::call),
            )
            .route(
                "/system/mms-runtime",
                post(crate::adapters::mobile::providers::mms_send::call),
            )
            .route(
                "/system/sessions",
                post(crate::adapters::mobile::http::sessions::call),
            )
            .route(
                "/system/providers",
                post(crate::adapters::mobile::providers::system_providers::call),
            )
            .route(
                "/system/preferences",
                post(crate::adapters::mobile::settings::preferences::call),
            )
            .route(
                "/system/media-buckets",
                post(crate::adapters::mobile::media::media_buckets::call),
            )
            .route(
                "/system/permissions",
                post(crate::adapters::mobile::providers::system_permissions::call),
            )
            .route(
                "/system/provider-plan",
                post(crate::adapters::mobile::providers::provider_plan::call),
            )
            .route(
                "/system/provider-delete",
                post(crate::adapters::mobile::providers::provider_deletes::call),
            )
            .route(
                "/system/notification",
                post(crate::adapters::mobile::chat::notification_actions::call),
            )
            .route(
                "/dlna/receiver",
                post(crate::adapters::mobile::casting::public_dlna::call),
            )
            .route(
                "/system/image-models",
                post(crate::adapters::mobile::media::image_models::call),
            )
            .route(
                "/system/image-search",
                post(crate::adapters::mobile::media::image_search::call),
            )
            .route(
                "/http/guest",
                post(crate::adapters::mobile::graphql::guest_graphql::call),
            );
        #[cfg(feature = "http_transport")]
        let router = router
            .route(
                "/system/http-server/health",
                post(crate::adapters::mobile::runtime::public_lifecycle::health),
            )
            .route(
                "/system/tls",
                post(crate::adapters::mobile::http::tls_identity::call),
            )
            .route("/resources/:id", get(resource_upgrade))
            .route(
                "/system/notification-event",
                post(crate::adapters::mobile::providers::public_notifications::publish),
            );
        let router = router
            .layer(DefaultBodyLimit::max(32 * 1024 * 1024))
            .with_state(state.clone());
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
            #[cfg(feature = "http_transport")]
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
    pub(in crate::adapters::mobile) fn runtime_state(&self) -> ServerState {
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
            .route(
                "/status",
                get(crate::adapters::mobile::peers::peer_status::public),
            )
            .with_state(crate::adapters::mobile::peers::peer_status::PublicState {
                state: self.state.clone(),
                stop: receiver.clone(),
            });
        let nearby_router = Router::new()
            .route(
                "/nearby",
                post(crate::adapters::mobile::discovery::nearby_public::call),
            )
            .with_state(self.state.clone());
        let main_graphql_router = Router::new()
            .route(
                "/graphql",
                post(crate::adapters::mobile::graphql::main_graphql::call),
            )
            .route(
                "/health",
                get(crate::adapters::mobile::graphql::main_graphql::health),
            )
            .route(
                "/proxyfs",
                get(crate::adapters::mobile::http::public_proxy::call),
            )
            .route(
                "/zip/dir",
                get(crate::adapters::mobile::files::public_zip::dir),
            )
            .route(
                "/zip/files",
                get(crate::adapters::mobile::files::public_zip::files),
            )
            .route(
                "/upload",
                post(crate::adapters::mobile::files::public_upload::upload)
                    .layer(DefaultBodyLimit::max(15 * 60 * 1000 * 1000)),
            )
            .route(
                "/upload_chunk",
                post(crate::adapters::mobile::files::public_upload::upload_chunk)
                    .layer(DefaultBodyLimit::max(15 * 60 * 1000 * 1000)),
            )
            .route("/", get(crate::adapters::mobile::http::main_ws::upgrade))
            .route(
                "/media/:id",
                get(crate::adapters::mobile::casting::cast_runtime::media),
            )
            .route(
                "/callback/cast",
                axum::routing::any(crate::adapters::mobile::casting::cast_runtime::callback),
            )
            .route(
                "/init",
                post(crate::adapters::mobile::graphql::main_graphql::init),
            )
            .route(
                "/shutdown",
                get(crate::adapters::mobile::graphql::main_graphql::shutdown),
            )
            .layer(DefaultBodyLimit::max(32 * 1024 * 1024))
            .with_state(self.state.clone());
        let router = Router::new()
            .fallback(crate::adapters::mobile::http::public_static::fallback)
            .layer(DefaultBodyLimit::max(64 * 1024 * 1024 * 1024))
            .with_state(self.state.clone())
            .merge(peer_router)
            .merge(status_router)
            .merge(nearby_router)
            .merge(main_graphql_router)
            .merge(crate::adapters::mobile::casting::public_dlna::router(
                self.state.clone(),
            ))
            .layer(axum::middleware::from_fn_with_state(
                self.state.clone(),
                crate::adapters::mobile::http::public_cors::apply,
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
        let generation = crate::adapters::mobile::runtime::public_lifecycle::next_generation();
        let failure = listeners.failures();
        *guard = Some(PublicServer {
            listeners,
            stop,
            generation,
        });
        crate::adapters::mobile::runtime::public_lifecycle::monitor(
            self.state.clone(),
            failure,
            receiver,
            generation,
        );
        Ok(ports)
    }
    #[cfg(feature = "http_transport")]
    pub async fn stop_public(&self) {
        let mut guard = self.state.public_server.lock().await;
        crate::adapters::mobile::runtime::public_lifecycle::stop_locked(&self.state, &mut guard)
            .await;
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
            post(crate::adapters::mobile::peers::peer_graphql::public),
        )
        .route(
            "/guest_graphql",
            post(crate::adapters::mobile::graphql::guest_graphql::public),
        )
        .route("/fs", get(crate::adapters::mobile::peers::peer_files::file))
        .layer(DefaultBodyLimit::max(32 * 1024 * 1024))
        .with_state(state)
}
pub(in crate::adapters::mobile) fn peer_router(state: ServerState) -> Router {
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
        let _ = s
            .events
            .send(WsEvent::broadcast("CONTENT_CHANGED", "{}".into()));
    }
    Json(response).into_response()
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
    tokio::spawn(
        crate::adapters::mobile::discovery::mdns_runtime::Runtime::host_connected(
            state.clone(),
            generation,
        ),
    );
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
#[path = "../../../../tests/unit/adapters/mobile/runtime/server.rs"]
mod tests;

#[cfg(feature = "http_transport")]
pub(in crate::adapters::mobile) struct PublicServer {
    pub(in crate::adapters::mobile) listeners: crate::http::listeners::HttpListeners,
    pub(in crate::adapters::mobile) stop: watch::Sender<bool>,
    pub(in crate::adapters::mobile) generation: u64,
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
#[path = "../../../../tests/unit/adapters/mobile/runtime/unified_http.rs"]
mod unified_http_tests;
