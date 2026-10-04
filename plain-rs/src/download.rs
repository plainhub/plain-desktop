//! Peer file download queue — mirrors plain-app's `DownloadQueue` +
//! `PeerFileDownloader`.
//!
//! When a peer-to-peer chat message carries `fsid:` URIs, the file lives
//! on the peer's device. This module downloads those files into the local
//! content-addressable store (`app_file_store`) and rewrites the message
//! content from `fsid:<peer-id>` to `fid:<hash>.<ext>`, exactly like
//! plain-app's `PeerFileDownloader.updateMessageFileUri`.
//!
//! Progress is reported via `WS_DOWNLOAD_PROGRESS` WebSocket events so the
//! web client can render a progress overlay with pause / resume / retry.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use std::time::Instant;

use serde_json::Value;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use crate::api::context::{AppCtx, WS_DOWNLOAD_PROGRESS, WS_MESSAGE_UPDATED, WsEvent};
use crate::api::enums::DownloadStatus;
use crate::chat::app_file_store::import_attachment;

/// Download task state. Mirrors plain-app `DownloadStatus`.
#[derive(Clone, Debug)]
pub struct DownloadState {
    pub message_id: String,
    pub status: DownloadStatus,
    pub downloaded: u64,
    pub total: u64,
    pub speed: u64,
}

struct TaskEntry {
    handle: JoinHandle<()>,
    abort: Option<tokio::sync::oneshot::Sender<()>>,
    state: Arc<Mutex<DownloadState>>,
}

/// Global download manager singleton.
pub struct DownloadManager {
    tasks: Mutex<HashMap<String, TaskEntry>>,
}

impl DownloadManager {
    fn new() -> Self {
        Self {
            tasks: Mutex::new(HashMap::new()),
        }
    }
}

fn manager() -> &'static DownloadManager {
    static MGR: OnceLock<DownloadManager> = OnceLock::new();
    MGR.get_or_init(DownloadManager::new)
}

/// reqwest client tolerant of self-signed peer certs (same as
/// `proxy_file.rs::proxy_client`).
fn http_client() -> reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .danger_accept_invalid_certs(true)
                .danger_accept_invalid_hostnames(true)
                .tcp_keepalive(std::time::Duration::from_secs(60))
                .pool_max_idle_per_host(20)
                .build()
                .expect("download reqwest client")
        })
        .clone()
}

fn emit_progress(ctx: &Arc<AppCtx>, state: &DownloadState) {
    let payload = serde_json::json!([{
        "id": state.message_id,
        "messageId": state.message_id,
        "downloaded": state.downloaded,
        "total": state.total,
        "speed": state.speed,
        "status": state.status,
    }])
    .to_string();
    let _ = ctx
        .event_tx
        .send(WsEvent::broadcast(WS_DOWNLOAD_PROGRESS, payload));
}

/// Start (or queue) a download for all `fsid:` files in the given message.
/// Returns `false` if a download is already in flight for that message.
pub async fn start_download(ctx: Arc<AppCtx>, message_id: String, peer_id: String) -> bool {
    let tasks = manager().tasks.lock().await;
    if tasks.contains_key(&message_id) {
        return false;
    }
    drop(tasks);

    let state = Arc::new(Mutex::new(DownloadState {
        message_id: message_id.clone(),
        status: DownloadStatus::Pending,
        downloaded: 0,
        total: 0,
        speed: 0,
    }));

    let (abort_tx, abort_rx) = tokio::sync::oneshot::channel::<()>();

    let ctx2 = ctx.clone();
    let mid = message_id.clone();
    let state2 = state.clone();
    let handle = tokio::spawn(async move {
        execute_download(ctx2, mid, peer_id, state2, abort_rx).await;
    });

    let mut tasks = manager().tasks.lock().await;
    tasks.insert(
        message_id,
        TaskEntry {
            handle,
            abort: Some(abort_tx),
            state,
        },
    );
    true
}

/// Pause an in-flight download. Returns `false` if no active task exists.
pub async fn pause_download(message_id: &str) -> bool {
    let mut tasks = manager().tasks.lock().await;
    if let Some(entry) = tasks.get_mut(message_id) {
        if let Some(abort) = entry.abort.take() {
            let _ = abort.send(());
        }
        let mut s = entry.state.lock().await;
        s.status = DownloadStatus::Paused;
        return true;
    }
    false
}

/// Resume a paused download. The caller must pass the peer id again since
/// the manager does not cache it.
pub async fn resume_download(ctx: Arc<AppCtx>, message_id: String, peer_id: String) -> bool {
    let mut tasks = manager().tasks.lock().await;
    if let Some(mut entry) = tasks.remove(&message_id) {
        if let Some(abort) = entry.abort.take() {
            let _ = abort.send(());
        }
        entry.handle.abort();
    }
    drop(tasks);
    start_download(ctx, message_id, peer_id).await
}

/// Retry a failed download.
pub async fn retry_download(ctx: Arc<AppCtx>, message_id: String, peer_id: String) -> bool {
    let mut tasks = manager().tasks.lock().await;
    if let Some(mut entry) = tasks.remove(&message_id) {
        if let Some(abort) = entry.abort.take() {
            let _ = abort.send(());
        }
        entry.handle.abort();
    }
    drop(tasks);
    start_download(ctx, message_id, peer_id).await
}

/// Core download loop — downloads every `fsid:` file in the message,
/// imports each into `app_file_store`, rewrites the content to `fid:`,
/// and emits progress + completion events.
async fn execute_download(
    ctx: Arc<AppCtx>,
    message_id: String,
    peer_id: String,
    state: Arc<Mutex<DownloadState>>,
    mut abort_rx: tokio::sync::oneshot::Receiver<()>,
) {
    let chat = match ctx.db.get_chat_by_id(&message_id) {
        Some(c) => c,
        None => {
            set_failed(&ctx, &state, "Message not found").await;
            cleanup(&message_id).await;
            return;
        }
    };

    let peer = match ctx.db.get_peer_by_id(&peer_id) {
        Some(p) => p,
        None => {
            set_failed(&ctx, &state, "Peer not found").await;
            cleanup(&message_id).await;
            return;
        }
    };

    let mut content: Value = match serde_json::from_str(&chat.content) {
        Ok(v) => v,
        Err(e) => {
            set_failed(&ctx, &state, &format!("Invalid content: {e}")).await;
            cleanup(&message_id).await;
            return;
        }
    };

    let items = match content
        .get_mut("value")
        .and_then(|v| v.get_mut("items"))
        .and_then(|i| i.as_array_mut())
    {
        Some(arr) => arr,
        None => {
            set_completed(&ctx, &state).await;
            cleanup(&message_id).await;
            return;
        }
    };

    // Collect (index, peer_file_id, file_name, file_size) for fsid: URIs.
    let mut to_download: Vec<(String, String, String, u64)> = Vec::new();
    for item in items.iter() {
        let uri = item.get("uri").and_then(|u| u.as_str()).unwrap_or("");
        if let Some(file_id) = uri.strip_prefix("fsid:") {
            let file_name = item
                .get("fileName")
                .and_then(|n| n.as_str())
                .unwrap_or("file")
                .to_string();
            let file_size = item.get("size").and_then(|s| s.as_u64()).unwrap_or(0);
            let attachment_id = item
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_owned();
            to_download.push((attachment_id, file_id.to_string(), file_name, file_size));
        }
    }

    if to_download.is_empty() {
        set_completed(&ctx, &state).await;
        cleanup(&message_id).await;
        return;
    }

    let total: u64 = to_download.iter().map(|(_, _, _, s)| s).sum();
    {
        let mut s = state.lock().await;
        s.status = DownloadStatus::Downloading;
        s.total = total;
        s.downloaded = 0;
        s.speed = 0;
        emit_progress(&ctx, &s);
    }

    let peer_ip = peer.ip.split(',').next().unwrap_or("").trim().to_string();
    if peer_ip.is_empty() {
        set_failed(&ctx, &state, "Peer has no IP").await;
        cleanup(&message_id).await;
        return;
    }
    let peer_port = peer.port;

    let mut downloaded_so_far: u64 = 0;
    let mut all_ok = true;

    for (attachment_id, file_id, file_name, file_size) in &to_download {
        if abort_rx.try_recv().is_ok() {
            let mut s = state.lock().await;
            s.status = DownloadStatus::Paused;
            emit_progress(&ctx, &s);
            cleanup(&message_id).await;
            return;
        }

        let fs_base = crate::utils::build_url::build_url("https", &peer_ip, peer_port, "/fs");
        let url = reqwest::Url::parse_with_params(&fs_base, &[("id", file_id.as_str())])
            .unwrap_or_else(|_| reqwest::Url::parse(&fs_base).expect("valid fs base url"));

        match download_one(
            &ctx,
            &state,
            url.as_str(),
            file_name,
            &message_id,
            attachment_id,
            &format!("fsid:{file_id}"),
            *file_size,
            &mut downloaded_so_far,
            &mut abort_rx,
        )
        .await
        {
            Ok(_) => {}
            Err(e) => {
                log::warn!("[download] failed to download {file_id}: {e}");
                all_ok = false;
                break;
            }
        }
    }

    if !all_ok {
        set_failed(&ctx, &state, "Download failed").await;
        cleanup(&message_id).await;
        return;
    }

    // Re-fetch the updated chat and emit the FULL item JSON (including the
    // `data` field with re-encrypted `fid:` file ids) so the web client can
    // resolve the new local URLs without a page refresh. Mirrors how
    // `chat_message.rs` emits `WS_MESSAGE_UPDATED` via `chat_to_json`.
    let updated_chat = match ctx.db.get_chat_by_id(&message_id) {
        Some(c) => c,
        None => {
            set_failed(&ctx, &state, "Message unavailable").await;
            cleanup(&message_id).await;
            return;
        }
    };
    let updated_payload =
        serde_json::json!([crate::chat::service::chat_to_json(&updated_chat)]).to_string();
    let _ = ctx
        .event_tx
        .send(WsEvent::broadcast(WS_MESSAGE_UPDATED, updated_payload));

    set_completed(&ctx, &state).await;
    cleanup(&message_id).await;
}

/// Download a single file from the peer, streaming to a temp file, then
/// import into `app_file_store`. Returns the `fid_suffix` (e.g.
/// `abc123.jpg`) on success.
async fn download_one(
    ctx: &Arc<AppCtx>,
    state: &Arc<Mutex<DownloadState>>,
    url: &str,
    file_name: &str,
    message_id: &str,
    attachment_id: &str,
    original_uri: &str,
    file_size: u64,
    downloaded_so_far: &mut u64,
    abort_rx: &mut tokio::sync::oneshot::Receiver<()>,
) -> Result<String, String> {
    let mut resp = http_client()
        .get(url)
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }

    let temp_dir = std::env::temp_dir();
    let temp_name = format!("plain_dl_{}", uuid::Uuid::new_v4());
    let temp_path: PathBuf = temp_dir.join(&temp_name);
    let _temporary = Temporary(temp_path.clone());

    let mut file = tokio::fs::File::create(&temp_path)
        .await
        .map_err(|e| format!("Failed to create temp file: {e}"))?;

    let mut last_progress = Instant::now();
    let mut bytes_since_last: u64 = 0;

    loop {
        if abort_rx.try_recv().is_ok() {
            drop(file);
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err("aborted".to_string());
        }

        match resp.chunk().await {
            Ok(Some(chunk)) => {
                file.write_all(&chunk)
                    .await
                    .map_err(|e| format!("Write failed: {e}"))?;
                let n = chunk.len() as u64;
                *downloaded_so_far += n;
                bytes_since_last += n;

                let now = Instant::now();
                if now.duration_since(last_progress) >= std::time::Duration::from_secs(1) {
                    let elapsed = now.duration_since(last_progress).as_secs_f64().max(0.001);
                    let speed = (bytes_since_last as f64 / elapsed) as u64;
                    {
                        let mut s = state.lock().await;
                        s.downloaded = *downloaded_so_far;
                        s.speed = speed;
                        emit_progress(ctx, &s);
                    }
                    last_progress = now;
                    bytes_since_last = 0;
                }
            }
            Ok(None) => break,
            Err(e) => {
                drop(file);
                let _ = tokio::fs::remove_file(&temp_path).await;
                return Err(format!("Stream error: {e}"));
            }
        }
    }

    file.flush()
        .await
        .map_err(|e| format!("Flush failed: {e}"))?;
    if file.metadata().await.map_err(|e| e.to_string())?.len() != file_size {
        return Err("Incomplete download".into());
    }
    drop(file);

    let import_result = {
        let db = ctx.db.clone();
        let data_dir = ctx.data_dir.clone();
        let temp_path = temp_path.clone();
        let file_name = file_name.to_string();
        let message_id = message_id.to_owned();
        let attachment_id = attachment_id.to_owned();
        let original_uri = original_uri.to_owned();
        tokio::task::spawn_blocking(move || {
            import_attachment(
                &db,
                &data_dir,
                &temp_path,
                &file_name,
                &message_id,
                &attachment_id,
                &original_uri,
            )
        })
        .await
        .map_err(|e| format!("Import task panicked: {e}"))?
        .map_err(|e| format!("Import failed: {e}"))?
    };

    let _ = tokio::fs::remove_file(&temp_path).await;
    if let Some(chat) = &import_result.chat {
        let payload = serde_json::json!([crate::chat::service::chat_to_json(chat)]).to_string();
        let _ = ctx
            .event_tx
            .send(WsEvent::broadcast(WS_MESSAGE_UPDATED, payload));
    }

    {
        let mut s = state.lock().await;
        s.downloaded = *downloaded_so_far;
        if file_size > 0 {
            s.total = s.total.max(*downloaded_so_far);
        }
    }

    Ok(import_result.fid_suffix)
}

async fn set_failed(ctx: &Arc<AppCtx>, state: &Arc<Mutex<DownloadState>>, error: &str) {
    log::warn!("[download] failed: {error}");
    let mut s = state.lock().await;
    s.status = DownloadStatus::Failed;
    s.speed = 0;
    emit_progress(ctx, &s);
}

async fn set_completed(ctx: &Arc<AppCtx>, state: &Arc<Mutex<DownloadState>>) {
    let mut s = state.lock().await;
    s.status = DownloadStatus::Completed;
    s.downloaded = s.total;
    s.speed = 0;
    emit_progress(ctx, &s);
}

async fn cleanup(message_id: &str) {
    // Wait a moment so the frontend can read the final progress event,
    // then remove the task entry so it can be retried if needed.
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    let mut tasks = manager().tasks.lock().await;
    tasks.remove(message_id);
}

struct Temporary(PathBuf);
impl Drop for Temporary {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
