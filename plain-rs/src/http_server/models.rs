use std::sync::Arc;

use crate::api::context::AppCtx;
use crate::http_server::main_schemas::ApiSchema;
use crate::http_server::peer_schemas::PeerSchema;
use crate::http_server::routes::cors;

#[derive(Clone)]
pub struct ServerState {
    pub schema: Arc<ApiSchema>,
    pub peer_schema: Arc<PeerSchema>,
    pub ctx: Arc<AppCtx>,
    pub settings: Arc<ServerSettings>,
}

pub enum AuthPolicy {
    LocalToken,
    Session { dev_token: String, device_id: String },
}

pub struct ServerSettings {
    pub auth: AuthPolicy,
    pub cors: cors::CorsPolicy,
    pub serve_spa: bool,
}

impl ServerState {
    pub fn new(schema: Arc<ApiSchema>, peer_schema: Arc<PeerSchema>, ctx: Arc<AppCtx>, settings: ServerSettings) -> Self {
        Self { schema, peer_schema, ctx, settings: Arc::new(settings) }
    }
}

#[cfg(all(test, feature = "system"))]
pub(crate) mod test_support {
    use super::*;

    pub(crate) fn as_desktop(state: &ServerState) -> ServerState {
        ServerState::new(state.schema.clone(), state.peer_schema.clone(), state.ctx.clone(), ServerSettings {
            auth: AuthPolicy::LocalToken,
            cors: cors::CorsPolicy::permissive_default(),
            serve_spa: false,
        })
    }

    pub(crate) fn nas_state() -> ServerState { nas_state_with(|_| {}) }

    pub(crate) fn nas_state_with(seed: impl FnOnce(&crate::prefs::Prefs)) -> ServerState {
        let dir = tempfile::tempdir().expect("temp dir");
        let data_dir = dir.path().to_path_buf();
        let prefs = Arc::new(crate::prefs::Prefs::load(&crate::prefs::default_path(&data_dir)).expect("prefs load"));
        seed(&prefs);
        let chat = Arc::new(crate::chat_service::ChatState::nas_init(&data_dir, &prefs).expect("chat init"));
        let config = Arc::new(crate::media::config::Config::parse("[server]\nhttp_port = 8080\n"));
        let (event_tx, _) = tokio::sync::broadcast::channel(64);
        let ctx = crate::api::context::AppCtx::assemble(
            data_dir.clone(), data_dir.join("cache"), data_dir.join("logs"), prefs.clone(), chat.clone(), event_tx,
            Arc::new(crate::api::context::LogShell { version: String::new() }), 8080, 8443,
        ).expect("nas app ctx");
        std::mem::forget(dir);
        ServerState::new(
            Arc::new(crate::http_server::main_schemas::build_schema()),
            Arc::new(crate::http_server::peer_schemas::build_schema()), ctx,
            ServerSettings { auth: AuthPolicy::Session { dev_token: config.get_string("auth.dev_token"), device_id: config.get_string("nas.id") }, cors: cors::CorsPolicy::default(), serve_spa: true },
        )
    }
}
