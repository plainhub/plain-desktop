use crate::services::files::upload_chunks::{UploadChunk, UploadChunks};
use std::sync::Arc;
use tokio::sync::watch;

#[derive(Clone)]
pub struct NativeUploads {
    chunks: Arc<UploadChunks>,
    stop: watch::Receiver<bool>,
}

impl NativeUploads {
    pub(crate) fn new(chunks: Arc<UploadChunks>, stop: watch::Receiver<bool>) -> Self {
        Self { chunks, stop }
    }

    pub async fn uploaded_chunks(&self, id: &str) -> Result<Vec<String>, String> {
        ensure_running(&self.stop)?;
        self.chunks.list(id).await
    }

    pub async fn open_chunk(&self, id: &str, index: i64, size: u64) -> Result<UploadChunk, String> {
        ensure_running(&self.stop)?;
        self.chunks
            .open_chunk(id, index, size, self.stop.clone())
            .await
    }
}

fn ensure_running(stop: &watch::Receiver<bool>) -> Result<(), String> {
    if *stop.borrow() || stop.has_changed().is_err() {
        return Err("Rust core stopped".into());
    }
    Ok(())
}
