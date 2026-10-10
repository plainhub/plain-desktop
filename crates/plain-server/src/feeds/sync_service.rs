use crate::{db::Db, ws_event::WsEvent};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use tokio::sync::{Mutex as AsyncMutex, broadcast};
#[derive(Clone, async_graphql::SimpleObject)]
pub struct FeedSyncState {
    pub feed_id: async_graphql::ID,
    pub status: String,
    pub error: String,
}
pub struct SyncService {
    db: Arc<Db>,
    events: broadcast::Sender<WsEvent>,
    states: Mutex<BTreeMap<String, FeedSyncState>>,
    assets: Option<Arc<super::FeedAssets>>,
    gate: AsyncMutex<()>,
}
impl SyncService {
    pub fn new(
        db: Arc<Db>,
        events: broadcast::Sender<WsEvent>,
        assets: Option<Arc<super::FeedAssets>>,
    ) -> Arc<Self> {
        Arc::new(Self {
            db,
            events,
            states: Mutex::new(BTreeMap::new()),
            assets,
            gate: AsyncMutex::new(()),
        })
    }
    pub fn states(&self) -> Vec<FeedSyncState> {
        self.states.lock().unwrap().values().cloned().collect()
    }
    pub fn queue(self: &Arc<Self>, id: Option<String>) {
        let key = id.clone().unwrap_or_else(|| "all".into());
        {
            let mut states = self.states.lock().unwrap();
            if states.get(&key).is_some_and(|s| s.status == "PENDING") {
                return;
            }
            states.insert(
                key.clone(),
                FeedSyncState {
                    feed_id: key.clone().into(),
                    status: "PENDING".into(),
                    error: String::new(),
                },
            );
        }
        let this = self.clone();
        let _ = this
            .events
            .send(WsEvent::broadcast("CONTENT_CHANGED", "{}".into()));
        tokio::spawn(async move {
            let _guard = this.gate.lock().await;
            let error = super::sync_with_assets(
                this.db.clone(),
                this.events.clone(),
                id,
                this.assets.clone(),
            )
            .await
            .err()
            .map(|e| e.to_string())
            .unwrap_or_default();
            this.states.lock().unwrap().insert(
                key.clone(),
                FeedSyncState {
                    feed_id: key.into(),
                    status: if error.is_empty() {
                        "COMPLETED"
                    } else {
                        "ERROR"
                    }
                    .into(),
                    error,
                },
            );
            let _ = this
                .events
                .send(WsEvent::broadcast("CONTENT_CHANGED", "{}".into()));
        });
    }
}
