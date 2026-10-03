use std::sync::Arc;
#[derive(Clone)]
pub(crate) struct Services {
    pub files: Arc<super::file_tasks::FileTasks>,
    pub audio: Arc<super::audio::Audio>,
    pub index: Arc<super::image_index::ImageIndex>,
}
impl Services {
    pub fn new(
        db: Arc<crate::db::Db>,
        host: Arc<super::host::Host>,
        events: tokio::sync::broadcast::Sender<crate::ws_event::WsEvent>,
        prefs: Arc<crate::prefs::Prefs>,
    ) -> Self {
        let audio = Arc::new(super::audio::Audio::new(db.clone(), host.clone()));
        let index = Arc::new(super::image_index::ImageIndex::new(
            db.clone(),
            host.clone(),
        ));
        let files = Arc::new(super::file_tasks::FileTasks::new(
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
