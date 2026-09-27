//! Router construction. Mirrors `cmd/services/api/run.go`.

use super::auth::AppState;
use super::cors::layer as cors_layer;
use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{get, post},
};

pub fn build_router(state: AppState) -> Router {
    let api = Router::new()
        .route("/health_check", get(super::static_files::health))
        .route("/init", post(super::auth::init_handler))
        .route("/auth", post(super::auth::auth_handler))
        .route("/auth/status", post(super::auth::auth_status_handler))
        .route("/auth/setup", post(super::auth::auth_setup_handler))
        .route("/graphql", post(super::graphql::graphql_handler))
        .route(
            "/upload",
            post(super::upload::upload_handler)
                .layer(DefaultBodyLimit::max(64 * 1024 * 1024 * 1024)),
        )
        .route(
            "/upload_chunk",
            post(super::upload::upload_chunk_handler)
                .layer(DefaultBodyLimit::max(64 * 1024 * 1024 * 1024)),
        )
        .route("/media/:name", get(super::media_thumb::media_handler))
        .route("/fs", get(super::fs::fs_handler))
        // Chat peer endpoints — the pairing transport and the encrypted
        // peer GraphQL ingestion (both self-authenticating protocols).
        .route("/nearby", post(super::chat_peer::nearby_handler))
        .route(
            "/peer_graphql",
            post(super::chat_peer::peer_graphql_handler),
        )
        .route("/zip/dir", get(super::zip::zip_dir_handler))
        .route("/zip/files", get(super::zip::zip_files_handler));

    let web = Router::new()
        // `/` is the WebSocket upgrade + SPA index split (plain-app contract);
        // the handler delegates non-upgrade GETs to the static index.
        .route("/", get(super::ws::root_handler))
        .route("/broken-image.png", get(super::static_files::broken_image))
        .route("/favicon.ico", get(super::static_files::favicon))
        .route("/logo.svg", get(super::static_files::logo))
        .route("/manifest.json", get(super::static_files::manifest))
        .route("/sw.js", get(super::static_files::sw))
        .route("/assets/*path", get(super::static_files::serve_assets))
        .route("/ficons/*path", get(super::static_files::serve_ficons))
        .route("/icons/*path", get(super::static_files::serve_icons))
        .fallback(super::static_files::spa_fallback);

    Router::new()
        .merge(api)
        .merge(web)
        .layer(tower_http::compression::CompressionLayer::new())
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .layer(cors_layer(&state.cors))
        .with_state(state)
}
