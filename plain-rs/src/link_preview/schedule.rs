use crate::db::{DChat, Db};
use futures_util::{FutureExt, future::BoxFuture};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tokio::sync::{Notify, mpsc, watch};
type Fetch = Arc<
    dyn Fn(Db, PathBuf, String) -> BoxFuture<'static, anyhow::Result<Option<DChat>>> + Send + Sync,
>;
struct Pending {
    dirty: bool,
    changed: Arc<Notify>,
}
pub struct Schedule {
    sender: mpsc::Sender<String>,
    pending: Arc<Mutex<HashMap<String, Pending>>>,
}
impl Schedule {
    pub fn new(
        db: Db,
        directory: PathBuf,
        publish: Arc<dyn Fn(DChat) + Send + Sync>,
        stop: watch::Receiver<bool>,
    ) -> Self {
        Self::with_fetch(
            db,
            directory,
            publish,
            stop,
            Arc::new(|db, directory, id| {
                Box::pin(async move { super::refresh(&db, &directory, &id).await })
            }),
        )
    }
    fn with_fetch(
        db: Db,
        directory: PathBuf,
        publish: Arc<dyn Fn(DChat) + Send + Sync>,
        stop: watch::Receiver<bool>,
        fetch: Fetch,
    ) -> Self {
        let (sender, receiver) = mpsc::channel::<String>(128);
        let receiver = Arc::new(tokio::sync::Mutex::new(receiver));
        let pending = Arc::new(Mutex::new(HashMap::<String, Pending>::new()));
        for _ in 0..4 {
            let db = db.clone();
            let directory = directory.clone();
            let publish = publish.clone();
            let receiver = receiver.clone();
            let pending = pending.clone();
            let mut stop = stop.clone();
            let fetch = fetch.clone();
            tokio::spawn(async move {
                loop {
                    let next = tokio::select! {next=async {receiver.lock().await.recv().await}=>next,_=stop.changed()=>break};
                    let Some(id) = next else { break };
                    loop {
                        let changed = {
                            let mut pending = pending.lock().unwrap();
                            let Some(slot) = pending.get_mut(&id) else {
                                break;
                            };
                            slot.dirty = false;
                            let _ = slot.changed.notified().now_or_never();
                            slot.changed.clone()
                        };
                        let result = tokio::select! {result=fetch(db.clone(),directory.clone(),id.clone())=>result,_=changed.notified()=>continue,_=stop.changed()=>return};
                        match result {
                            Ok(Some(row)) => publish(row),
                            Err(error) => log::warn!("Link preview refresh failed: {error}"),
                            _ => {}
                        }
                        let rerun = {
                            let mut pending = pending.lock().unwrap();
                            match pending.get(&id) {
                                Some(slot) if slot.dirty => true,
                                _ => {
                                    pending.remove(&id);
                                    false
                                }
                            }
                        };
                        if !rerun {
                            break;
                        }
                    }
                }
            });
        }
        Self { sender, pending }
    }
    pub fn request(&self, id: &str) -> bool {
        let mut pending = self.pending.lock().unwrap();
        if let Some(slot) = pending.get_mut(id) {
            slot.dirty = true;
            slot.changed.notify_one();
            return true;
        }
        if pending.len() >= 128 {
            return false;
        }
        pending.insert(
            id.into(),
            Pending {
                dirty: false,
                changed: Arc::new(Notify::new()),
            },
        );
        if self.sender.try_send(id.into()).is_err() {
            pending.remove(id);
            false
        } else {
            true
        }
    }
}
#[cfg(test)]
#[path = "../../tests/unit/link_preview/schedule.rs"]
mod tests;
