use super::{audio_library::HostLibrary, host::Host};
use crate::{db::Db, library::LibraryResult};
use std::sync::Arc;
use tokio::sync::{Mutex, Semaphore};

pub struct Audio {
    db: Arc<Db>,
    host: Arc<Host>,
    lock: Arc<Mutex<()>>,
    capacity: Arc<Semaphore>,
}
impl Audio {
    pub fn new(db: Arc<Db>, host: Arc<Host>) -> Self {
        Self {
            db,
            host,
            lock: Arc::new(Mutex::new(())),
            capacity: Arc::new(Semaphore::new(32)),
        }
    }
    pub async fn run<T: Send + 'static>(
        &self,
        operation: impl FnOnce(&Db, &mut HostLibrary) -> LibraryResult<T> + Send + 'static,
    ) -> async_graphql::Result<T> {
        let db = self.db.clone();
        let permit = self
            .capacity
            .clone()
            .try_acquire_owned()
            .map_err(|_| async_graphql::Error::new("audio request capacity exceeded"))?;
        let guard = self.lock.clone().lock_owned().await;
        let mut library = HostLibrary::new(self.host.clone());
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let _guard = guard;
            operation(&db, &mut library)
        })
        .await
        .map_err(|e| async_graphql::Error::new(e.to_string()))?
        .map_err(|e| async_graphql::Error::new(e.to_string()))
    }
}
