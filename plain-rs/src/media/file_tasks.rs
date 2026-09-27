//! Async file task manager (copy/move). Port of
//! `internal/graph/file_tasks_async.go` + `file_tasks_store.go`.
//!
//! Architecture
//! ============
//! A single global `FileTaskManager`:
//!   * owns a tokio task pool, with one worker
//!   * a bounded MPSC `queue` of task ids
//!   * an in-memory `HashMap<id, FileTask>` for live state
//!
//! Each task runs a copy/move pipeline and emits a `FileTask` snapshot
//! through the global `eventbus` (channel 6 = `EVENT_FILE_TASK_PROGRESS`).
//! Snapshots are also persisted to the KV store under `filetask:<cid>:<id>` at
//! most every 200ms (throttle); final state (DONE/ERROR) is always
//! persisted.
//!
//! Frontend subscribes via WebSocket (see `ws_hub.rs`) to receive the
//! same payload format as the Go side.

use crate::media::eventbus::EVENT_FILE_TASK_PROGRESS;
use crate::media::kv::get_default;
use crate::media::eventbus;
use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tokio::sync::mpsc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FileTaskType {
    Copy,
    Move,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FileTaskStatus {
    Queued,
    Running,
    Done,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileTaskOp {
    pub src: String,
    pub dst: String,
    pub overwrite: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileTask {
    pub id: String,
    pub client_id: String,
    #[serde(rename = "type")]
    pub kind: FileTaskType,
    pub title: String,
    pub status: FileTaskStatus,
    pub error: String,
    pub total_bytes: i64,
    pub done_bytes: i64,
    pub total_items: i64,
    pub done_items: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip)]
    pub last_persist: Option<DateTime<Utc>>,
}

const NS: &str = "filetask:";

fn db_key(client_id: &str, task_id: &str) -> String {
    format!("{NS}{client_id}:{task_id}")
}

/// Live task map (id → FileTask).
type TaskMap = HashMap<String, FileTask>;
type TaskSlot = (TaskMap, Option<DateTime<Utc>>);

struct Manager {
    queue: mpsc::UnboundedSender<String>,
    state: Mutex<HashMap<String, parking_lot::Mutex<TaskSlot>>>,
}

static MGR: OnceLock<Manager> = OnceLock::new();

fn get_manager() -> &'static Manager {
    MGR.get_or_init(|| {
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        tokio::spawn(async move {
            while let Some(id) = rx.recv().await {
                run_task_by_id(&id).await;
            }
        });
        Manager {
            queue: tx,
            state: Mutex::new(HashMap::new()),
        }
    })
}

pub fn create(client_id: &str, kind: FileTaskType, title: &str, ops: Vec<FileTaskOp>) -> FileTask {
    let now = Utc::now();
    let task = FileTask {
        id: crate::utils::shortid::new_id(),
        client_id: client_id.to_string(),
        kind,
        title: title.to_string(),
        status: FileTaskStatus::Queued,
        error: String::new(),
        total_bytes: 0,
        done_bytes: 0,
        total_items: 0,
        done_items: 0,
        created_at: now,
        updated_at: now,
        last_persist: None,
    };
    let m = get_manager();
    {
        let mut g = m.state.lock();
        g.insert(
            task.id.clone(),
            parking_lot::Mutex::new((HashMap::new(), None)),
        );
    }
    ops_map().lock().insert(task.id.clone(), ops);
    publish_snapshot(&task);
    let _ = m.queue.send(task.id.clone());
    task
}

pub fn get(id: &str) -> Option<FileTask> {
    let m = get_manager();
    let g = m.state.lock();
    g.get(id).and_then(|slot| {
        let (map, _) = &*slot.lock();
        map.get("__self__").cloned()
    })
}

pub fn list_for_client(client_id: &str) -> Vec<FileTask> {
    // Read from the KV store (persisted state survives restarts) first, then merge
    // in-memory live state.
    let mut out: Vec<FileTask> = load_from_db(client_id).unwrap_or_default();
    let m = get_manager();
    let g = m.state.lock();
    for t in g.values() {
        let (map, _) = &*t.lock();
        if let Some(t) = map.get("__self__") {
            if t.client_id == client_id {
                out.push(t.clone());
            }
        }
    }
    out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    out
}

// ----- Ops store (per task id) -----
static OPS: OnceLock<parking_lot::Mutex<HashMap<String, Vec<FileTaskOp>>>> = OnceLock::new();
fn ops_map() -> &'static parking_lot::Mutex<HashMap<String, Vec<FileTaskOp>>> {
    OPS.get_or_init(|| parking_lot::Mutex::new(HashMap::new()))
}

fn take_ops(id: &str) -> Vec<FileTaskOp> {
    ops_map().lock().remove(id).unwrap_or_default()
}

// ----- Worker -----

async fn run_task_by_id(id: &str) {
    let m = get_manager();
    let snapshot = {
        let g = m.state.lock();
        let Some(slot) = g.get(id) else {
            return;
        };
        let (map, _) = &*slot.lock();
        map.get("__self__").cloned()
    };
    let Some(mut task) = snapshot else {
        return;
    };
    let ops = take_ops(id);
    let total = compute_totals(&ops);
    task.total_bytes = total.0;
    task.total_items = total.1;
    task.status = FileTaskStatus::Running;
    task.updated_at = Utc::now();
    write_self(&task);
    publish_snapshot(&task);

    let last_emit = std::sync::Arc::new(parking_lot::Mutex::new(Utc::now()));

    let mut err: Option<String> = None;
    for op in ops {
        let cb_done_bytes = std::sync::Arc::new(parking_lot::Mutex::new(0i64));
        let cb_done_items = std::sync::Arc::new(parking_lot::Mutex::new(0i64));
        let id_str = id.to_string();
        let cb_bytes_outer = cb_done_bytes.clone();
        let cb_items_outer = cb_done_items.clone();
        let progress = Progress {
            on_bytes: Box::new({
                let cb = cb_done_bytes.clone();
                let id = id_str.clone();
                let emit_fn = last_emit.clone();
                move |n: i64| {
                    *cb.lock() += n;
                    if let Some(t) = get(&id) {
                        let mut t = t;
                        t.done_bytes = *cb_bytes_outer.lock();
                        t.updated_at = Utc::now();
                        write_self(&t);
                        publish_snapshot_with_throttle(&t, &emit_fn);
                    }
                }
            }),
            on_item: Box::new({
                let cb = cb_done_items.clone();
                let id = id_str.clone();
                let emit_fn = last_emit.clone();
                move || {
                    *cb.lock() += 1;
                    if let Some(t) = get(&id) {
                        let mut t = t;
                        t.done_items = *cb_items_outer.lock();
                        t.updated_at = Utc::now();
                        write_self(&t);
                        publish_snapshot_with_throttle(&t, &emit_fn);
                    }
                }
            }),
        };
        let r = match task.kind {
            FileTaskType::Copy => copy_op(&op.src, &op.dst, op.overwrite, &progress).await,
            FileTaskType::Move => move_op(&op.src, &op.dst, op.overwrite, &progress).await,
        };
        if let Err(e) = r {
            err = Some(e.to_string());
            break;
        }
    }
    let final_snap = get(id);
    if let Some(mut t) = final_snap {
        match err {
            Some(msg) => {
                t.status = FileTaskStatus::Error;
                t.error = msg;
            }
            None => {
                t.status = FileTaskStatus::Done;
                t.error.clear();
            }
        }
        t.updated_at = Utc::now();
        write_self(&t);
        persist_to_db(&t);
        publish_snapshot(&t);
    }
    // Free the slot
    m.state.lock().remove(id);
}

fn write_self(t: &FileTask) {
    let m = get_manager();
    let g = m.state.lock();
    if let Some(slot) = g.get(&t.id) {
        let mut inner = slot.lock();
        inner.0.insert("__self__".to_string(), t.clone());
    }
}

fn snapshot_json(t: &FileTask) -> serde_json::Value {
    serde_json::json!({
        "id": t.id,
        "type": t.kind,
        "title": t.title,
        "status": t.status,
        "error": t.error,
        "totalBytes": t.total_bytes,
        "doneBytes": t.done_bytes,
        "totalItems": t.total_items,
        "doneItems": t.done_items,
        "createdAt": t.created_at,
        "updatedAt": t.updated_at,
    })
}

fn publish_snapshot(t: &FileTask) {
    let cid = t.client_id.clone();
    eventbus::EventBus::global().publish_with_cid(EVENT_FILE_TASK_PROGRESS, &cid, snapshot_json(t));
    // Persist on terminal state; throttle the rest.
    let terminal = matches!(t.status, FileTaskStatus::Done | FileTaskStatus::Error);
    if terminal {
        persist_to_db(t);
        return;
    }
    let now = Utc::now();
    let should = match t.last_persist {
        Some(last) => (now - last) > chrono::Duration::milliseconds(200),
        None => true,
    };
    if should {
        persist_to_db_with_last(t, Some(now));
    }
}

/// Snapshot publish + emit throttle. Used from worker progress callbacks
/// to avoid emitting on every byte.
fn publish_snapshot_with_throttle(
    t: &FileTask,
    last_emit: &std::sync::Arc<parking_lot::Mutex<DateTime<Utc>>>,
) {
    let mut le = last_emit.lock();
    let now = Utc::now();
    if (now - *le).num_milliseconds() >= 200 {
        *le = now;
        publish_snapshot(t);
    }
}

fn persist_to_db(t: &FileTask) {
    persist_to_db_with_last(t, None);
}
fn persist_to_db_with_last(t: &FileTask, last: Option<DateTime<Utc>>) {
    let mut stored = t.clone();
    stored.last_persist = last.or(stored.last_persist);
    if let Ok(json) = serde_json::to_vec(&stored) {
        let db = get_default();
        let _ = db.insert(db_key(&t.client_id, &t.id), json);
    }
}

fn load_from_db(client_id: &str) -> Result<Vec<FileTask>> {
    let db = get_default();
    let prefix = format!("{NS}{client_id}:");
    let mut out: Vec<FileTask> = Vec::new();
    for kv in db.scan_prefix(&prefix) {
        let (_, v) = kv?;
        if let Ok(t) = serde_json::from_slice::<FileTask>(&v) {
            out.push(t);
        }
    }
    Ok(out)
}

fn compute_totals(ops: &[FileTaskOp]) -> (i64, i64) {
    let mut total_bytes = 0i64;
    let mut total_items = 0i64;
    for op in ops {
        let s = Path::new(&op.src);
        let Ok(meta) = std::fs::symlink_metadata(s) else {
            continue;
        };
        if meta.is_dir() {
            for entry in crate::media::walk::Walk::new(s).into_iter().filter_map(|e| e.ok()) {
                if entry.file_type().is_file() {
                    if let Ok(m) = entry.metadata() {
                        total_bytes += m.len() as i64;
                    }
                    total_items += 1;
                }
            }
        } else {
            total_bytes += meta.len() as i64;
            total_items += 1;
        }
    }
    (total_bytes, total_items)
}

// ----- Copy / Move with progress -----

pub struct Progress {
    pub on_bytes: Box<dyn Fn(i64) + Send + Sync + 'static>,
    pub on_item: Box<dyn Fn() + Send + Sync + 'static>,
}

async fn copy_op(src: &str, dst: &str, overwrite: bool, p: &Progress) -> Result<()> {
    let src = clean(src);
    let dst = clean(dst);
    let s_meta =
        std::fs::symlink_metadata(&src).map_err(|e| anyhow!("stat {}: {e}", src.display()))?;
    if s_meta.is_dir() {
        let resolved = resolve_dst(&src, dst.to_str().unwrap(), &s_meta, overwrite)?;
        ensure_no_self_copy(&src, &resolved)?;
        tokio::fs::create_dir_all(&resolved).await.ok();
        for entry in crate::media::walk::Walk::new(&src)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let p_src = entry.path();
            let rel = p_src.strip_prefix(&src).unwrap_or(p_src);
            let p_dst = resolved.join(rel);
            if entry.file_type().is_dir() {
                tokio::fs::create_dir_all(&p_dst).await.ok();
            } else {
                if let Some(parent) = p_dst.parent() {
                    tokio::fs::create_dir_all(parent).await.ok();
                }
                let bytes = tokio::fs::read(p_src).await?;
                tokio::fs::write(&p_dst, &bytes).await?;
                (p.on_bytes)(bytes.len() as i64);
                (p.on_item)();
            }
        }
    } else {
        let resolved = resolve_dst(&src, dst.to_str().unwrap(), &s_meta, overwrite)?;
        if let Some(parent) = resolved.parent() {
            tokio::fs::create_dir_all(parent).await.ok();
        }
        let bytes = tokio::fs::read(&src).await?;
        tokio::fs::write(&resolved, &bytes).await?;
        (p.on_bytes)(bytes.len() as i64);
        (p.on_item)();
    }
    Ok(())
}

async fn move_op(src: &str, dst: &str, overwrite: bool, p: &Progress) -> Result<()> {
    // Try cross-filesystem rename first; if it fails, fall back to copy + delete.
    let src = clean(src);
    let dst = clean(dst);
    let s_meta =
        std::fs::symlink_metadata(&src).map_err(|e| anyhow!("stat {}: {e}", src.display()))?;
    let resolved = resolve_dst(&src, dst.to_str().unwrap(), &s_meta, overwrite)?;
    match std::fs::rename(&src, &resolved) {
        Ok(()) => {
            let sz = s_meta.len() as i64;
            (p.on_bytes)(sz);
            (p.on_item)();
            Ok(())
        }
        Err(_) => {
            copy_op(src.to_str().unwrap(), resolved.to_str().unwrap(), true, p).await?;
            if s_meta.is_dir() {
                tokio::fs::remove_dir_all(&src).await.ok();
            } else {
                tokio::fs::remove_file(&src).await.ok();
            }
            Ok(())
        }
    }
}

fn resolve_dst(
    src: &Path,
    dst: &str,
    _s_meta: &std::fs::Metadata,
    overwrite: bool,
) -> Result<PathBuf> {
    let dst_p = clean(dst);
    if dst_p == src {
        // duplicate: same name in same dir — shared `name_1.ext` convention
        return Ok(unique_path(src));
    }
    if let Ok(d) = std::fs::metadata(&dst_p) {
        if d.is_dir() {
            return Ok(dst_p.join(src.file_name().unwrap_or_default()));
        }
    }
    if !overwrite {
        // unique suffix
        if dst_p.exists() {
            return Ok(unique_path(&dst_p));
        }
    }
    Ok(dst_p)
}

fn unique_path(target: &Path) -> PathBuf {
    crate::utils::unique_path::unique_sibling(target)
}

fn ensure_no_self_copy(src: &Path, dst: &Path) -> Result<()> {
    let sa = std::fs::canonicalize(src).unwrap_or_else(|_| src.to_path_buf());
    let da = std::fs::canonicalize(dst).unwrap_or_else(|_| dst.to_path_buf());
    if da.starts_with(&sa) {
        Err(anyhow!("cannot copy a directory into itself"))
    } else {
        Ok(())
    }
}

fn clean<P: AsRef<Path>>(p: P) -> PathBuf {
    let s = p.as_ref().to_string_lossy().to_string();
    let mut out = PathBuf::new();
    for c in s.split('/').filter(|x| !x.is_empty()) {
        match c {
            "." => {}
            ".." => {
                out.pop();
            }
            x => out.push(x),
        }
    }
    if s.starts_with('/') {
        out = PathBuf::from("/").join(out);
    }
    out
}

// ----- Public API used by GraphQL -----

pub fn create_copy_task(client_id: &str, ops: Vec<FileTaskOp>) -> Result<FileTask> {
    if client_id.is_empty() {
        return Err(anyhow!("unauthorized"));
    }
    if ops.is_empty() {
        return Err(anyhow!("no operations"));
    }
    Ok(create(client_id, FileTaskType::Copy, "Copy files", ops))
}

pub fn create_move_task(client_id: &str, ops: Vec<FileTaskOp>) -> Result<FileTask> {
    if client_id.is_empty() {
        return Err(anyhow!("unauthorized"));
    }
    if ops.is_empty() {
        return Err(anyhow!("no operations"));
    }
    Ok(create(client_id, FileTaskType::Move, "Move files", ops))
}

pub fn list_tasks(client_id: &str) -> Result<Vec<FileTask>> {
    if client_id.is_empty() {
        return Err(anyhow!("unauthorized"));
    }
    Ok(list_for_client(client_id))
}

#[cfg(test)]
#[path = "../../tests/unit/media/file_tasks.rs"]
mod tests;
