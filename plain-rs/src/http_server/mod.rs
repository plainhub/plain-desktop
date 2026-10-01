pub mod events;
pub mod main_schemas;
pub mod models;
pub mod peer_schemas;
pub mod proxy;
pub mod routes;
pub mod runtime;
pub mod temp_store;
pub mod tls;
pub mod websocket;

pub use models::{AuthPolicy, ServerSettings, ServerState};
#[cfg(all(test, feature = "system"))]
pub(crate) use models::test_support;
pub use routes::cors::CorsPolicy;
use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};

use routes::{auth, cors, file_server, handlers, proxy_file, static_files, upload, zip};

pub const MAX_UPLOAD_BODY_BYTES: usize = 64 * 1024 * 1024 * 1024;

pub fn build_router(state: ServerState) -> Router {
    let mut router = Router::new()
        .route("/health", get(handlers::health))
        .route("/health_check", get(handlers::health))
        .route("/init", post(handlers::init))
        .route("/graphql", post(handlers::graphql))
        .route("/peer_graphql", post(handlers::peer_graphql_handler))
        .route("/nearby", post(handlers::nearby))
        .route("/fs", get(file_server::fs_handler))
        .route("/upload", post(upload::upload_handler).layer(DefaultBodyLimit::max(MAX_UPLOAD_BODY_BYTES)))
        .route("/upload_chunk", post(upload::upload_chunk_handler).layer(DefaultBodyLimit::max(MAX_UPLOAD_BODY_BYTES)))
        .route("/zip/dir", get(zip::zip_dir_handler))
        .route("/zip/files", get(zip::zip_files_handler));

    #[cfg(feature = "system")]
    if state.settings.serve_spa {
        router = router
            .route("/auth", post(auth::auth_handler))
            .route("/auth/status", post(auth::auth_status_handler))
            .route("/auth/setup", post(auth::auth_setup_handler))
            .route("/media/:name", get(routes::media_alias::media_handler))
            .route("/", get(websocket::root_handler))
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

    router.route("/proxyfs", get(proxy_file::proxyfs_handler))
        .fallback(handlers::fallback)
        .layer(cors::layer(&state.settings.cors))
        .with_state(state)
}
