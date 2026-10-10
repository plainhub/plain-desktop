use std::sync::Arc;
#[derive(Clone)]
pub(crate) struct Services {
    pub files: Arc<crate::adapters::mobile::files::file_tasks::FileTasks>,
    pub audio: Arc<crate::adapters::mobile::media::audio::Audio>,
    pub index: Arc<crate::adapters::mobile::media::image_index::ImageIndex>,
}
impl Services {
    pub fn new(
        db: Arc<plain_rs::db::Db>,
        host: Arc<crate::adapters::mobile::runtime::host::Host>,
        events: tokio::sync::broadcast::Sender<plain_rs::ws_event::WsEvent>,
        prefs: Arc<plain_rs::prefs::Prefs>,
    ) -> Self {
        let audio = Arc::new(crate::adapters::mobile::media::audio::Audio::new(
            db.clone(),
            host.clone(),
        ));
        let index = Arc::new(
            crate::adapters::mobile::media::image_index::ImageIndex::new(db.clone(), host.clone()),
        );
        let files = Arc::new(crate::adapters::mobile::files::file_tasks::FileTasks::new(
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
