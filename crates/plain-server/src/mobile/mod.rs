#[cfg(feature = "core")]
pub mod deletes;

pub mod contacts;

pub mod sms;

pub mod phone_calls;

pub mod packages;

pub mod query;

use std::sync::Arc;
pub(crate) struct Runtime {
    pub(crate) mms: Arc<sms::mms::Runtime>,
}
impl Runtime {
    pub(crate) fn new(
        host: Arc<crate::host::bridge::Host>,
        prefs: Arc<plain_rs::prefs::Prefs>,
        db: Arc<plain_rs::db::Db>,
        directory: std::path::PathBuf,
        events: tokio::sync::broadcast::Sender<plain_rs::events::WsEvent>,
    ) -> Arc<Self> {
        Arc::new(Self {
            mms: sms::mms::Runtime::new(host, prefs, db, directory, events),
        })
    }
}
pub mod graphql;
pub mod http;
