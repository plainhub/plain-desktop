use std::sync::{Arc, Mutex};

use serde::Serialize;
use tokio::sync::watch;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Running,
    Paused,
    Cancelling,
    Cancelled,
    Completed,
    Failed,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub id: String,
    pub status: Status,
    pub total_files: Option<usize>,
    pub processed_files: usize,
    pub error: Option<String>,
}

struct Job {
    snapshot: Snapshot,
    cancel: watch::Sender<bool>,
    activity: watch::Sender<()>,
}

#[derive(Default)]
struct State {
    job: Option<Job>,
    closed: bool,
    policy: Option<Arc<super::media_scan_policy::Policy>>,
    task: Option<tokio::task::JoinHandle<()>>,
}

#[derive(Default)]
pub(crate) struct MediaScans {
    state: Mutex<State>,
    events: Option<tokio::sync::broadcast::Sender<plain_rs::events::WsEvent>>,
}

impl MediaScans {
    pub(crate) fn new(events: tokio::sync::broadcast::Sender<plain_rs::events::WsEvent>) -> Self {
        Self {
            state: Mutex::new(State::default()),
            events: Some(events),
        }
    }

    fn publish(&self, snapshot: &Snapshot) {
        let Some(events) = &self.events else {
            return;
        };
        let state = match snapshot.status {
            Status::Running => "RUNNING",
            Status::Paused => "PAUSED",
            Status::Cancelling | Status::Cancelled => "STOPPED",
            Status::Completed => "IDLE",
            Status::Failed => "FAILED",
        };
        let payload = serde_json::json!({
            "id": snapshot.id,
            "indexed": snapshot.processed_files,
            "pending": snapshot.total_files.map(|total| total.saturating_sub(snapshot.processed_files)),
            "total": snapshot.total_files,
            "state": state,
            "error": snapshot.error,
        });
        let _ = events.send(plain_rs::events::WsEvent::broadcast(
            plain_rs::events::MEDIA_SCAN_PROGRESS,
            payload.to_string(),
        ));
    }

    pub(crate) fn begin(self: &Arc<Self>, total_files: usize) -> Result<Session, String> {
        self.begin_with_total(Some(total_files))
    }

    pub(crate) fn begin_discovery(self: &Arc<Self>) -> Result<Session, String> {
        self.begin_with_total(None)
    }

    fn begin_with_total(self: &Arc<Self>, total_files: Option<usize>) -> Result<Session, String> {
        let mut guard = self
            .state
            .lock()
            .map_err(|_| "Media scan state unavailable")?;
        if guard.closed {
            return Err("Rust core stopped".into());
        }
        if guard.job.as_ref().is_some_and(|job| {
            matches!(
                job.snapshot.status,
                Status::Running | Status::Paused | Status::Cancelling
            )
        }) {
            return Err("Media scan already running".into());
        }
        let (cancel, receiver) = watch::channel(false);
        let (activity, _) = watch::channel(());
        let id = uuid::Uuid::new_v4().to_string();
        guard.job = Some(Job {
            snapshot: Snapshot {
                id: id.clone(),
                status: Status::Running,
                total_files,
                processed_files: 0,
                error: None,
            },
            cancel,
            activity,
        });
        if let Some(job) = &guard.job {
            self.publish(&job.snapshot);
        }
        Ok(Session {
            scans: self.clone(),
            id,
            cancel: receiver,
        })
    }

    pub(crate) fn spawn<F>(
        &self,
        runtime: &tokio::runtime::Handle,
        session: Session,
        run: impl FnOnce(Session) -> F,
    ) -> Result<Snapshot, String>
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        let mut guard = self
            .state
            .lock()
            .map_err(|_| "Media scan state unavailable")?;
        if guard.closed {
            return Err("Rust core stopped".into());
        }
        let snapshot = guard
            .job
            .as_ref()
            .filter(|job| job.snapshot.id == session.id)
            .ok_or("Media scan replaced")?
            .snapshot
            .clone();
        guard.task = Some(runtime.spawn(run(session)));
        Ok(snapshot)
    }

    pub(crate) async fn wait_background(&self) -> Result<(), String> {
        let task = self
            .state
            .lock()
            .map_err(|_| "Media scan state unavailable")?
            .task
            .take();
        if let Some(task) = task {
            task.await.map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    pub(crate) fn configure(&self, policy: super::media_scan_policy::Policy) -> Result<(), String> {
        let mut guard = self
            .state
            .lock()
            .map_err(|_| "Media scan state unavailable")?;
        if guard.closed {
            return Err("Rust core stopped".into());
        }
        if guard.job.as_ref().is_some_and(|job| {
            matches!(
                job.snapshot.status,
                Status::Running | Status::Paused | Status::Cancelling
            )
        }) {
            return Err("Media scan already running".into());
        }
        guard.policy = Some(Arc::new(policy));
        Ok(())
    }

    pub(crate) fn policy(&self) -> Result<Arc<super::media_scan_policy::Policy>, String> {
        self.state
            .lock()
            .map_err(|_| "Media scan state unavailable")?
            .policy
            .clone()
            .ok_or_else(|| "Media scan directories not configured".into())
    }

    pub(crate) fn record_completion<T>(
        &self,
        record: impl FnOnce() -> Result<T, String>,
    ) -> Result<T, String> {
        let guard = self
            .state
            .lock()
            .map_err(|_| "Media scan state unavailable")?;
        if guard.closed {
            return Err("Rust core stopped".into());
        }
        let result = record();
        drop(guard);
        result
    }

    pub(crate) fn snapshot(&self) -> Result<Option<Snapshot>, String> {
        Ok(self
            .state
            .lock()
            .map_err(|_| "Media scan state unavailable")?
            .job
            .as_ref()
            .map(|job| job.snapshot.clone()))
    }

    pub(crate) fn pause(&self) -> Result<Option<Snapshot>, String> {
        self.set_paused(true)
    }

    pub(crate) fn resume(&self) -> Result<Option<Snapshot>, String> {
        self.set_paused(false)
    }

    fn set_paused(&self, paused: bool) -> Result<Option<Snapshot>, String> {
        let mut guard = self
            .state
            .lock()
            .map_err(|_| "Media scan state unavailable")?;
        if guard.closed {
            return Err("Rust core stopped".into());
        }
        if let Some(job) = guard.job.as_mut()
            && matches!(job.snapshot.status, Status::Running | Status::Paused)
        {
            job.snapshot.status = if paused {
                Status::Paused
            } else {
                Status::Running
            };
            job.activity.send_replace(());
        }
        if let Some(job) = &guard.job {
            self.publish(&job.snapshot);
        }
        Ok(guard.job.as_ref().map(|job| job.snapshot.clone()))
    }

    pub(crate) fn cancel(&self) -> Result<Option<Snapshot>, String> {
        let mut guard = self
            .state
            .lock()
            .map_err(|_| "Media scan state unavailable")?;
        if let Some(job) = guard.job.as_mut()
            && matches!(job.snapshot.status, Status::Running | Status::Paused)
        {
            job.snapshot.status = Status::Cancelling;
            let _ = job.cancel.send(true);
            job.activity.send_replace(());
        }
        if let Some(job) = &guard.job {
            self.publish(&job.snapshot);
        }
        Ok(guard.job.as_ref().map(|job| job.snapshot.clone()))
    }
    pub(crate) fn close(&self) -> Result<(), String> {
        let mut guard = self
            .state
            .lock()
            .map_err(|_| "Media scan state unavailable")?;
        guard.closed = true;
        if let Some(job) = guard.job.as_mut()
            && matches!(job.snapshot.status, Status::Running | Status::Paused)
        {
            job.snapshot.status = Status::Cancelling;
            let _ = job.cancel.send(true);
            job.activity.send_replace(());
        }
        if let Some(job) = &guard.job {
            self.publish(&job.snapshot);
        }
        Ok(())
    }
}

pub(crate) struct Session {
    scans: Arc<MediaScans>,
    id: String,
    pub(crate) cancel: watch::Receiver<bool>,
}

impl Session {
    pub(crate) async fn wait_running(&self) -> Result<(), String> {
        loop {
            let mut activity = {
                let guard = self
                    .scans
                    .state
                    .lock()
                    .map_err(|_| "Media scan state unavailable")?;
                if guard.closed {
                    return Err("Rust core stopped".into());
                }
                let job = guard
                    .job
                    .as_ref()
                    .filter(|job| job.snapshot.id == self.id)
                    .ok_or("Media scan replaced")?;
                match job.snapshot.status {
                    Status::Running => return Ok(()),
                    Status::Paused => job.activity.subscribe(),
                    _ => return Err("Media scan cancelled".into()),
                }
            };
            activity
                .changed()
                .await
                .map_err(|_| "Media scan session closed")?;
        }
    }

    pub(crate) async fn record_when_running<T>(
        &self,
        record: impl FnOnce() -> Result<T, String>,
    ) -> Result<T, String> {
        let mut record = Some(record);
        loop {
            let mut activity = {
                let mut guard = self
                    .scans
                    .state
                    .lock()
                    .map_err(|_| "Media scan state unavailable")?;
                if guard.closed {
                    return Err("Rust core stopped".into());
                }
                let job = guard
                    .job
                    .as_mut()
                    .filter(|job| job.snapshot.id == self.id)
                    .ok_or("Media scan replaced")?;
                match job.snapshot.status {
                    Status::Running => {
                        let value = record.take().ok_or("Media scan record unavailable")?()?;
                        job.snapshot.processed_files += 1;
                        self.scans.publish(&job.snapshot);
                        return Ok(value);
                    }
                    Status::Paused => job.activity.subscribe(),
                    _ => return Err("Media scan cancelled".into()),
                }
            };
            activity
                .changed()
                .await
                .map_err(|_| "Media scan session closed")?;
        }
    }

    pub(crate) fn record<T>(
        &self,
        record: impl FnOnce() -> Result<T, String>,
    ) -> Result<T, String> {
        let mut guard = self
            .scans
            .state
            .lock()
            .map_err(|_| "Media scan state unavailable")?;
        let job = guard
            .job
            .as_mut()
            .filter(|job| job.snapshot.id == self.id)
            .ok_or("Media scan replaced")?;
        if job.snapshot.status != Status::Running {
            return Err("Media scan cancelled".into());
        }
        let value = record()?;
        job.snapshot.processed_files += 1;
        self.scans.publish(&job.snapshot);
        Ok(value)
    }

    pub(crate) fn discovered(&self, total_files: usize) -> Result<(), String> {
        let mut guard = self
            .scans
            .state
            .lock()
            .map_err(|_| "Media scan state unavailable")?;
        let job = guard
            .job
            .as_mut()
            .filter(|job| job.snapshot.id == self.id)
            .ok_or("Media scan replaced")?;
        if !matches!(job.snapshot.status, Status::Running | Status::Paused) {
            return Err("Media scan cancelled".into());
        }
        job.snapshot.total_files = Some(total_files);
        self.scans.publish(&job.snapshot);
        Ok(())
    }

    pub(crate) fn finish(&self, result: Result<(), String>) -> Result<Snapshot, String> {
        let mut guard = self
            .scans
            .state
            .lock()
            .map_err(|_| "Media scan state unavailable")?;
        let job = guard
            .job
            .as_mut()
            .filter(|job| job.snapshot.id == self.id)
            .ok_or("Media scan replaced")?;
        if job.snapshot.status == Status::Cancelling {
            job.snapshot.status = Status::Cancelled;
        } else if matches!(job.snapshot.status, Status::Running | Status::Paused) {
            match result {
                Ok(()) => job.snapshot.status = Status::Completed,
                Err(error) => {
                    job.snapshot.status = Status::Failed;
                    job.snapshot.error = Some(error);
                }
            }
        }
        if matches!(job.snapshot.status, Status::Cancelled | Status::Failed) {
            let _ = job.cancel.send(true);
        }
        job.activity.send_replace(());
        self.scans.publish(&job.snapshot);
        Ok(job.snapshot.clone())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.scans.state.lock()
            && let Some(job) = guard.job.as_mut().filter(|job| job.snapshot.id == self.id)
            && matches!(
                job.snapshot.status,
                Status::Running | Status::Paused | Status::Cancelling
            )
        {
            let _ = job.cancel.send(true);
            job.activity.send_replace(());
            job.snapshot.status = Status::Cancelled;
            self.scans.publish(&job.snapshot);
        }
    }
}

pub(crate) async fn wait_stop(receiver: &mut watch::Receiver<bool>) {
    let _ = receiver.wait_for(|stopped| *stopped).await;
}

#[cfg(test)]
#[path = "../../../tests/unit/services/files/media_scans.rs"]
mod tests;
