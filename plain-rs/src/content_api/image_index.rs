use super::{host::Host, image_index_host::HostProvider};
use crate::{
    db::Db,
    library::{
        LibraryError, LibraryResult,
        image_embeddings::{self, EmbeddingInput},
        image_indexing::{self, Control, Progress},
    },
};
use serde::Serialize;
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};
#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub version: u64,
    pub is_running: bool,
    pub total_images: usize,
    pub indexed_images: usize,
    pub skipped_images: usize,
    pub error_message: String,
}
enum Job {
    Full(bool),
    Selected(Vec<String>),
}
#[derive(Default)]
struct State {
    generation: u64,
    worker: bool,
    pending: VecDeque<Job>,
    status: Status,
    suspended: bool,
}
pub struct ImageIndex {
    db: Arc<Db>,
    host: Arc<Host>,
    state: Arc<Mutex<State>>,
}
impl ImageIndex {
    pub fn new(db: Arc<Db>, host: Arc<Host>) -> Self {
        Self {
            db,
            host,
            state: Arc::new(Mutex::new(State::default())),
        }
    }
    pub fn status(&self) -> Status {
        self.state.lock().unwrap().status.clone()
    }
    pub fn start(self: &Arc<Self>, force: bool) -> LibraryResult<Status> {
        self.enqueue(Job::Full(force))
    }
    pub fn selected(self: &Arc<Self>, ids: Vec<String>) -> LibraryResult<Status> {
        if ids.is_empty() {
            return Ok(self.status());
        }
        if ids.len() > 4096 || ids.iter().any(|id| id.is_empty()) {
            return Err(LibraryError::Other("invalid image index selection".into()));
        }
        let mut ids = ids
            .into_iter()
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        ids.sort();
        self.enqueue(Job::Selected(ids))
    }
    fn enqueue(self: &Arc<Self>, job: Job) -> LibraryResult<Status> {
        let spawn = {
            let mut s = self.state.lock().unwrap();
            s.suspended = false;
            let mut merged = false;
            if let Job::Full(force) = &job {
                if let Some(Job::Full(queued)) =
                    s.pending.iter_mut().find(|j| matches!(j, Job::Full(_)))
                {
                    *queued |= *force;
                    merged = true;
                }
            }
            if !merged {
                if s.pending.len() >= 32 {
                    return Err(LibraryError::Other(
                        "image index queue capacity exceeded".into(),
                    ));
                }
                s.pending.push_back(job);
            }
            if s.worker {
                false
            } else {
                s.worker = true;
                s.status.is_running = true;
                true
            }
        };
        if spawn {
            let service = self.clone();
            tokio::spawn(async move {
                service.work().await;
            });
        }
        Ok(self.status())
    }
    pub fn cancel(&self) -> LibraryResult<Status> {
        let mut s = self.state.lock().unwrap();
        s.generation = s
            .generation
            .checked_add(1)
            .ok_or_else(|| LibraryError::Other("image index generation overflow".into()))?;
        s.pending.clear();
        s.suspended = true;
        s.status.version = s.generation;
        s.status.error_message.clear();
        Ok(s.status.clone())
    }
    pub fn remove(&self, ids: &[String]) -> LibraryResult<usize> {
        if ids.is_empty() {
            return Ok(0);
        }
        let mut s = self.state.lock().unwrap();
        s.generation = s
            .generation
            .checked_add(1)
            .ok_or_else(|| LibraryError::Other("image index generation overflow".into()))?;
        s.status.version = s.generation;
        let count = image_embeddings::delete(&self.db, ids)?;
        s.status.indexed_images = image_embeddings::count(&self.db)?
            .try_into()
            .map_err(|_| LibraryError::Other("invalid image embedding count".into()))?;
        if s.worker && !s.suspended {
            let force = s.pending.iter().any(|j| matches!(j, Job::Full(true)));
            s.pending.clear();
            s.pending.push_back(Job::Full(force));
        }
        Ok(count)
    }
    pub fn clear(&self) -> LibraryResult<usize> {
        let mut s = self.state.lock().unwrap();
        s.generation = s
            .generation
            .checked_add(1)
            .ok_or_else(|| LibraryError::Other("image index generation overflow".into()))?;
        s.pending.clear();
        s.suspended = true;
        s.status.version = s.generation;
        let count = image_embeddings::clear(&self.db)?;
        s.status.indexed_images = 0;
        Ok(count)
    }
    async fn publish(&self) {
        let status = self.status();
        let _ = self
            .host
            .call("imageIndexProgress", serde_json::to_value(status).unwrap())
            .await;
    }
    async fn work(self: Arc<Self>) {
        loop {
            let next = {
                let mut s = self.state.lock().unwrap();
                match s.pending.pop_front() {
                    Some(job) => {
                        let Some(generation) = s.generation.checked_add(1) else {
                            s.status.error_message = "image index generation overflow".into();
                            s.worker = false;
                            s.status.is_running = false;
                            return;
                        };
                        s.generation = generation;
                        s.status = Status {
                            version: generation,
                            is_running: true,
                            ..Default::default()
                        };
                        Some((generation, job))
                    }
                    None => {
                        s.worker = false;
                        s.status.is_running = false;
                        None
                    }
                }
            };
            let Some((generation, job)) = next else {
                self.publish().await;
                return;
            };
            let control = IndexControl {
                generation,
                state: self.state.clone(),
                host: self.host.clone(),
                runtime: tokio::runtime::Handle::current(),
            };
            let provider = HostProvider::new(self.host.clone(), generation.to_string());
            let db = self.db.clone();
            let result = tokio::task::spawn_blocking(move || {
                let mut provider = provider;
                let result = match job {
                    Job::Full(force) => image_indexing::scan(&db, &mut provider, &control, force),
                    Job::Selected(ids) => {
                        image_indexing::selected(&db, &mut provider, &control, &ids)
                    }
                };
                let finished = provider.finish();
                match result {
                    Ok(progress) => {
                        finished?;
                        Ok(progress)
                    }
                    Err(error) => Err(error),
                }
            })
            .await;
            {
                let mut s = self.state.lock().unwrap();
                if s.generation == generation {
                    match result {
                        Ok(Ok(progress)) => {
                            if progress.skipped > 0 {
                                s.status.error_message =
                                    format!("{} images could not be decoded", progress.skipped);
                            }
                        }
                        Ok(Err(e)) => s.status.error_message = e.to_string(),
                        Err(e) => s.status.error_message = e.to_string(),
                    }
                }
            }
        }
    }
}
struct IndexControl {
    generation: u64,
    state: Arc<Mutex<State>>,
    host: Arc<Host>,
    runtime: tokio::runtime::Handle,
}
impl IndexControl {
    fn valid(&self, s: &State) -> LibraryResult<()> {
        if s.generation == self.generation {
            Ok(())
        } else {
            Err(LibraryError::Other("image index cancelled".into()))
        }
    }
}
impl Control for IndexControl {
    fn check(&self) -> LibraryResult<()> {
        self.valid(&self.state.lock().unwrap())
    }
    fn save(&self, db: &Db, items: &[EmbeddingInput]) -> LibraryResult<()> {
        let s = self.state.lock().unwrap();
        self.valid(&s)?;
        image_embeddings::save(db, items)
    }
    fn remove(&self, db: &Db, ids: &[String]) -> LibraryResult<()> {
        let s = self.state.lock().unwrap();
        self.valid(&s)?;
        image_embeddings::delete(db, ids)?;
        Ok(())
    }
    fn progress(&self, p: Progress) {
        let status = {
            let mut s = self.state.lock().unwrap();
            if self.valid(&s).is_err() {
                return;
            }
            s.status.total_images = p.total;
            s.status.indexed_images = p.indexed;
            s.status.skipped_images = p.skipped;
            s.status.clone()
        };
        let _ = self.runtime.block_on(
            self.host
                .call("imageIndexProgress", serde_json::to_value(status).unwrap()),
        );
    }
}

#[cfg(test)]
#[path = "../../tests/unit/content_api/image_index.rs"]
mod tests;
