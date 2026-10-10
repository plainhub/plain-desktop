use anyhow::Result;
use std::path::{Path, PathBuf};
pub(super) struct FileWalker {
    next: Option<PathBuf>,
    directories: Vec<tokio::fs::ReadDir>,
}
impl FileWalker {
    pub fn new(root: &Path) -> Self {
        Self {
            next: Some(root.to_owned()),
            directories: Vec::new(),
        }
    }
    pub async fn next(&mut self) -> Result<Option<PathBuf>> {
        loop {
            let path = if let Some(path) = self.next.take() {
                path
            } else {
                loop {
                    let Some(directory) = self.directories.last_mut() else {
                        return Ok(None);
                    };
                    if let Some(entry) = directory.next_entry().await? {
                        break entry.path();
                    }
                    self.directories.pop();
                }
            };
            let info = tokio::fs::symlink_metadata(&path).await?;
            if info.is_dir() {
                self.directories.push(tokio::fs::read_dir(&path).await?);
            } else if info.is_file() {
                return Ok(Some(path));
            }
        }
    }
}
