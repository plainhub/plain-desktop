use std::collections::HashMap;
use std::sync::Arc;

use plain_server::assembly::server::ContentServer;
use plain_server::native::archives::{ArchiveEntry, NativeArchives};
use plain_server::services::files::archives::ArchiveReader;
use serde::{Deserialize, Serialize};
use tauri::{Runtime, State, WebviewWindow};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveFile {
    pub path: String,
    pub name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenedArchive {
    pub handle: String,
    pub size: u64,
}

struct Slot {
    owner: String,
    reader: tokio::sync::Mutex<Option<ArchiveReader>>,
}

#[derive(Default)]
pub struct ArchiveReaders {
    slots: std::sync::Mutex<HashMap<String, Arc<Slot>>>,
    next: std::sync::atomic::AtomicU64,
}

impl ArchiveReaders {
    fn insert(&self, owner: &str, reader: ArchiveReader) -> Result<OpenedArchive, String> {
        let handle = format!(
            "archive-{}",
            self.next.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        );
        let size = reader.size;
        self.slots.lock().unwrap().insert(
            handle.clone(),
            Arc::new(Slot {
                owner: owner.to_string(),
                reader: tokio::sync::Mutex::new(Some(reader)),
            }),
        );
        Ok(OpenedArchive { handle, size })
    }

    fn owned(&self, owner: &str, handle: &str) -> Result<Arc<Slot>, String> {
        let slot = self
            .slots
            .lock()
            .unwrap()
            .get(handle)
            .cloned()
            .ok_or("archive unavailable")?;
        if slot.owner != owner {
            return Err("archive unavailable".into());
        }
        Ok(slot)
    }

    /// Drop the reader and its staged file. The map entry is only removed
    /// once the owner checks out, so one window cannot cancel another's
    /// reader. An in-flight read keeps the slot alive and releases it when
    /// it finishes.
    fn release(&self, owner: &str, handle: &str) {
        if self.owned(owner, handle).is_err() {
            return;
        }
        let slot = self.slots.lock().unwrap().remove(handle);
        if let Some(slot) = slot
            && let Some(reader) = slot
                .reader
                .try_lock()
                .ok()
                .and_then(|mut guard| guard.take())
        {
            drop(reader);
        }
    }

    fn release_owner(&self, owner: &str) {
        let handles: Vec<String> = self
            .slots
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, slot)| slot.owner == owner)
            .map(|(handle, _)| handle.clone())
            .collect();
        for handle in handles {
            self.release(owner, &handle);
        }
    }
}

fn archives(core: &ContentServer) -> Result<NativeArchives, String> {
    core.native_archives()
}

/// Build an archive from an authorized list of real files. Returns an
/// opaque handle; the bytes are read through
/// `native_archive_read` in bounded chunks.
#[tauri::command]
pub async fn native_archive_open<R: Runtime>(
    core: State<'_, Arc<ContentServer>>,
    window: WebviewWindow<R>,
    readers: State<'_, ArchiveReaders>,
    files: Vec<ArchiveFile>,
) -> Result<OpenedArchive, String> {
    let entries = files
        .into_iter()
        .map(|file| ArchiveEntry {
            path: file.path.into(),
            name: file.name,
        })
        .collect();
    let reader = archives(&core)?.files(entries).await?;
    readers.insert(window.label(), reader)
}

/// Read the next bounded chunk. `None` marks EOF; the handle is released
/// as soon as the reader is exhausted.
#[tauri::command]
pub async fn native_archive_read<R: Runtime>(
    window: WebviewWindow<R>,
    readers: State<'_, ArchiveReaders>,
    handle: String,
) -> Result<Option<Vec<u8>>, String> {
    let slot = readers.owned(window.label(), &handle)?;
    let mut guard = slot.reader.lock().await;
    let reader = guard.as_mut().ok_or("archive unavailable")?;
    let chunk = reader.read().await?;
    if chunk.is_none() {
        *guard = None;
    }
    Ok(chunk)
}

/// Release a reader without reading it to the end (cancel, error, or the
/// window going away).
#[tauri::command]
pub fn native_archive_close<R: Runtime>(
    window: WebviewWindow<R>,
    readers: State<'_, ArchiveReaders>,
    handle: String,
) {
    readers.release(window.label(), &handle);
}

#[tauri::command]
pub fn native_archive_close_owner<R: Runtime>(
    window: WebviewWindow<R>,
    readers: State<'_, ArchiveReaders>,
) {
    readers.release_owner(window.label());
}
