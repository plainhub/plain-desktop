use std::sync::{Arc, Mutex};

use serde::Serialize;
use tokio::sync::watch;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Running,
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
}

#[derive(Default)]
struct State {
    job: Option<Job>,
    closed: bool,
    policy: Option<Arc<super::media_scan_policy::Policy>>,
}

#[derive(Default)]
pub(crate) struct MediaScans {
    state: Mutex<State>,
}

impl MediaScans {
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
        if guard
            .job
            .as_ref()
            .is_some_and(|job| matches!(job.snapshot.status, Status::Running | Status::Cancelling))
        {
            return Err("Media scan already running".into());
        }
        let (cancel, receiver) = watch::channel(false);
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
        });
        Ok(Session {
            scans: self.clone(),
            id,
            cancel: receiver,
        })
    }

    pub(crate) fn configure(&self, policy: super::media_scan_policy::Policy) -> Result<(), String> {
        let mut guard = self
            .state
            .lock()
            .map_err(|_| "Media scan state unavailable")?;
        if guard.closed {
            return Err("Rust core stopped".into());
        }
        if guard
            .job
            .as_ref()
            .is_some_and(|job| matches!(job.snapshot.status, Status::Running | Status::Cancelling))
        {
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

    pub(crate) fn snapshot(&self) -> Result<Option<Snapshot>, String> {
        Ok(self
            .state
            .lock()
            .map_err(|_| "Media scan state unavailable")?
            .job
            .as_ref()
            .map(|job| job.snapshot.clone()))
    }

    pub(crate) fn cancel(&self) -> Result<Option<Snapshot>, String> {
        let mut guard = self
            .state
            .lock()
            .map_err(|_| "Media scan state unavailable")?;
        if let Some(job) = guard.job.as_mut()
            && job.snapshot.status == Status::Running
        {
            job.snapshot.status = Status::Cancelling;
            let _ = job.cancel.send(true);
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
            && job.snapshot.status == Status::Running
        {
            job.snapshot.status = Status::Cancelling;
            let _ = job.cancel.send(true);
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
        if job.snapshot.status != Status::Running {
            return Err("Media scan cancelled".into());
        }
        job.snapshot.total_files = Some(total_files);
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
        } else if job.snapshot.status == Status::Running {
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
        Ok(job.snapshot.clone())
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        if let Ok(mut guard) = self.scans.state.lock()
            && let Some(job) = guard.job.as_mut().filter(|job| job.snapshot.id == self.id)
            && matches!(job.snapshot.status, Status::Running | Status::Cancelling)
        {
            let _ = job.cancel.send(true);
            job.snapshot.status = Status::Cancelled;
        }
    }
}

pub(crate) async fn wait_stop(receiver: &mut watch::Receiver<bool>) {
    let _ = receiver.wait_for(|stopped| *stopped).await;
}

#[cfg(test)]
#[path = "../../../tests/unit/services/files/media_scans.rs"]
mod tests;
