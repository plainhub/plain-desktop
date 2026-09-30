pub mod auth;
pub mod cors;
pub mod events;
pub mod file_server;
pub mod handlers;
#[cfg(feature = "nas")]
pub mod media_alias;
pub mod proxy_file;
pub mod request_key;
pub mod response;
pub mod runtime;
#[cfg(feature = "nas")]
pub mod static_files;
pub mod upload;
pub mod uri;
pub mod ws;
pub mod zip;

use std::sync::Arc;

use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};
use axum::Router;

use super::context::AppCtx;
use crate::httpserver::mainschemas::ApiSchema;
use crate::httpserver::peerschemas::PeerSchema;

#[derive(Clone)]
pub struct ServerState {
    pub schema: Arc<ApiSchema>,
    pub peer_schema: Arc<PeerSchema>,
    pub ctx: Arc<AppCtx>,
    pub settings: Arc<ServerSettings>,
}

pub enum AuthPolicy {
    LocalToken,
    Session {
        dev_token: String,
        device_id: String,
    },
}

pub struct ServerSettings {
    pub auth: AuthPolicy,
    pub cors: cors::CorsPolicy,
    pub serve_spa: bool,
}

#[cfg(all(test, feature = "nas"))]
pub(crate) mod test_support {
    use super::*;

    pub(crate) fn as_desktop(state: &ServerState) -> ServerState {
        ServerState::new(
            state.schema.clone(),
            state.peer_schema.clone(),
            state.ctx.clone(),
            ServerSettings {
                auth: AuthPolicy::LocalToken,
                cors: cors::CorsPolicy::permissive_default(),
                serve_spa: false,
            },
        )
    }

    /// Build a nas `ServerState` over a fresh temp data dir. The temp
    /// dir is leaked on purpose — the fjall/SQLite handles must outlive
    /// the test.
    pub(crate) fn nas_state() -> ServerState {
        nas_state_with(|_prefs| {})
    }

    /// `nas_state` with a hook that can seed prefs (e.g. the url_token)
    /// before the chat stack and ctx assemble.
    pub(crate) fn nas_state_with(seed: impl FnOnce(&crate::prefs::Prefs)) -> ServerState {
        let dir = tempfile::tempdir().expect("temp dir");
        let data_dir = dir.path().to_path_buf();
        let prefs =
            Arc::new(crate::prefs::Prefs::load(&data_dir.join("prefs.json")).expect("prefs load"));
        seed(&prefs);
        let chat =
            Arc::new(crate::api::chat::ChatState::nas_init(&data_dir, &prefs).expect("chat init"));
        let config = Arc::new(crate::media::config::Config::parse(
            "[server]
http_port = 8080
",
        ));
        let (event_tx, _) = tokio::sync::broadcast::channel(64);
        let ctx = crate::api::context::AppCtx::assemble(
            data_dir.clone(),
            data_dir.join("cache"),
            data_dir.join("logs"),
            data_dir.join("library.db"),
            prefs.clone(),
            chat.clone(),
            event_tx,
            Arc::new(crate::api::context::LogShell {
                version: String::new(),
            }),
            8080,
            8443,
        )
        .expect("nas app ctx");
        std::mem::forget(dir);
        ServerState::new(
            Arc::new(crate::httpserver::mainschemas::build_schema()),
            Arc::new(crate::httpserver::peerschemas::build_schema()),
            ctx,
            ServerSettings {
                auth: AuthPolicy::Session {
                    dev_token: config.get_string("auth.dev_token"),
                    device_id: config.get_string("nas.id"),
                },
                cors: cors::CorsPolicy::default(),
                serve_spa: true,
            },
        )
    }
}

impl ServerState {
    pub fn new(
        schema: Arc<ApiSchema>,
        peer_schema: Arc<PeerSchema>,
        ctx: Arc<AppCtx>,
        settings: ServerSettings,
    ) -> Self {
        Self {
            schema,
            peer_schema,
            ctx,
            settings: Arc::new(settings),
        }
    }
}

/// Per-route upload body cap: multipart streaming must not hit axum's
/// 2 MB default (5 MB chunks, ≤200 MB direct uploads, larger on the
/// nas LAN paths).
pub const MAX_UPLOAD_BODY_BYTES: usize = 64 * 1024 * 1024 * 1024;

/// Build the local API router for either host.
pub fn build_router(state: ServerState) -> Router {
    let mut router = Router::new()
        .route("/health", get(handlers::health))
        .route("/health_check", get(handlers::health))
        .route("/init", post(handlers::init))
        .route("/graphql", post(handlers::graphql))
        .route("/peer_graphql", post(handlers::peer_graphql_handler))
        .route("/nearby", post(handlers::nearby))
        .route("/fs", get(file_server::fs_handler))
        .route(
            "/upload",
            post(upload::upload_handler).layer(DefaultBodyLimit::max(MAX_UPLOAD_BODY_BYTES)),
        )
        .route(
            "/upload_chunk",
            post(upload::upload_chunk_handler).layer(DefaultBodyLimit::max(MAX_UPLOAD_BODY_BYTES)),
        )
        .route("/zip/dir", get(zip::zip_dir_handler))
        .route("/zip/files", get(zip::zip_files_handler));

    #[cfg(feature = "nas")]
    {
        if state.settings.serve_spa {
            router = router
                .route("/auth", post(auth::auth_handler))
                .route("/auth/status", post(auth::auth_status_handler))
                .route("/auth/setup", post(auth::auth_setup_handler))
                .route("/media/:name", get(media_alias::media_handler))
                // `/` is the WebSocket upgrade + SPA index split
                // (plain-app contract); the handler delegates non-upgrade
                // GETs to the static index.
                .route("/", get(ws::root_handler))
                .route("/broken-image.png", get(static_files::broken_image))
                .route("/favicon.ico", get(static_files::favicon))
                .route("/logo.svg", get(static_files::logo))
                .route("/manifest.json", get(static_files::manifest))
                .route("/sw.js", get(static_files::sw))
                .route("/assets/*path", get(static_files::serve_assets))
                .route("/ficons/*path", get(static_files::serve_ficons))
                .route("/icons/*path", get(static_files::serve_icons))
                .fallback(static_files::spa_fallback)
                .layer(tower_http::compression::CompressionLayer::new())
                .layer(tower_http::trace::TraceLayer::new_for_http())
                .layer(cors::layer(&state.settings.cors));
            return router.with_state(state);
        }
    }

    router = router
        .route("/proxyfs", get(proxy_file::proxyfs_handler))
        // DLNA receiver routes (custom GENA methods + GET /description.xml),
        // WebSocket upgrades on any path, and the 404 — same dispatch order
        // the hand-rolled server used.
        .fallback(handlers::fallback)
        .layer(cors::layer(&state.settings.cors));
    router.with_state(state)
}
