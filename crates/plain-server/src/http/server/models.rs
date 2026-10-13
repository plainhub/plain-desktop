use std::sync::Arc;

use crate::assembly::context::AppCtx;
use crate::graphql::schemas::desktop::ApiSchema;
use crate::graphql::schemas::peer::PeerSchema;
use crate::http::routes::local::cors;

#[derive(Clone)]
pub struct ServerState {
    pub schema: Arc<ApiSchema>,
    pub peer_schema: Arc<PeerSchema>,
    pub ctx: Arc<AppCtx>,
    pub settings: Arc<ServerSettings>,
}

pub enum AuthPolicy {
    LocalToken,
    Session,
}

pub struct ServerSettings {
    pub auth: AuthPolicy,
    pub cors: cors::CorsPolicy,
    pub serve_spa: bool,
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

#[cfg(all(test, feature = "system"))]
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

    pub(crate) fn nas_state() -> ServerState {
        nas_state_with(|_| {})
    }

    pub(crate) fn nas_state_with(seed: impl FnOnce(&plain_rs::prefs::Prefs)) -> ServerState {
        let dir = tempfile::tempdir().expect("temp dir");
        let data_dir = dir.path().to_path_buf();
        let prefs = Arc::new(
            plain_rs::prefs::Prefs::load(&plain_rs::prefs::default_path(&data_dir))
                .expect("prefs load"),
        );
        seed(&prefs);
        let chat = Arc::new(
            crate::services::chat::desktop::ChatState::nas_init(&data_dir, &prefs)
                .expect("chat init"),
        );
        let (event_tx, _) = tokio::sync::broadcast::channel(64);
        let ctx = crate::assembly::context::AppCtx::assemble(
            data_dir.clone(),
            data_dir.join("cache"),
            data_dir.join("logs"),
            prefs.clone(),
            chat.clone(),
            event_tx,
            Arc::new(crate::assembly::context::LogShell {
                version: String::new(),
            }),
            8080,
            8443,
        )
        .expect("nas app ctx");
        crate::test_tempdirs::retain(dir);
        ServerState::new(
            Arc::new(crate::graphql::schemas::desktop::build_schema()),
            Arc::new(crate::graphql::schemas::peer::build_schema()),
            ctx,
            ServerSettings {
                auth: AuthPolicy::Session,
                cors: cors::CorsPolicy::default(),
                serve_spa: true,
            },
        )
    }
}
