use std::sync::Arc;
#[derive(Clone)]
pub(crate) struct Services {
    pub archives: Arc<crate::services::files::archives::Archives>,
    pub app_files: Arc<plain_rs::files::app_files::FileStore>,
    pub shares: Arc<plain_rs::files::shares::Service>,
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
        let app_files = plain_rs::files::app_files::FileStore::new(db.clone(), directory.clone());
        let shares = plain_rs::files::shares::Service::new(db.clone(), prefs.clone());
        let media_actions = Arc::new(crate::services::files::media_actions::MediaActions::new(
            db,
            host,
            audio.clone(),
            index.clone(),
            prefs,
            events,
        ));
        Self {
            archives: Arc::new(crate::services::files::archives::Archives::new(
                directory.join("archives"),
            )),
            app_files,
            shares,
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
    pub(crate) fn configure_schema<
        Q: async_graphql::ObjectType + 'static,
        M: async_graphql::ObjectType + 'static,
        S: async_graphql::SubscriptionType + 'static,
    >(
        self,
        builder: async_graphql::SchemaBuilder<Q, M, S>,
        db: Arc<plain_rs::db::Db>,
        host: Arc<crate::host::bridge::Host>,
        prefs: Arc<plain_rs::prefs::Prefs>,
        events: tokio::sync::broadcast::Sender<plain_rs::events::WsEvent>,
    ) -> async_graphql::SchemaBuilder<Q, M, S> {
        builder
            .data(crate::graphql::resolvers::tags::loader::new(db.clone()))
            .data(db)
            .data(host)
            .data(prefs)
            .data(events)
            .data(self.app_files)
            .data(self.shares)
            .data(self.media_actions)
            .data(self.files)
            .data(self.audio)
            .data(self.index)
            .data(self.pomodoro)
            .data(self.image_updates)
            .data(self.feeds)
            .data(self.feed_assets)
            .data(self.upload_chunks)
            .data(self.uploads)
    }
}
