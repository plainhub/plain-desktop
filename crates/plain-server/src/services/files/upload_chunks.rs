use std::path::{Path, PathBuf};
use tokio::{fs::File, io::AsyncWriteExt, sync::watch};

pub const MAX_UPLOAD_WRITE_BYTES: usize = 64 * 1024;

pub struct UploadChunks {
    directory: PathBuf,
}

impl UploadChunks {
    pub(crate) fn new(directory: PathBuf) -> Self {
        Self { directory }
    }

    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub async fn list(&self, id: &str) -> Result<Vec<String>, String> {
        plain_rs::files::uploads::list(&self.directory, id)
            .await
            .map_err(|error| error.to_string())
    }
    pub async fn open_chunk(
        &self,
        id: &str,
        index: i64,
        size: u64,
        stop: watch::Receiver<bool>,
    ) -> Result<UploadChunk, String> {
        ensure_running(&stop)?;
        let destination = plain_rs::files::uploads::chunk_path(self.directory(), id, index)
            .map_err(|error| error.to_string())?;
        let parent = destination.parent().ok_or("upload directory unavailable")?;
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|error| error.to_string())?;
        let temporary = parent.join(format!(".upload_chunk_{}", uuid::Uuid::new_v4()));
        let file = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .await
            .map_err(|error| error.to_string())?;
        let chunk = UploadChunk {
            file: Some(file),
            temporary,
            destination,
            size,
            written: 0,
            failed: false,
            stop,
        };
        ensure_running(&chunk.stop)?;
        Ok(chunk)
    }
}

pub struct UploadChunk {
    file: Option<File>,
    temporary: PathBuf,
    destination: PathBuf,
    size: u64,
    written: u64,
    failed: bool,
    stop: watch::Receiver<bool>,
}

impl UploadChunk {
    pub async fn write(&mut self, bytes: &[u8]) -> Result<(), String> {
        ensure_running(&self.stop)?;
        if self.failed {
            return Err("upload chunk failed".into());
        }
        if bytes.is_empty() || bytes.len() > MAX_UPLOAD_WRITE_BYTES {
            return Err("invalid upload write length".into());
        }
        let total = self
            .written
            .checked_add(bytes.len() as u64)
            .ok_or("upload size overflow")?;
        if total > self.size {
            return Err("upload exceeds declared chunk size".into());
        }
        let file = self.file.as_mut().ok_or("upload chunk closed")?;
        self.failed = true;
        if let Err(error) = file.write_all(bytes).await {
            return Err(error.to_string());
        }
        self.failed = false;
        self.written = total;
        Ok(())
    }

    pub async fn finish(mut self) -> Result<u64, String> {
        ensure_running(&self.stop)?;
        if self.failed {
            return Err("upload chunk failed".into());
        }
        if self.written != self.size {
            return Err("upload chunk size mismatch".into());
        }
        let mut file = self.file.take().ok_or("upload chunk closed")?;
        file.flush().await.map_err(|error| error.to_string())?;
        drop(file);
        ensure_running(&self.stop)?;
        tokio::fs::rename(&self.temporary, &self.destination)
            .await
            .map_err(|error| error.to_string())?;
        Ok(self.written)
    }
}

impl Drop for UploadChunk {
    fn drop(&mut self) {
        self.file.take();
        let _ = std::fs::remove_file(&self.temporary);
    }
}

fn ensure_running(stop: &watch::Receiver<bool>) -> Result<(), String> {
    if *stop.borrow() || stop.has_changed().is_err() {
        return Err("Rust core stopped".into());
    }
    Ok(())
}
