use super::{
    batch_plan::{Kind, Plan, Walker},
    client::{File, Link},
};
use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashSet, VecDeque},
    sync::Mutex,
    time::Instant,
};
use tokio::sync::{oneshot, watch};

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Intent {
    pub message_id: String,
    pub kind: Kind,
    pub link: Link,
    pub url_token: String,
    pub entries: Vec<File>,
    pub target_dir: String,
    pub downloads_base: String,
    pub zip_name: String,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub id: String,
    pub message_id: String,
    #[serde(rename = "type")]
    pub kind: Kind,
    pub title: String,
    pub target_dir: String,
    pub link: Link,
    pub url_token: String,
    pub entries: Vec<File>,
    pub zip_name: String,
    pub generation: String,
    pub status: String,
    pub error: String,
    pub downloaded_size: i64,
    pub total_size: i64,
    pub download_speed: i64,
    pub total_files: usize,
    pub done_files: usize,
    pub failed_files: usize,
    pub current_file: String,
    pub packing: bool,
    pub failures: Vec<Value>,
}
#[derive(Clone)]
pub struct Run {
    pub snapshot: Snapshot,
    pub intent: Intent,
    pub plan: Option<Plan>,
    pub completed: HashSet<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub path: String,
    pub bytes: i64,
    pub error: Option<String>,
}
struct Transfer {
    ticket: String,
    size: Option<i64>,
    bytes: i64,
    reply: oneshot::Sender<Receipt>,
}
struct Task {
    run: Run,
    transfer: Option<Transfer>,
    last: Instant,
    last_bytes: i64,
}
#[derive(Default)]
struct State {
    tasks: BTreeMap<String, Task>,
    pending: VecDeque<String>,
    revision: u64,
    stopped: bool,
}
pub struct Queue {
    state: Mutex<State>,
    pub changed: watch::Sender<u64>,
}
impl Default for Queue {
    fn default() -> Self {
        Self {
            state: Mutex::new(State::default()),
            changed: watch::channel(0).0,
        }
    }
}
fn terminal(status: &str) -> bool {
    matches!(status, "COMPLETED" | "PARTIAL" | "FAILED" | "CANCELED")
}
impl Queue {
    fn touch(&self, s: &mut State) {
        s.revision += 1;
        self.changed.send_replace(s.revision);
    }
    pub fn enqueue(&self, intent: Intent) -> Result<String> {
        ensure!(!intent.message_id.is_empty(), "Missing shared message ID");
        intent.link.url("/guest_graphql")?;
        Walker::new(
            intent.kind,
            intent.entries.clone(),
            &intent.target_dir,
            &intent.downloads_base,
        )?;
        if intent.kind == Kind::Zip {
            ensure!(
                !intent.zip_name.is_empty()
                    && !intent.zip_name.contains(['/', '\\', '\0'])
                    && !matches!(intent.zip_name.as_str(), "." | ".."),
                "Invalid archive name"
            );
        }
        let mut paths: Vec<_> = intent
            .entries
            .iter()
            .map(|e| e.virtual_path.as_str())
            .collect();
        paths.sort_unstable();
        paths.dedup();
        let key = json!([
            intent.message_id,
            intent.kind,
            paths,
            intent.target_dir,
            intent.zip_name
        ])
        .to_string();
        let mut s = self.state.lock().unwrap();
        ensure!(!s.stopped, "Core stopped");
        if let Some(t) = s.tasks.values_mut().find(|t| {
            let mut old: Vec<_> = t
                .run
                .intent
                .entries
                .iter()
                .map(|e| e.virtual_path.as_str())
                .collect();
            old.sort_unstable();
            old.dedup();
            json!([
                t.run.intent.message_id,
                t.run.intent.kind,
                old,
                t.run.intent.target_dir,
                t.run.intent.zip_name
            ])
            .to_string()
                == key
        }) {
            let id = t.run.snapshot.id.clone();
            if terminal(&t.run.snapshot.status) {
                t.run.intent.link = intent.link.clone();
                t.run.intent.url_token = intent.url_token.clone();
                t.run.snapshot.link = intent.link;
                t.run.snapshot.url_token = intent.url_token;
                t.run.snapshot.status = "PENDING".into();
                s.pending.push_back(id.clone());
                self.touch(&mut s);
            }
            return Ok(id);
        }
        ensure!(s.tasks.len() < 128, "Shared batch capacity exceeded");
        let id = uuid::Uuid::new_v4().to_string();
        let title = if intent.kind == Kind::Zip {
            intent.zip_name.clone()
        } else {
            format!(
                "{}{}",
                intent.entries[0].name,
                if intent.entries.len() > 1 {
                    format!(" (+{})", intent.entries.len() - 1)
                } else {
                    String::new()
                }
            )
        };
        let snapshot = Snapshot {
            id: id.clone(),
            message_id: intent.message_id.clone(),
            kind: intent.kind,
            title,
            target_dir: intent.target_dir.clone(),
            link: intent.link.clone(),
            url_token: intent.url_token.clone(),
            entries: intent.entries.clone(),
            zip_name: intent.zip_name.clone(),
            generation: String::new(),
            status: "PENDING".into(),
            error: String::new(),
            downloaded_size: 0,
            total_size: 0,
            download_speed: 0,
            total_files: 0,
            done_files: 0,
            failed_files: 0,
            current_file: String::new(),
            packing: false,
            failures: vec![],
        };
        s.tasks.insert(
            id.clone(),
            Task {
                run: Run {
                    snapshot,
                    intent,
                    plan: None,
                    completed: HashSet::new(),
                },
                transfer: None,
                last: Instant::now(),
                last_bytes: 0,
            },
        );
        s.pending.push_back(id.clone());
        self.touch(&mut s);
        Ok(id)
    }
    pub fn claim(&self, blocked: &HashSet<String>) -> Option<Run> {
        let mut s = self.state.lock().unwrap();
        if s.stopped
            || s.tasks
                .values()
                .filter(|t| t.run.snapshot.status == "DOWNLOADING")
                .count()
                >= 3
        {
            return None;
        }
        let count = s.pending.len();
        for _ in 0..count {
            let id = s.pending.pop_front()?;
            if blocked.contains(&id) {
                s.pending.push_back(id);
                continue;
            }
            let Some(t) = s
                .tasks
                .get_mut(&id)
                .filter(|t| t.run.snapshot.status == "PENDING")
            else {
                continue;
            };
            t.run.snapshot.generation = uuid::Uuid::new_v4().to_string();
            t.run.snapshot.status = "DOWNLOADING".into();
            t.run.snapshot.error.clear();
            t.run.snapshot.failures.clear();
            t.run.snapshot.failed_files = 0;
            t.run.snapshot.download_speed = 0;
            t.run.snapshot.packing = false;
            if t.run.intent.kind == Kind::Zip {
                t.run.completed.clear();
            }
            t.last = Instant::now();
            t.last_bytes = 0;
            let run = t.run.clone();
            self.touch(&mut s);
            return Some(run);
        }
        None
    }
    pub fn is_active(&self, id: &str) -> bool {
        self.state
            .lock()
            .unwrap()
            .tasks
            .get(id)
            .is_some_and(|t| t.run.snapshot.status == "DOWNLOADING")
    }
    pub fn control(&self, id: &str, command: &str) -> Result<Option<String>> {
        let mut s = self.state.lock().unwrap();
        ensure!(!s.stopped, "Core stopped");
        let Some(t) = s.tasks.get_mut(id) else {
            return Ok(None);
        };
        let allowed = match command {
            "pause" => matches!(t.run.snapshot.status.as_str(), "PENDING" | "DOWNLOADING"),
            "resume" => t.run.snapshot.status == "PAUSED",
            "retry" => terminal(&t.run.snapshot.status),
            "cancel" => !terminal(&t.run.snapshot.status),
            "remove" => true,
            _ => bail!("Unknown shared batch command"),
        };
        if !allowed {
            return Ok(None);
        }
        let generation = t.run.snapshot.generation.clone();
        t.transfer.take();
        t.run.snapshot.generation.clear();
        t.run.snapshot.download_speed = 0;
        t.run.snapshot.packing = false;
        t.run.snapshot.status = match command {
            "pause" => "PAUSED",
            "cancel" => "CANCELED",
            _ => "PENDING",
        }
        .into();
        s.pending.retain(|v| v != id);
        match command {
            "remove" => {
                s.tasks.remove(id);
            }
            "resume" | "retry" => s.pending.push_back(id.into()),
            _ => {}
        }
        self.touch(&mut s);
        Ok(Some(generation))
    }
    fn active<'a>(s: &'a mut State, id: &str, generation: &str) -> Result<&'a mut Task> {
        s.tasks
            .get_mut(id)
            .filter(|t| {
                t.run.snapshot.status == "DOWNLOADING" && t.run.snapshot.generation == generation
            })
            .ok_or_else(|| anyhow::anyhow!("Shared batch expired"))
    }
    pub fn planned(&self, id: &str, generation: &str, plan: Plan) -> Result<()> {
        let mut s = self.state.lock().unwrap();
        let t = Self::active(&mut s, id, generation)?;
        t.run.snapshot.total_files = plan.total_files;
        t.run.snapshot.total_size = plan.total_size;
        t.run.snapshot.done_files = plan
            .targets
            .iter()
            .filter(|v| t.run.completed.contains(&v.entry.virtual_path))
            .count();
        t.run.snapshot.downloaded_size = plan
            .targets
            .iter()
            .filter(|v| t.run.completed.contains(&v.entry.virtual_path))
            .map(|v| v.entry.size)
            .sum();
        t.last_bytes = t.run.snapshot.downloaded_size;
        t.last = Instant::now();
        t.run.plan = Some(plan);
        self.touch(&mut s);
        Ok(())
    }
    pub fn begin(
        &self,
        id: &str,
        generation: &str,
        file: &str,
        size: Option<i64>,
    ) -> Result<(String, oneshot::Receiver<Receipt>)> {
        let mut s = self.state.lock().unwrap();
        let t = Self::active(&mut s, id, generation)?;
        ensure!(t.transfer.is_none(), "Shared transfer already active");
        let ticket = uuid::Uuid::new_v4().to_string();
        let (reply, receiver) = oneshot::channel();
        t.run.snapshot.current_file = file.into();
        t.run.snapshot.packing = size.is_none();
        t.transfer = Some(Transfer {
            ticket: ticket.clone(),
            size,
            bytes: 0,
            reply,
        });
        self.touch(&mut s);
        Ok((ticket, receiver))
    }
    pub fn progress(&self, id: &str, generation: &str, ticket: &str, bytes: i64) -> Result<bool> {
        let mut s = self.state.lock().unwrap();
        let Ok(t) = Self::active(&mut s, id, generation) else {
            return Ok(false);
        };
        let Some(transfer) = t.transfer.as_mut().filter(|v| v.ticket == ticket) else {
            return Ok(false);
        };
        ensure!(
            bytes >= transfer.bytes && transfer.size.is_none_or(|size| bytes <= size),
            "Invalid shared byte count"
        );
        if transfer.size.is_some() {
            t.run.snapshot.downloaded_size += bytes - transfer.bytes;
            let elapsed = t.last.elapsed().as_secs_f64();
            if elapsed >= 0.5 {
                t.run.snapshot.download_speed =
                    ((t.run.snapshot.downloaded_size - t.last_bytes) as f64 / elapsed).max(0.)
                        as i64;
                t.last_bytes = t.run.snapshot.downloaded_size;
                t.last = Instant::now();
            }
        }
        transfer.bytes = bytes;
        self.touch(&mut s);
        Ok(true)
    }
    pub fn receipt(
        &self,
        id: &str,
        generation: &str,
        ticket: &str,
        mut receipt: Receipt,
    ) -> Result<bool> {
        let mut s = self.state.lock().unwrap();
        let Ok(t) = Self::active(&mut s, id, generation) else {
            return Ok(false);
        };
        if !t.transfer.as_ref().is_some_and(|v| v.ticket == ticket) {
            return Ok(false);
        }
        let transfer = t.transfer.take().unwrap();
        if receipt.error.is_none()
            && (receipt.path.is_empty()
                || receipt.bytes < transfer.bytes
                || transfer.size.is_some_and(|n| receipt.bytes != n))
        {
            receipt.error = Some("Invalid OS save receipt".into());
        }
        let successful = receipt.error.is_none();
        let accepted = transfer.reply.send(receipt).is_ok() && successful;
        self.touch(&mut s);
        Ok(accepted)
    }
    pub fn file_finished(
        &self,
        id: &str,
        generation: &str,
        path: &str,
        error: Option<String>,
    ) -> Result<()> {
        let mut s = self.state.lock().unwrap();
        let t = Self::active(&mut s, id, generation)?;
        t.transfer.take();
        if let Some(error) = error {
            t.run
                .snapshot
                .failures
                .push(json!({"path":path,"error":error}));
            t.run.snapshot.failed_files += 1;
        } else {
            t.run.completed.insert(path.into());
        }
        if let Some(plan) = &t.run.plan {
            t.run.snapshot.done_files = plan
                .targets
                .iter()
                .filter(|v| t.run.completed.contains(&v.entry.virtual_path))
                .count();
            t.run.snapshot.downloaded_size = plan
                .targets
                .iter()
                .filter(|v| t.run.completed.contains(&v.entry.virtual_path))
                .map(|v| v.entry.size)
                .sum();
        }
        t.run.snapshot.download_speed = 0;
        t.last_bytes = t.run.snapshot.downloaded_size;
        t.last = Instant::now();
        self.touch(&mut s);
        Ok(())
    }
    pub fn finish(&self, id: &str, generation: &str, error: Option<String>) {
        let mut s = self.state.lock().unwrap();
        let Ok(t) = Self::active(&mut s, id, generation) else {
            return;
        };
        t.transfer.take();
        t.run.snapshot.current_file.clear();
        t.run.snapshot.packing = false;
        t.run.snapshot.download_speed = 0;
        if let Some(error) = error {
            t.run.snapshot.error = error;
        }
        let failed = t.run.snapshot.failed_files > 0 || !t.run.snapshot.error.is_empty();
        if t.run.intent.kind == Kind::Zip && failed {
            t.run.snapshot.done_files = 0;
            t.run.snapshot.downloaded_size = 0;
        }
        t.run.snapshot.status = if failed {
            if t.run.snapshot.done_files > 0 {
                "PARTIAL"
            } else {
                "FAILED"
            }
        } else {
            "COMPLETED"
        }
        .into();
        self.touch(&mut s);
    }
    pub fn snapshot(&self) -> Value {
        let s = self.state.lock().unwrap();
        json!({"revision":s.revision,"tasks":s.tasks.values().map(|v|&v.run.snapshot).collect::<Vec<_>>()})
    }
    pub fn stop(&self) {
        let mut s = self.state.lock().unwrap();
        s.stopped = true;
        for t in s.tasks.values_mut() {
            t.transfer.take();
            if !terminal(&t.run.snapshot.status) {
                t.run.snapshot.status = "CANCELED".into();
            }
            t.run.snapshot.generation.clear();
            t.run.snapshot.download_speed = 0;
        }
        s.pending.clear();
        self.touch(&mut s);
    }
}
#[cfg(test)]
#[path = "../../tests/unit/shares/batch_queue.rs"]
mod tests;
