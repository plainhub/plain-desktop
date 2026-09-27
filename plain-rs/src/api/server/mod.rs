//! axum router for the desktop app's embedded local API stack.
//!
//! The HTTP/WS surface used to be a hand-rolled HTTP/1.1 + WebSocket + TLS
//! server; it is now an axum `Router` (the same framework plain-nas runs)
//! so both servers share one HTTP implementation and the wire behavior
//! comes from hyper. The listener loops and port rebinding stay in the
//! host (`plain-desktop`'s `local/server/mod.rs`): HTTP via
//! `axum::serve`, HTTPS via `axum_server::from_tcp_rustls`.

pub mod file_server;
pub mod handlers;
pub mod proxy_file;
pub mod response;
pub mod upload;
pub mod uri;
pub mod ws;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::middleware;
use axum::routing::{get, post};
use std::sync::Arc;

use super::peer_graphql::PeerSchema;
use crate::api::context::AppCtx;
use crate::api::schema::LocalSchema;

/// Everything the axum handlers need: the local GraphQL schema, the peer
/// schema, and the resolver context. Cheap to clone (all `Arc`s).
#[derive(Clone)]
pub struct ServerState {
    pub schema: Arc<LocalSchema>,
    pub peer_schema: Arc<PeerSchema>,
    pub ctx: Arc<AppCtx>,
}

/// Build the local API router. The host serves it twice (HTTP + HTTPS)
/// with `into_make_service_with_connect_info::<SocketAddr>()` — handlers
/// need the peer address for `/nearby` logging.
pub fn build_router(state: ServerState) -> Router {
    Router::new()
        .route("/health", get(handlers::health))
        .route("/init", post(handlers::init))
        .route("/fs", get(file_server::fs_handler))
        .route("/proxyfs", get(proxy_file::proxyfs_handler))
        .route("/graphql", post(handlers::graphql))
        .route("/peer_graphql", post(handlers::peer_graphql_handler))
        .route("/nearby", post(handlers::nearby))
        .route("/upload", post(upload::upload_handler))
        .route("/upload_chunk", post(upload::upload_chunk_handler))
        // DLNA receiver routes (custom GENA methods + GET /description.xml),
        // WebSocket upgrades on any path, and the 404 — same dispatch order
        // the hand-rolled server used.
        .fallback(handlers::fallback)
        // Any OPTIONS request answers 200, matching the old CORS preflight
        // shortcut that ran before the route table.
        .layer(middleware::from_fn(handlers::options_preflight))
        // One body cap for every route (16 MB, the old upload safety cap) —
        // the hand-rolled server enforced no general limit, but no legal
        // request exceeds this; it also lifts axum's 2 MB default off the
        // multipart upload fields (~5 MB chunks).
        .layer(DefaultBodyLimit::max(upload::MAX_BODY_BYTES))
        .with_state(state)
}
