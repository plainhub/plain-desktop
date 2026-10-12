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
        let pomodoro =
            crate::services::pomodoro::Service::new(db.clone(), prefs.clone(), events.clone());
        let feed_assets = Arc::new(crate::services::feeds::FeedAssets {
            db: db.clone(),
            directory,
        });
        let feeds = crate::services::feeds::SyncService::new(
            db.clone(),
            events.clone(),
            Some(feed_assets.clone()),
        );
        let image_updates = plain_rs::media::images::editor::Updates::new(events.clone());
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
        let media_actions = Arc::new(crate::services::files::media_actions::MediaActions::new(
            db,
            host,
            audio.clone(),
            index.clone(),
            prefs,
            events,
        ));
        Self {
            media_actions,
            files,
            audio,
            index,
            pomodoro,
            feeds,
            feed_assets,
            image_updates,
            uploads,
        }
    }
}
