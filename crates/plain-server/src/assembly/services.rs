use std::sync::Arc;
#[derive(Clone)]
pub(crate) struct Services {
    pub media_actions: Arc<crate::services::files::media_actions::MediaActions>,
    pub files: Arc<crate::services::files::file_tasks::FileTasks>,
    pub audio: Arc<crate::services::audio::host::Audio>,
    pub index: Arc<crate::services::images::index::ImageIndex>,
    pub pomodoro: Arc<crate::services::pomodoro::Service>,
    pub feeds: Arc<crate::services::feeds::SyncService>,
    pub feed_assets: Arc<crate::services::feeds::FeedAssets>,
    pub image_updates: Arc<plain_rs::media::images::editor::Updates>,
    pub upload_chunks: Arc<crate::services::files::upload_chunks::UploadChunks>,
    pub uploads: Arc<plain_rs::files::uploads::Runtime>,
}
impl Services {
    pub fn new(
        db: Arc<plain_rs::db::Db>,
        host: Arc<crate::host::bridge::Host>,
        events: tokio::sync::broadcast::Sender<plain_rs::events::WsEvent>,
        prefs: Arc<plain_rs::prefs::Prefs>,
        directory: std::path::PathBuf,
    ) -> Self {
        let shared = Self::shared(
            db.clone(),
            host.clone(),
            events.clone(),
            prefs.clone(),
            directory.clone(),
        );
        Self::with_shared(db, host, events, prefs, directory, shared)
    }

    pub(crate) fn shared(
        db: Arc<plain_rs::db::Db>,
        host: Arc<crate::host::bridge::Host>,
        events: tokio::sync::broadcast::Sender<plain_rs::events::WsEvent>,
        prefs: Arc<plain_rs::prefs::Prefs>,
        directory: std::path::PathBuf,
    ) -> crate::assembly::shared_services::SharedServices {
        let uploads = Arc::new(plain_rs::files::uploads::Runtime::with_completion({
            let host = host.clone();
            move |path| {
                let host = host.clone();
                Box::pin(async move {
                    if let Err(error) = host
                        .call(
                            "scanFilesFacts",
                            serde_json::json!({"paths": [path.to_string_lossy()]}),
                        )
                        .await
                    {
                        log::warn!("Merged upload media scan failed: {error}");
                    }
                })
            }
        }));
        crate::assembly::shared_services::SharedServices::new(db, prefs, events, directory, uploads)
    }

    pub(crate) fn with_shared(
        db: Arc<plain_rs::db::Db>,
        host: Arc<crate::host::bridge::Host>,
        events: tokio::sync::broadcast::Sender<plain_rs::events::WsEvent>,
        prefs: Arc<plain_rs::prefs::Prefs>,
        directory: std::path::PathBuf,
        shared: crate::assembly::shared_services::SharedServices,
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
            db.clone(),
            host.clone(),
            events.clone(),
            audio.clone(),
            index.clone(),
            prefs.clone(),
        ));
        let media_actions = Arc::new(crate::services::files::media_actions::MediaActions::new(
            db,
            host,
            audio.clone(),
            index.clone(),
            prefs,
            events,
        ));
        Self {
            upload_chunks: Arc::new(crate::services::files::upload_chunks::UploadChunks::new(
                directory.join("uploads"),
            )),
            media_actions,
            files,
            audio,
            index,
            pomodoro: shared.pomodoro,
            feeds: shared.feeds,
            feed_assets: shared.feed_assets,
            image_updates: shared.image_updates,
            uploads: shared.uploads,
        }
    }
}
