//! Tauri commands for native upload chunk writing.
//!
//! The shared core owns the chunk directory; this bridge only receives
//! bytes. Opening a chunk yields an opaque handle owned by the calling
//! window; the frontend streams bounded writes through it and finishes the
//! chunk. Finishing a chunk is not finishing a file — the whole-file merge
//! stays in the main GraphQL `mergeChunks` / `mergeAppFileChunks`.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use plain_server::assembly::server::ContentServer;
use plain_server::native::uploads::NativeUploads;
use plain_server::services::files::upload_chunks::UploadChunk;
use tauri::{Runtime, State, WebviewWindow};

/// Handle id travels in a header because the write body is raw bytes.
pub const UPLOAD_HANDLE_HEADER: &str = "x-plain-upload-handle";

struct Slot {
    owner: String,
    writer: tokio::sync::Mutex<Option<UploadChunk>>,
    closed: AtomicBool,
}

/// Per-window chunk writers. Dropping an [`UploadWriters`] (or calling
/// `native_upload_chunk_abort`) removes the temporary file the shared
/// writer created and permanently closes the handle, so a cancelled chunk
/// can never be re-written from the same buffer.
#[derive(Default)]
pub struct UploadWriters {
    slots: std::sync::Mutex<HashMap<String, Arc<Slot>>>,
}

impl UploadWriters {
    fn insert(&self, owner: &str, writer: UploadChunk) -> String {
        let handle = next_handle();
        self.slots.lock().unwrap().insert(
            handle.clone(),
            Arc::new(Slot {
                owner: owner.to_string(),
                writer: tokio::sync::Mutex::new(Some(writer)),
                closed: AtomicBool::new(false),
            }),
        );
        handle
    }

    fn owned(&self, owner: &str, handle: &str) -> Result<Arc<Slot>, String> {
        let slot = self
            .slots
            .lock()
            .unwrap()
            .get(handle)
            .cloned()
            .ok_or("upload chunk unavailable")?;
        if slot.owner != owner {
            return Err("upload chunk unavailable".into());
        }
        Ok(slot)
    }

    fn take(&self, owner: &str, handle: &str) -> Result<UploadChunk, String> {
        let slot = self.owned(owner, handle)?;
        if slot.closed.load(Ordering::SeqCst) {
            return Err("upload chunk canceled".into());
        }
        let mut writer = slot.writer.lock().await;
        writer.take().ok_or("upload chunk unavailable".to_string())
    }

    fn discard(&self, owner: &str, handle: &str) {
        if let Ok(slot) = self.owned(owner, handle) {
            slot.closed.store(true, Ordering::SeqCst);
        }
        let removed = self.slots.lock().unwrap().remove(handle);
        drop(removed);
    }
}

fn next_handle() -> String {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    format!(
        "chunk-{}",
        SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    )
}

fn uploads(core: &ContentServer) -> Result<NativeUploads, String> {
    core.native_uploads()
}

/// Chunk entries already stored for `fileId`, as the shared core lists
/// them. Resuming an interrupted upload reads this before writing.
#[tauri::command]
pub async fn native_upload_chunks(
    core: State<'_, Arc<ContentServer>>,
    file_id: String,
) -> Result<Vec<String>, String> {
    uploads(&core)?.uploaded_chunks(&file_id).await
}

#[tauri::command]
pub async fn native_upload_chunk_open<R: Runtime>(
    core: State<'_, Arc<ContentServer>>,
    window: WebviewWindow<R>,
    writers: State<'_, UploadWriters>,
    file_id: String,
    index: i64,
    size: u64,
) -> Result<String, String> {
    let writer = uploads(&core)?.open_chunk(&file_id, index, size).await?;
    Ok(writers.insert(window.label(), writer))
}

#[tauri::command]
pub async fn native_upload_chunk_write<R: Runtime>(
    window: WebviewWindow<R>,
    writers: State<'_, UploadWriters>,
    request: tauri::ipc::Request<'_>,
) -> Result<(), String> {
    let handle = request
        .headers()
        .get(UPLOAD_HANDLE_HEADER)
        .and_then(|value| value.to_str().ok())
        .ok_or("upload chunk unavailable")?
        .to_string();
    let bytes = match request.body() {
        tauri::ipc::InvokeBody::Raw(bytes) => bytes.clone(),
        tauri::ipc::InvokeBody::Json(_) => return Err("upload chunk requires raw bytes".into()),
    };
    let slot = writers.owned(window.label(), &handle)?;
    if slot.closed.load(Ordering::SeqCst) {
        return Err("upload chunk canceled".into());
    }
    let mut writer = slot.writer.lock().await;
    let chunk = writer
        .as_mut()
        .ok_or("upload chunk unavailable".to_string())?;
    chunk.write(&bytes).await
}

#[tauri::command]
pub async fn native_upload_chunk_finish<R: Runtime>(
    window: WebviewWindow<R>,
    writers: State<'_, UploadWriters>,
    handle: String,
) -> Result<u64, String> {
    let writer = writers.take(window.label(), &handle)?;
    writers.discard(window.label(), &handle);
    writer.finish().await
}

/// Cancel a chunk in flight. The partial file is removed and the handle
/// stays closed.
#[tauri::command]
pub fn native_upload_chunk_abort<R: Runtime>(
    window: WebviewWindow<R>,
    writers: State<'_, UploadWriters>,
    handle: String,
) {
    writers.discard(window.label(), &handle);
}

/// Cancel every chunk a window still holds open.
#[tauri::command]
pub fn native_upload_chunks_abort_owner<R: Runtime>(
    window: WebviewWindow<R>,
    writers: State<'_, UploadWriters>,
) {
    let owner = window.label().to_string();
    let handles: Vec<String> = writers
        .slots
        .lock()
        .unwrap()
        .iter()
        .filter(|(_, slot)| slot.owner == owner)
        .map(|(handle, _)| handle.clone())
        .collect();
    for handle in handles {
        writers.discard(&owner, &handle);
    }
}
