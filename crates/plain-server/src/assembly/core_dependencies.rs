use crate::host::bridge::Host;
use plain_rs::{db::Db, events::WsEvent, prefs::Prefs};
use std::{path::PathBuf, sync::Arc};
use tokio::sync::broadcast;

pub(crate) struct CoreDependencies {
    pub(crate) db: Arc<Db>,
    pub(crate) prefs: Arc<Prefs>,
    pub(crate) directory: PathBuf,
    pub(crate) token: String,
    pub(crate) events: broadcast::Sender<WsEvent>,
    pub(crate) host: Arc<Host>,
}

impl CoreDependencies {
    pub(crate) fn validate_token(token: &str) -> Result<(), String> {
        if plain_rs::utils::base64::base64_decode(token).len() != 32 {
            return Err("token must contain 32 random bytes".into());
        }
        Ok(())
    }

    pub(crate) fn new(
        db: Arc<Db>,
        prefs: Arc<Prefs>,
        directory: PathBuf,
        token: String,
        events: broadcast::Sender<WsEvent>,
        host: Arc<Host>,
    ) -> Self {
        Self {
            db,
            prefs,
            directory,
            token,
            events,
            host,
        }
    }
}
