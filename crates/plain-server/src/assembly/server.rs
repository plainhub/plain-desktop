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
    pub(crate) http_failure:
        Arc<std::sync::Mutex<Option<crate::assembly::lifecycle::HttpServerFailure>>>,
    #[cfg(feature = "http_transport")]
    pub(crate) build_debug: Arc<std::sync::atomic::AtomicBool>,
    #[cfg(feature = "http_transport")]
    pub(crate) web_root: Arc<std::sync::RwLock<Option<PathBuf>>>,
    #[cfg(feature = "http_transport")]
    pub(crate) tls_identity: Arc<std::sync::RwLock<crate::http::tls_identity::IdentitySource>>,
    pub(crate) schema: ContentSchema,
    /// The contract schema the public `/graphql` executes. Built once next
    /// to the app's own schema so both share one host and one database.
    pub(crate) public: crate::graphql::schemas::public::PublicSchema,
    pub(crate) peer_status: Arc<crate::services::peers::peer_status::Runtime>,
    pub(crate) mdns: Arc<crate::services::discovery::mdns_runtime::Runtime>,
    pub(crate) guest_replay: Arc<crate::http::auth::replay::Replay>,
    pub(crate) main_replay: Arc<crate::http::auth::replay::Replay>,
    pub(crate) peer_schema: crate::graphql::schemas::peer_host::PeerSchema,
    pub(crate) archives: Arc<crate::services::files::archives::Archives>,
    pub(crate) upload_chunks: Arc<crate::services::files::upload_chunks::UploadChunks>,
    pub(crate) files: Arc<crate::services::files::file_tasks::FileTasks>,
    #[cfg(feature = "media")]
    pub(crate) media_scans: Arc<crate::services::files::media_scans::MediaScans>,
    /// The live media watcher. Held so `stop_core` can release it; the
    /// catalog replaces it rather than stacking a second watch.
    #[cfg(feature = "media")]
    pub(crate) media_watch: std::sync::Arc<
        std::sync::Mutex<Option<crate::services::files::media_watch::WatcherHandle>>,
    >,
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
    pub(crate) image_index: Arc<crate::services::images::index::ImageIndex>,
    #[cfg(feature = "mobile")]
    pub(crate) mobile: Arc<crate::mobile::Runtime>,
    pub(crate) cast: Arc<crate::services::casting::dlna_sender_runtime::Runtime>,
    pub(crate) audio: Arc<crate::services::audio::host::Audio>,
    pub(crate) main_ws: Arc<crate::http::websocket::host::Runtime>,
    pub(crate) login_attempts: Arc<std::sync::Mutex<crate::http::auth::ws_login::LoginAttempts>>,
    pub(crate) directory: std::path::PathBuf,
    pub(crate) token: Arc<str>,
    pub(crate) temporary_values: Arc<std::sync::Mutex<std::collections::HashMap<String, String>>>,
    pub(crate) events: broadcast::Sender<WsEvent>,
    pub(crate) shutdown: Arc<tokio::sync::Mutex<()>>,
    pub(crate) stop: watch::Receiver<bool>,
    pub(crate) bridge: Arc<crate::host::resources::NativeResources>,
}
impl ServerState {
    pub(crate) fn native_client_id(&self) -> Result<String, String> {
        if *self.stop.borrow() || self.stop.has_changed().is_err() {
            return Err("Rust core stopped".into());
        }
        self.prefs
            .get::<String>(plain_rs::prefs::system::keys::CLIENT_ID)
            .map_err(|error| error.to_string())?
            .filter(|id| !id.is_empty())
            .ok_or_else(|| "native client identity unavailable".into())
    }

    pub(crate) async fn public_ports(&self) -> Option<(u16, u16)> {
        #[cfg(feature = "http_transport")]
        {
            self.public_server
                .lock()
                .await
                .as_ref()
                .map(|server| (server.listeners.http_port, server.listeners.https_port))
        }
        #[cfg(not(feature = "http_transport"))]
        {
            None
        }
    }

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
    pub(crate) fn set_temporary_value(&self, key: String, value: String) -> Result<(), String> {
        self.state.native_client_id()?;
        self.state
            .temporary_values
            .lock()
            .map_err(|_| "Temporary values unavailable")?
            .insert(key, value);
        Ok(())
    }

    #[cfg(feature = "headless")]
    pub(crate) fn headless_state(&self) -> &ServerState {
        &self.state
    }

    pub fn web_login_confirmation(&self) -> crate::http::auth::ws_login::Confirmation {
        crate::http::auth::ws_login::Confirmation::new(self.state.clone())
    }

    pub fn dlna_receiver(&self) -> crate::services::casting::receiver::control::Controller {
        crate::services::casting::receiver::control::Controller::new(self.state.clone())
    }

    pub fn shared_preferences(&self) -> Result<Arc<plain_rs::prefs::Prefs>, String> {
        self.state.native_client_id()?;
        Ok(self.state.prefs.clone())
    }

    pub fn preferences(&self) -> Result<&plain_rs::prefs::Prefs, String> {
        self.state.native_client_id()?;
        Ok(&self.state.prefs)
    }

    pub fn dlna_engine(
        &self,
    ) -> std::sync::Arc<crate::services::casting::receiver::receiver_engine::DlnaEngine> {
        self.state.dlna.clone()
    }

    pub fn peer_status(&self) -> std::sync::Arc<crate::services::peers::peer_status::Runtime> {
        self.state.peer_status.clone()
    }

    pub fn peer_status_controller(
        &self,
    ) -> crate::services::peers::peer_status_controller::Controller {
        crate::services::peers::peer_status_controller::Controller::new(self.state.clone())
    }

    pub fn native_graphql(&self) -> Result<crate::native::graphql::NativeGraphqlExecutor, String> {
        let client_id = self.state.native_client_id()?;
        Ok(crate::native::graphql::NativeGraphqlExecutor::new(
            self.graphql_executor(),
            self.state.events.clone(),
            client_id,
            self.state.stop.clone(),
        ))
    }

    pub fn native_file_tasks(&self) -> Result<crate::native::file_tasks::NativeFileTasks, String> {
        Ok(crate::native::file_tasks::NativeFileTasks::new(
            self.state.files.clone(),
            self.state.host.clone(),
            self.state.native_client_id()?,
            self.state.stop.clone(),
        ))
    }

    pub fn native_uploads(&self) -> Result<crate::native::uploads::NativeUploads, String> {
        self.state.native_client_id()?;
        Ok(crate::native::uploads::NativeUploads::new(
            self.state.upload_chunks.clone(),
            self.state.stop.clone(),
        ))
    }

    pub fn native_archives(&self) -> Result<crate::native::archives::NativeArchives, String> {
        self.state.native_client_id()?;
        Ok(crate::native::archives::NativeArchives::new(
            self.state.archives.clone(),
            self.state.host.clone(),
            self.state.stop.clone(),
        ))
    }

    pub fn graphql_executor(&self) -> crate::graphql::executor::GraphqlExecutor {
        crate::graphql::executor::GraphqlExecutor::new(self.state.clone())
    }
    pub async fn resolve_native_file(&self, reference: &str) -> Result<PathBuf, String> {
        crate::services::files::file_access::resolve_native(&self.state, reference).await
    }

    pub async fn read_resource(
        &self,
        path: &str,
    ) -> Result<crate::host::resources::NativeResourceReader, String> {
        let path = crate::host::filesystem::resolve(&self.state, path)
            .await
            .map_err(|error| error.to_string())?;
        self.state
            .bridge
            .read_native(&path, self.state.stop.clone())
            .await
    }

    pub fn native_dlna(&self) -> Result<crate::native::dlna::NativeDlnaReceiver, String> {
        self.state.native_client_id()?;
        Ok(crate::native::dlna::NativeDlnaReceiver::new(
            self.state.clone(),
        ))
    }

    pub fn native_peers(&self) -> Result<crate::native::peers::NativePeers, String> {
        self.state.native_client_id()?;
        Ok(crate::native::peers::NativePeers::new(self.state.clone()))
    }

    pub fn native_pairing(&self) -> Result<crate::native::pairing::NativePairing, String> {
        self.state.native_client_id()?;
        Ok(crate::native::pairing::NativePairing::new(
            self.state.clone(),
        ))
    }

    pub fn native_discovery(&self) -> Result<crate::native::discovery::NativeDiscovery, String> {
        self.state.native_client_id()?;
        Ok(crate::native::discovery::NativeDiscovery::new(
            self.state.clone(),
        ))
    }

    pub fn native_media_catalog(
        &self,
    ) -> Result<crate::native::media_catalog::NativeMediaCatalog, String> {
        self.state.native_client_id()?;
        Ok(crate::native::media_catalog::NativeMediaCatalog::new(
            self.state.clone(),
        ))
    }

    pub fn data_directory(&self) -> &Path {
        &self.state.directory
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

    fn build_state(
        path: &Path,
        token: &str,
        prefs: Arc<plain_rs::prefs::Prefs>,
    ) -> Result<(ServerState, watch::Sender<bool>, watch::Receiver<bool>), String> {
        crate::assembly::core_dependencies::CoreDependencies::validate_token(token)?;
        let db = Arc::new(Db::open(path).map_err(|e| e.to_string())?);
        let (events, _) = broadcast::channel(256);
        Self::build_state_with(crate::assembly::core_dependencies::CoreDependencies::new(
            db,
            prefs,
            path.parent().unwrap_or(Path::new(".")).to_path_buf(),
            token.to_owned(),
            events,
            Arc::new(crate::host::bridge::Host::default()),
        ))
    }

    fn build_state_with(
        dependencies: crate::assembly::core_dependencies::CoreDependencies,
    ) -> Result<(ServerState, watch::Sender<bool>, watch::Receiver<bool>), String> {
        let crate::assembly::core_dependencies::CoreDependencies {
            db,
            prefs,
            directory,
            token,
            events,
            host,
        } = dependencies;
        crate::assembly::core_dependencies::CoreDependencies::validate_token(&token)?;
        plain_rs::prefs::system::bootstrap_runtime(&prefs).map_err(|error| error.to_string())?;
        log::set_max_level(
            plain_rs::prefs::system::log_level(&prefs).map_err(|error| error.to_string())?,
        );
        plain_rs::prefs::identity::ensure_identity(&prefs);
        // The mDNS runtime reads the hostname rather than deriving one, so a
        // core that never seeds it cannot answer a pairing or advertisement
        // request. The legacy `AppCtx::assemble` already did this; the core
        // needs the same identity floor.
        plain_rs::prefs::ensure_mdns_hostname(&prefs);
        let (stop, receiver) = watch::channel(false);
        let lan =
            Arc::new(crate::services::peers::peer_lan::Lan::new().map_err(|e| e.to_string())?);
        let shared = crate::assembly::services::Services::shared(
            db.clone(),
            host.clone(),
            events.clone(),
            prefs.clone(),
            directory.clone(),
        );
        let services = crate::assembly::services::Services::with_shared(
            db.clone(),
            host.clone(),
            events.clone(),
            prefs.clone(),
            directory.clone(),
            shared,
        );
        let schema = schema::build_with_services(
            db.clone(),
            events.clone(),
            prefs.clone(),
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
            services.clone(),
            image_models.clone(),
            #[cfg(feature = "mobile")]
            mobile.clone(),
        );
        let bridge = Arc::new(crate::host::resources::NativeResources::new(host.clone()));
        let attachments =
            Arc::new(plain_rs::chat::attachments::attachment_imports::Imports::default());
        let downloads = Arc::new(plain_rs::chat::attachments::download_queue::Queue::new(
            (*db).clone(),
            directory.clone(),
            attachments.clone(),
        ));
        let preview_events = events.clone();
        let previews = Arc::new(crate::services::link_preview::Schedule::new(
            (*db).clone(),
            directory.clone(),
            Arc::new(move |row| {
                let _ = preview_events.send(WsEvent::broadcast(
                    plain_rs::chat::events::WS_MESSAGE_UPDATED,
                    serde_json::json!([plain_rs::chat::service::chat_to_json(&row)]).to_string(),
                ));
            }),
            receiver.clone(),
        ));
        let dlna = Arc::new(crate::services::casting::receiver::receiver_engine::DlnaEngine::new());
        let state = ServerState {
            #[cfg(feature = "media")]
            media_scans: Arc::new(crate::services::files::media_scans::MediaScans::new(
                events.clone(),
            )),
            #[cfg(feature = "media")]
            media_watch: std::sync::Arc::new(std::sync::Mutex::new(None)),
            #[cfg(feature = "http_transport")]
            public_server: Arc::new(tokio::sync::Mutex::new(None)),
            #[cfg(feature = "http_transport")]
            http_failure: Arc::new(std::sync::Mutex::new(None)),
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
            dlna,
            archives: services.archives,
            upload_chunks: services.upload_chunks,
            files: services.files,
            image_index: services.index.clone(),
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
            directory: directory.clone(),
            #[cfg(feature = "http_transport")]
            build_debug: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            #[cfg(feature = "http_transport")]
            web_root: Arc::new(std::sync::RwLock::new(None)),
            #[cfg(feature = "http_transport")]
            tls_identity: Arc::new(std::sync::RwLock::new(
                crate::http::tls_identity::IdentitySource::Json(
                    directory.join("tls-identity.json"),
                ),
            )),
            token: Arc::from(token.as_str()),
            temporary_values: Default::default(),
            events,
            shutdown: Arc::new(tokio::sync::Mutex::new(())),
            stop: receiver.clone(),
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
        crate::services::casting::receiver::control::follow_changes(&state);
        let mdns = state.mdns.clone();
        let mdns_host = state.host.clone();
        let mut mdns_stop = receiver.clone();
        tokio::spawn(async move {
            if !*mdns_stop.borrow() {
                let _ = mdns_stop.changed().await;
            }
            mdns.close(&mdns_host).await;
        });
        Ok((state, stop, receiver))
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
        let (state, stop, receiver) = Self::build_state(path, token, prefs)?;
        let router = Router::new()
            .route("/graphql", post(graphql))
            .route("/health", get(health))
            .route("/fs", get(crate::http::routes::files::file))
            .route(
                "/chat/mdns",
                post(crate::services::discovery::mdns_runtime::call),
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
                "/chat/download",
                post(crate::services::chat::download_queue::call),
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
    pub fn open(directory: &Path) -> Result<Self, String> {
        tokio::runtime::Handle::try_current().map_err(|error| error.to_string())?;
        if !directory.is_absolute() {
            return Err("absolute core data directory required".into());
        }
        std::fs::create_dir_all(directory).map_err(|error| error.to_string())?;
        let prefs = Arc::new(
            plain_rs::prefs::Prefs::load(&plain_rs::prefs::default_path(directory))
                .map_err(|error| error.to_string())?,
        );
        let url_token = plain_rs::prefs::identity::ensure_url_token(&prefs);
        crate::assembly::core_dependencies::CoreDependencies::validate_token(&url_token)?;
        if prefs
            .get::<String>(plain_rs::prefs::system::keys::URL_TOKEN)
            .map_err(|error| error.to_string())?
            .as_deref()
            != Some(url_token.as_str())
        {
            return Err("URL token persistence failed".into());
        }
        let token = plain_rs::crypto::gen_token();
        Self::start_core(&directory.join("plain.db"), &token, prefs)
    }

    pub fn start_core(
        path: &Path,
        token: &str,
        prefs: Arc<plain_rs::prefs::Prefs>,
    ) -> Result<Self, String> {
        let (state, stop, _) = Self::build_state(path, token, prefs)?;
        Ok(Self {
            port: 0,
            state,
            task: None,
            stop,
            #[cfg(feature = "http_transport")]
            local_router: None,
        })
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
    #[cfg(test)]
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
        let _core = self.state.shutdown.lock().await;
        self.start_public_locked(http, https, cert, key).await
    }

    #[cfg(feature = "http_transport")]
    async fn start_public_locked(
        &self,
        http: u16,
        https: u16,
        cert: Vec<u8>,
        key: Vec<u8>,
    ) -> Result<(u16, u16), String> {
        self.state.native_client_id()?;
        let mut guard = self.state.public_server.lock().await;
        if guard.is_some() {
            return Err("Public HTTP server is already running".into());
        }
        let (stop, receiver) = watch::channel(false);
        let router =
            crate::http::routes::public::build_router(self.state.clone(), receiver.clone());
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
        *self.state.http_failure.lock().unwrap() = None;
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
    pub fn native_url_token(&self) -> Result<String, String> {
        self.state.native_client_id()?;
        self.state
            .prefs
            .get::<String>(plain_rs::prefs::system::keys::URL_TOKEN)
            .map_err(|error| error.to_string())?
            .filter(|token| !token.is_empty())
            .ok_or_else(|| "native URL token unavailable".into())
    }

    pub async fn set_public_ports(
        &self,
        http: Option<u16>,
        https: Option<u16>,
    ) -> Result<bool, String> {
        self.state.native_client_id()?;
        crate::graphql::resolvers::settings::state::apply_user_patch(
            crate::graphql::resolvers::settings::state::UserPatchEnv::from_server(&self.state),
            plain_rs::prefs::user::UserSettingsPatch {
                http_port: http.map(i32::from),
                https_port: https.map(i32::from),
                ..Default::default()
            },
        )
        .await
        .map_err(|error| error.message)
    }

    #[cfg(feature = "http_transport")]
    async fn configured_public_identity(&self) -> Result<(u16, u16, Vec<u8>, Vec<u8>), String> {
        let settings = plain_rs::prefs::user::snapshot(&self.state.prefs)
            .map_err(|error| error.to_string())?;
        let http = u16::try_from(settings.http_port).map_err(|error| error.to_string())?;
        let https = u16::try_from(settings.https_port).map_err(|error| error.to_string())?;
        let identity = self
            .state
            .tls_identity
            .read()
            .map_err(|_| "TLS identity configuration unavailable")?
            .clone();
        let (cert, key) = tokio::task::spawn_blocking(move || identity.identity())
            .await
            .map_err(|error| error.to_string())??;
        Ok((http, https, cert, key))
    }

    #[cfg(feature = "http_transport")]
    pub async fn start_configured_public(&self) -> Result<(u16, u16), String> {
        let _core = self.state.shutdown.lock().await;
        self.state.native_client_id()?;
        let (http, https, cert, key) = self.configured_public_identity().await?;
        self.start_public_locked(http, https, cert, key).await
    }

    #[cfg(feature = "http_transport")]
    pub async fn restart_public(&self) -> Result<(u16, u16), String> {
        let _core = self.state.shutdown.lock().await;
        self.state.native_client_id()?;
        let (http, https, cert, key) = self.configured_public_identity().await?;
        let receiver_running = self.state.dlna.is_running();
        {
            let mut guard = self.state.public_server.lock().await;
            crate::assembly::lifecycle::stop_locked(&self.state, &mut guard).await;
        }
        let ports = self.start_public_locked(http, https, cert, key).await?;
        if receiver_running {
            crate::services::casting::receiver::control::execute(
                &self.state,
                crate::services::casting::receiver::control::Request::Start { port: ports.0 },
            )
            .await?;
        }
        self.state
            .mdns
            .update_hostname(&self.state)
            .await
            .map_err(|error| error.to_string())?;
        Ok(ports)
    }

    #[cfg(feature = "http_transport")]
    pub async fn stop_public(&self) {
        let _core = self.state.shutdown.lock().await;
        self.stop_public_locked().await;
    }

    #[cfg(feature = "http_transport")]
    async fn stop_public_locked(&self) {
        let mut guard = self.state.public_server.lock().await;
        crate::assembly::lifecycle::stop_locked(&self.state, &mut guard).await;
    }
    #[cfg(feature = "http_transport")]
    pub async fn http_ports(&self) -> Option<(u16, u16)> {
        self.state.public_ports().await
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
    pub async fn stop_core(&self) {
        let _shutdown = self.state.shutdown.lock().await;
        if *self.state.stop.borrow() {
            return;
        }
        #[cfg(feature = "http_transport")]
        self.stop_public_locked().await;
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
        #[cfg(feature = "media")]
        {
            // The watcher writes into the catalog, so it has to stop before
            // the scan state it feeds is closed.
            if let Ok(mut slot) = self.state.media_watch.lock() {
                slot.take();
            }
            if let Err(error) = self.state.media_scans.close() {
                log::error!("Media scan cancellation failed: {error}");
            }
        }
        let _ = self.stop.send(true);
        #[cfg(feature = "media")]
        if let Err(error) = self.state.media_scans.wait_background().await {
            log::error!("Media scan task shutdown failed: {error}");
        }
        #[cfg(feature = "http_transport")]
        self.state.mdns.close(&self.state.host).await;
    }

    pub async fn shutdown(mut self) {
        self.stop_core().await;
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
