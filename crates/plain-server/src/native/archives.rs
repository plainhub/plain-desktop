use crate::services::files::archives::{ArchiveReader, Archives};
use std::{
    path::{Component, Path, PathBuf},
    sync::Arc,
};
use tokio::sync::watch;

#[derive(serde::Deserialize)]
pub struct ArchiveEntry {
    pub path: PathBuf,
    pub name: String,
}

#[derive(Clone)]
pub struct NativeArchives {
    archives: Arc<Archives>,
    host: Arc<crate::host::bridge::Host>,
    stop: watch::Receiver<bool>,
}

impl NativeArchives {
    pub(crate) fn new(
        archives: Arc<Archives>,
        host: Arc<crate::host::bridge::Host>,
        stop: watch::Receiver<bool>,
    ) -> Self {
        Self {
            archives,
            host,
            stop,
        }
    }

    pub async fn files(&self, entries: Vec<ArchiveEntry>) -> Result<ArchiveReader, String> {
        if *self.stop.borrow() || self.stop.has_changed().is_err() {
            return Err("Rust core stopped".into());
        }
        let permit = self.archives.acquire().ok_or("archive busy")?;
        let mut items = Vec::with_capacity(entries.len());
        for entry in entries {
            if !entry.path.is_absolute()
                || entry.name.is_empty()
                || entry.name.contains(['\\', '\0'])
                || Path::new(&entry.name)
                    .components()
                    .any(|component| !matches!(component, Component::Normal(_)))
            {
                return Err("invalid archive entry".into());
            }
            crate::services::files::file_access::authorize_read(
                &self.host,
                entry.path.to_str().ok_or("invalid file path encoding")?,
            )
            .await?;
            items.push((entry.path, entry.name));
        }
        self.archives.build(items, permit, self.stop.clone()).await
    }
}
