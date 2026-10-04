use super::{
    attachment_imports::{Imports, Ticket},
    download_status::DownloadStatus as Status,
};
use crate::db::{
    DPeer, Db,
    chat_store::{messages, peers},
};
use anyhow::{Result, bail};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, VecDeque},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Instant,
};
use tokio::sync::watch;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub id: String,
    pub message_id: String,
    pub file: Value,
    pub peer: DPeer,
    pub status: Status,
    pub error: String,
    pub downloaded: u64,
    pub total: u64,
    pub speed: u64,
    pub generation: String,
}
struct Task {
    snapshot: Snapshot,
    ticket: Option<Ticket>,
    last: Instant,
    last_bytes: u64,
}
#[derive(Clone)]
pub enum Effect {
    Start { task: Snapshot, ticket: Ticket },
    Cancel { token: String },
}
#[derive(Default)]
struct State {
    tasks: BTreeMap<String, Task>,
    pending: VecDeque<String>,
    effects: VecDeque<Effect>,
    revision: u64,
}
pub struct Queue {
    db: Db,
    directory: PathBuf,
    imports: Arc<Imports>,
    state: Mutex<State>,
    pub changed: watch::Sender<u64>,
}
impl Queue {
    pub fn new(db: Db, directory: PathBuf, imports: Arc<Imports>) -> Self {
        Self {
            db,
            directory,
            imports,
            state: Mutex::new(State::default()),
            changed: watch::channel(0).0,
        }
    }
    pub fn enqueue(&self, message_id: &str, id: &str, peer_id: &str) -> Result<bool> {
        let mut s = self.state.lock().unwrap();
        if s.tasks.contains_key(id) {
            return Ok(false);
        }
        self.capacity(&s)?;
        if s.tasks.len() >= 128 {
            bail!("Download task capacity exceeded");
        }
        let peer =
            peers::get(&self.db, peer_id)?.ok_or_else(|| anyhow::anyhow!("Peer unavailable"))?;
        let row = messages::get(&self.db, message_id)?
            .ok_or_else(|| anyhow::anyhow!("Message unavailable"))?;
        if peer_id != row.from_id && peer_id != row.to_id {
            bail!("Attachment peer does not belong to message");
        }
        let content: Value = serde_json::from_str(&row.content)?;
        if !matches!(content["type"].as_str(), Some("FILES" | "IMAGES")) {
            bail!("Message has no attachments");
        }
        let file = content["value"]["items"]
            .as_array()
            .and_then(|items| items.iter().find(|v| v["id"].as_str() == Some(id)))
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Attachment unavailable"))?;
        if !file["uri"].as_str().is_some_and(|v| v.starts_with("fsid:")) {
            bail!("Attachment is already local");
        }
        let total = file["size"]
            .as_u64()
            .filter(|n| *n <= i64::MAX as u64)
            .ok_or_else(|| anyhow::anyhow!("Invalid attachment size"))?;
        s.tasks.insert(
            id.into(),
            Task {
                snapshot: Snapshot {
                    id: id.into(),
                    message_id: message_id.into(),
                    file,
                    peer,
                    status: Status::Pending,
                    error: String::new(),
                    downloaded: 0,
                    total,
                    speed: 0,
                    generation: String::new(),
                },
                ticket: None,
                last: Instant::now(),
                last_bytes: 0,
            },
        );
        s.pending.push_back(id.into());
        self.schedule(&mut s);
        self.touch(&mut s);
        Ok(true)
    }
    pub fn control(&self, id: &str, action: &str) -> Result<bool> {
        let mut s = self.state.lock().unwrap();
        self.capacity(&s)?;
        let Some(task) = s.tasks.get_mut(id) else {
            return Ok(false);
        };
        let status = task.snapshot.status;
        let allowed = match action {
            "pause" => matches!(status, Status::Pending | Status::Downloading),
            "resume" => status == Status::Paused,
            "retry" => status == Status::Failed,
            "cancel" => matches!(
                status,
                Status::Pending | Status::Downloading | Status::Paused
            ),
            "remove" => true,
            _ => bail!("Unknown download action"),
        };
        if !allowed {
            return Ok(false);
        }
        let cancel = task.ticket.take();
        task.snapshot.speed = 0;
        task.snapshot.status = match action {
            "pause" => Status::Paused,
            "cancel" => Status::Canceled,
            _ => Status::Pending,
        };
        if let Some(ticket) = cancel {
            self.imports.abort(&ticket.token)?;
            s.effects.push_back(Effect::Cancel {
                token: ticket.token,
            });
        }
        s.pending.retain(|pending| pending != id);
        match action {
            "remove" => {
                s.tasks.remove(id);
            }
            "retry" | "resume" => {
                s.pending.push_back(id.into());
            }
            _ => {}
        }
        self.schedule(&mut s);
        self.touch(&mut s);
        Ok(true)
    }
    pub fn progress(&self, id: &str, generation: &str, downloaded: u64) -> Result<bool> {
        let mut s = self.state.lock().unwrap();
        let Some(task) = s.tasks.get_mut(id).filter(|t| {
            t.snapshot.status == Status::Downloading && t.snapshot.generation == generation
        }) else {
            return Ok(false);
        };
        if downloaded < task.snapshot.downloaded || downloaded > task.snapshot.total {
            bail!("Invalid download byte count");
        }
        let elapsed = task.last.elapsed().as_secs_f64();
        task.snapshot.downloaded = downloaded;
        if elapsed >= 0.5 {
            task.snapshot.speed = ((downloaded - task.last_bytes) as f64 / elapsed) as u64;
            task.last_bytes = downloaded;
            task.last = Instant::now();
        }
        self.touch(&mut s);
        Ok(true)
    }
    pub fn finish(&self, id: &str, generation: &str, error: Option<String>) -> Result<bool> {
        let mut s = self.state.lock().unwrap();
        let Some(task) = s.tasks.get_mut(id).filter(|t| {
            t.snapshot.status == Status::Downloading && t.snapshot.generation == generation
        }) else {
            return Ok(false);
        };
        let ticket = task
            .ticket
            .take()
            .ok_or_else(|| anyhow::anyhow!("Missing download ticket"))?;
        let committed = match error {
            Some(error) => {
                self.imports.abort(&ticket.token)?;
                Err(anyhow::anyhow!(error))
            }
            None => self
                .imports
                .finish(&self.db, &self.directory, &ticket.token),
        };
        task.snapshot.speed = 0;
        match committed {
            Ok(_) => {
                task.snapshot.status = Status::Completed;
                task.snapshot.downloaded = task.snapshot.total;
                task.snapshot.error.clear();
            }
            Err(error) => {
                task.snapshot.status = Status::Failed;
                task.snapshot.error = error.to_string();
            }
        }
        self.schedule(&mut s);
        self.touch(&mut s);
        Ok(true)
    }
    pub fn snapshot(&self) -> Value {
        let s = self.state.lock().unwrap();
        json!({"revision":s.revision,"tasks":s.tasks.values().map(|t|&t.snapshot).collect::<Vec<_>>()})
    }
    pub fn public_progress(&self) -> String {
        let s = self.state.lock().unwrap();
        json!(s.tasks.values().map(|t|json!({"id":t.snapshot.id,"messageId":t.snapshot.message_id,"downloaded":t.snapshot.downloaded,"total":t.snapshot.total,"speed":t.snapshot.speed,"status":t.snapshot.status.as_str().to_lowercase()})).collect::<Vec<_>>()).to_string()
    }
    pub fn effect(&self) -> Option<Effect> {
        self.state.lock().unwrap().effects.pop_front()
    }
    fn capacity(&self, s: &State) -> Result<()> {
        if s.effects.len() > 250 {
            bail!("Download command capacity exceeded");
        }
        Ok(())
    }
    fn touch(&self, s: &mut State) {
        s.revision += 1;
        self.changed.send_replace(s.revision);
    }
    fn schedule(&self, s: &mut State) {
        while s
            .tasks
            .values()
            .filter(|t| t.snapshot.status == Status::Downloading)
            .count()
            < 3
        {
            let Some(id) = s.pending.pop_front() else {
                break;
            };
            let Some(task) = s
                .tasks
                .get_mut(&id)
                .filter(|t| t.snapshot.status == Status::Pending)
            else {
                continue;
            };
            let result = (|| {
                let peer = peers::get(&self.db, &task.snapshot.peer.id)?
                    .ok_or_else(|| anyhow::anyhow!("Peer unavailable"))?;
                let uri = task.snapshot.file["uri"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Invalid attachment URI"))?;
                let ticket = self.imports.begin(
                    &self.db,
                    &self.directory,
                    &task.snapshot.message_id,
                    &id,
                    uri,
                )?;
                task.snapshot.peer = peer;
                task.snapshot.file["fileName"] = json!(ticket.file_name);
                task.snapshot.file["size"] = json!(ticket.size);
                task.snapshot.total = ticket.size as u64;
                Ok::<_, anyhow::Error>(ticket)
            })();
            match result {
                Ok(ticket) => {
                    task.snapshot.status = Status::Downloading;
                    task.snapshot.generation = ticket.token.clone();
                    task.snapshot.downloaded = 0;
                    task.snapshot.speed = 0;
                    task.snapshot.error.clear();
                    task.last = Instant::now();
                    task.last_bytes = 0;
                    task.ticket = Some(ticket.clone());
                    s.effects.push_back(Effect::Start {
                        task: task.snapshot.clone(),
                        ticket,
                    });
                }
                Err(error) => {
                    task.snapshot.status = Status::Failed;
                    task.snapshot.error = error.to_string();
                }
            }
        }
    }
}
impl Drop for Queue {
    fn drop(&mut self) {
        if let Ok(s) = self.state.get_mut() {
            for task in s.tasks.values() {
                if let Some(ticket) = &task.ticket {
                    let _ = self.imports.abort(&ticket.token);
                }
            }
        }
    }
}
#[cfg(test)]
#[path = "../../tests/unit/chat/download_queue.rs"]
mod tests;
