//! The ONE axum router both hosts (desktop Tauri shell, plain-nas
//! server) serve: HTTP routes, WebSocket upgrades, and the CORS body
//! limit layers. The listener loops and TLS stay in the hosts
//! (`plain-desktop`'s `local/server/mod.rs`, plain-nas's `cmd/run.rs`):
//! HTTP via `axum::serve`, HTTPS via `axum_server::from_tcp_rustls`,
//! both with `into_make_service_with_connect_info::<SocketAddr>()` —
//! handlers need the peer address for `/nearby` logging.
//!
//! Host selection is a field on [`ServerState`]: the desktop stores its
//! `LocalSchema` behind the type-erased [`GraphqlExec`] and leaves
//! `nas` = `None`; the nas crate (until its GraphQL layer folds in)
//! stores its own schema shim plus its config/CORS in
//! [`NasServerState`]. The nas-only routes (auth, static SPA, `/media`
//! alias, `/` WS+SPA split) mount only when `nas` is `Some`.

pub mod cors;
pub mod events;
pub mod file_server;
pub mod handlers;
#[cfg(feature = "nas")]
pub mod nas_ctx;
pub mod proxy_file;
pub mod request_key;
pub mod response;
pub mod upload;
pub mod uri;
pub mod ws;
pub mod zip;
#[cfg(feature = "nas")]
pub mod auth;
#[cfg(feature = "nas")]
pub mod media_alias;
#[cfg(feature = "nas")]
pub mod static_files;

use std::sync::Arc;

use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};

use super::context::AppCtx;
use super::peer_graphql::PeerSchema;
use super::schema::LocalSchema;

/// Everything the axum handlers need: the GraphQL schema behind a
/// type-erased executor, the peer schema, and the resolver context.
/// Cheap to clone (all `Arc`s).
#[derive(Clone)]
pub struct ServerState {
    /// The host's GraphQL schema, type-erased so the desktop's
    /// `LocalSchema` and (this phase) the nas crate's own schema both
    /// fit. The desktop stores a plain `LocalSchema` here — its
    /// graphql branch downcasts back through [`GraphqlExec::as_any`].
    pub schema: Arc<dyn GraphqlExec>,
    pub peer_schema: Arc<PeerSchema>,
    pub ctx: Arc<AppCtx>,
    /// Nas host extras (config, CORS policy, this phase's nas-local
    /// peer schema). `None` on the desktop — the nas-only routes mount
    /// only when this is `Some`.
    #[cfg(feature = "nas")]
    pub nas: Option<Arc<NasServerState>>,
}

/// Nas-specific state carried next to [`ServerState`].
#[cfg(feature = "nas")]
pub struct NasServerState {
    pub config: Arc<crate::media::config::Config>,
    pub cors: cors::CorsPolicy,
    /// This phase's nas-local peer GraphQL schema for `/peer_graphql`
    /// (dies in phase 3 when the schemas unify).
    pub peer_schema: Arc<dyn PeerSchemaExec>,
}

/// Type-erased executor over the nas crate's peer GraphQL schema.
/// The nas crate implements this for its shim; phase 3 removes it when
/// `/peer_graphql` serves the plain-rs `PeerSchema` for both hosts.
#[cfg(feature = "nas")]
pub trait PeerSchemaExec: Send + Sync + 'static {
    fn execute(
        &self,
        request: async_graphql::Request,
        peer: crate::chat::db::DPeer,
        channel_id: &str,
        chat: Arc<crate::api::chat::ChatState>,
    ) -> futures_util::future::BoxFuture<'_, async_graphql::Response>;
}

/// Type-erased `async_graphql::Schema` executor: one field holds either
/// host's schema without naming the root types.
pub trait GraphqlExec: Send + Sync + 'static {
    fn execute(
        &self,
        request: async_graphql::Request,
        cid: &str,
    ) -> futures_util::future::BoxFuture<'_, async_graphql::Response>;

    /// Downcast seam for the desktop graphql branch, which runs its
    /// local executor (stub pre-filter + `AppCtx` data injection)
    /// against the concrete `LocalSchema`.
    fn as_any(&self) -> &dyn std::any::Any;
}

impl<Q, M, S> GraphqlExec for async_graphql::Schema<Q, M, S>
where
    Q: async_graphql::ObjectType + 'static,
    M: async_graphql::ObjectType + 'static,
    S: async_graphql::SubscriptionType + 'static,
{
    fn execute(
        &self,
        request: async_graphql::Request,
        _cid: &str,
    ) -> futures_util::future::BoxFuture<'_, async_graphql::Response> {
        let schema = self.clone();
        Box::pin(async move { schema.execute(request).await })
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(all(test, feature = "nas"))]
pub(crate) mod test_support {
    //! Shared fixture for the nas-flavored `ServerState` tests (auth,
    //! ws, fs, chat_peer): a real `AppCtx` over temp dirs plus stub
    //! schema executors (the nas gql schema lives in the nas crate and
    //! is exercised by its own tests).

    use super::*;

    struct StubExec;

    impl GraphqlExec for StubExec {
        fn execute(
            &self,
            _request: async_graphql::Request,
            _cid: &str,
        ) -> futures_util::future::BoxFuture<'_, async_graphql::Response> {
            Box::pin(async {
                async_graphql::Response::from_errors(vec![async_graphql::ServerError::new(
                    "stub schema",
                    None,
                )])
            })
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    struct StubPeerExec;

    impl PeerSchemaExec for StubPeerExec {
        fn execute(
            &self,
            _request: async_graphql::Request,
            _peer: crate::chat::db::DPeer,
            _channel_id: &str,
            _chat: Arc<crate::api::chat::ChatState>,
        ) -> futures_util::future::BoxFuture<'_, async_graphql::Response> {
            Box::pin(async {
                async_graphql::Response::from_errors(vec![async_graphql::ServerError::new(
                    "stub peer schema",
                    None,
                )])
            })
        }
    }

    /// Re-frame a nas test state as the desktop host (same ctx, no nas
    /// state) — selects the desktop branches of the host-split
    /// handlers.
    pub(crate) fn as_desktop(state: &ServerState) -> ServerState {
        ServerState::desktop(
            state.schema.clone(),
            state.peer_schema.clone(),
            state.ctx.clone(),
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
        let prefs = Arc::new(
            crate::prefs::Prefs::load(&data_dir.join("prefs.json")).expect("prefs load"),
        );
        seed(&prefs);
        let chat = Arc::new(
            crate::api::chat::ChatState::nas_init(&data_dir, &prefs).expect("chat init"),
        );
        let config = Arc::new(crate::media::config::Config::parse(
            "[server]
http_port = 8080
",
        ));
        let (event_tx, _) = tokio::sync::broadcast::channel(64);
        let ctx = super::nas_ctx::nas_app_ctx(super::nas_ctx::NasCtxInputs {
            data_dir: data_dir.clone(),
            cache_dir: data_dir.join("cache"),
            prefs: prefs.clone(),
            config: config.clone(),
            chat: chat.clone(),
            event_tx,
        })
        .expect("nas app ctx");
        std::mem::forget(dir);
        ServerState {
            schema: Arc::new(StubExec),
            peer_schema: Arc::new(crate::api::peer_graphql::build_schema()),
            ctx,
            nas: Some(Arc::new(NasServerState {
                config,
                cors: cors::CorsPolicy::default(),
                peer_schema: Arc::new(StubPeerExec),
            })),
        }
    }
}

impl ServerState {
    /// Desktop construction: no nas state, so the nas-only routes
    /// (auth/SPA/media alias) stay unmounted. Hosts that build feature
    /// unions with the nas feature still compile — the `nas` field is
    /// filled with `None` here under `cfg`.
    pub fn desktop(
        schema: Arc<dyn GraphqlExec>,
        peer_schema: Arc<PeerSchema>,
        ctx: Arc<AppCtx>,
    ) -> Self {
        Self {
            schema,
            peer_schema,
            ctx,
            #[cfg(feature = "nas")]
            nas: None,
        }
    }

    /// The desktop's concrete schema, when this state carries one (the
    /// nas shim stores its own schema type instead).
    pub fn local_schema(&self) -> Option<Arc<LocalSchema>> {
        self.schema
            .as_any()
            .downcast_ref::<LocalSchema>()
            .map(|s| Arc::new(s.clone()))
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
            post(upload::upload_chunk_handler)
                .layer(DefaultBodyLimit::max(MAX_UPLOAD_BODY_BYTES)),
        )
        .route("/zip/dir", get(zip::zip_dir_handler))
        .route("/zip/files", get(zip::zip_files_handler));

    #[cfg(feature = "nas")]
    {
        if let Some(nas) = state.nas.clone() {
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
                .layer(cors::layer(&nas.cors));
            return router.with_state(state);
        }
    }

    router = router
        .route("/proxyfs", get(proxy_file::proxyfs_handler))
        // DLNA receiver routes (custom GENA methods + GET /description.xml),
        // WebSocket upgrades on any path, and the 404 — same dispatch order
        // the hand-rolled server used.
        .fallback(handlers::fallback);
    router.with_state(state)
}
