use crate::db::{DAppFile, Db};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

pub struct FileStore {
    pub db: Arc<Db>,
    pub directory: PathBuf,
}
impl FileStore {
    pub fn new(db: Arc<Db>, directory: PathBuf) -> Arc<Self> {
        Arc::new(Self { db, directory })
    }
    pub async fn import(
        self: &Arc<Self>,
        source: PathBuf,
        name: String,
        mime: String,
        delete_source: bool,
    ) -> Result<DAppFile, String> {
        let store = self.clone();
        tokio::task::spawn_blocking(move || {
            let result = crate::chat::app_file_store::import_file(
                &store.db,
                &store.directory,
                &source,
                &name,
                &mime,
            )
            .map_err(|error| error.to_string())?;
            let outcome = (|| {
                if delete_source
                    && std::fs::canonicalize(&source).map_err(|e| e.to_string())?
                        != std::fs::canonicalize(&result.real_path).map_err(|e| e.to_string())?
                {
                    std::fs::remove_file(&source).map_err(|e| e.to_string())?;
                }
                store
                    .db
                    .app_file_get(&result.id)
                    .map_err(|e| e.to_string())?
                    .ok_or_else(|| "imported file missing".to_string())
            })();
            if outcome.is_err() {
                crate::chat::app_file_store::release(&store.db, &store.directory, &result.id)
                    .map_err(|e| e.to_string())?;
            }
            outcome
        })
        .await
        .map_err(|error| error.to_string())?
    }
    pub async fn release(self: &Arc<Self>, id: String) -> Result<bool, String> {
        let store = self.clone();
        tokio::task::spawn_blocking(move || {
            crate::chat::app_file_store::release(&store.db, &store.directory, &id)
                .map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| e.to_string())?
    }
    pub fn resolve(&self, suffix: &str) -> Result<PathBuf, String> {
        let id = suffix.split('.').next().unwrap_or_default();
        if id.len() != 64 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("invalid file id".into());
        }
        let record = self
            .db
            .app_file_get(id)
            .map_err(|e| e.to_string())?
            .ok_or("file not found")?;
        if Path::new(&record.real_path)
            .file_name()
            .and_then(|s| s.to_str())
            != Some(suffix)
        {
            return Err("file suffix does not match stored file".into());
        }
        let path = self.directory.join(&record.real_path);
        let root = std::fs::canonicalize(&self.directory).map_err(|e| e.to_string())?;
        let actual = std::fs::canonicalize(&path).map_err(|e| e.to_string())?;
        if !actual.starts_with(root.join("files")) {
            return Err("file outside store".into());
        }
        Ok(path)
    }
}
