use plain_rs::{db::Db, events::WsEvent, prefs::Prefs};
use std::{path::PathBuf, sync::Arc};
use tokio::sync::broadcast;

pub(crate) struct SharedServices {
    pub pomodoro: Arc<crate::services::pomodoro::Service>,
    pub feeds: Arc<crate::services::feeds::SyncService>,
    pub feed_assets: Arc<crate::services::feeds::FeedAssets>,
    pub image_updates: Arc<plain_rs::media::images::editor::Updates>,
    pub uploads: Arc<plain_rs::files::uploads::Runtime>,
}

impl SharedServices {
    pub(crate) fn new(
        db: Arc<Db>,
        prefs: Arc<Prefs>,
        events: broadcast::Sender<WsEvent>,
        directory: PathBuf,
        uploads: Arc<plain_rs::files::uploads::Runtime>,
    ) -> Self {
        let feed_assets = Arc::new(crate::services::feeds::FeedAssets {
            db: db.clone(),
            directory,
        });
        Self {
            pomodoro: crate::services::pomodoro::Service::new(db.clone(), prefs, events.clone()),
            feeds: crate::services::feeds::SyncService::new(
                db,
                events.clone(),
                Some(feed_assets.clone()),
            ),
            image_updates: plain_rs::media::images::editor::Updates::new(events),
            feed_assets,
            uploads,
        }
    }
}
