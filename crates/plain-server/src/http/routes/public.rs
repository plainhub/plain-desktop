use crate::assembly::server::ServerState;
use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{get, post},
};
use tokio::sync::watch;

#[cfg(feature = "http_transport")]
pub(crate) fn build_router(state: ServerState, receiver: watch::Receiver<bool>) -> Router {
    let peer_router = peer_router(state.clone());
    let status_router = Router::new()
        .route("/status", get(crate::services::peers::peer_status::public))
        .with_state(crate::services::peers::peer_status::PublicState {
            state: state.clone(),
            stop: receiver.clone(),
        });
    let nearby_router = Router::new()
        .route("/nearby", post(crate::http::routes::discovery::call))
        .with_state(state.clone());
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
        .with_state(state.clone());
    Router::new()
        .fallback(crate::http::routes::static_host::fallback)
        .layer(DefaultBodyLimit::max(64 * 1024 * 1024 * 1024))
        .with_state(state.clone())
        .merge(peer_router)
        .merge(status_router)
        .merge(nearby_router)
        .merge(main_graphql_router)
        .merge(crate::graphql::resolvers::casting::control::router(
            state.clone(),
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::http::routes::cors_host::apply,
        ))
}

pub(crate) fn peer_router(state: ServerState) -> Router {
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
