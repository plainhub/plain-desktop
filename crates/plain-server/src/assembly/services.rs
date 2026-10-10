use std::sync::Arc;
#[derive(Clone)]
pub(crate) struct Services {
    pub files: Arc<crate::services::files::file_tasks::FileTasks>,
    pub audio: Arc<crate::services::audio::host::Audio>,
    pub index: Arc<crate::services::images::index::ImageIndex>,
}
impl Services {
    pub fn new(
        db: Arc<plain_rs::db::Db>,
        host: Arc<crate::host::bridge::Host>,
        events: tokio::sync::broadcast::Sender<plain_rs::events::WsEvent>,
        prefs: Arc<plain_rs::prefs::Prefs>,
    ) -> Self {
        let audio = Arc::new(crate::services::audio::host::Audio::new(
            db.clone(),
            host.clone(),
        ));
        let index = Arc::new(crate::services::images::index::ImageIndex::new(
            db.clone(),
            host.clone(),
        ));
        let files = Arc::new(crate::services::files::file_tasks::FileTasks::new(
            db,
            host,
            events,
            audio.clone(),
            index.clone(),
            prefs,
        ));
        Self {
            files,
            audio,
            index,
        }
    }
}
