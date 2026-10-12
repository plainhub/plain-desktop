use crate::host::bridge::Host;
use crate::services::files::file_tasks::FileTasks;
use plain_rs::files::filesystem::record::FileRecord;
use plain_rs::files::filesystem::writes::Operation;
use plain_rs::files::tasks::{FileTask, FileTaskOp, FileTaskType};
use std::sync::Arc;
use tokio::sync::watch;

#[derive(Clone)]
pub struct NativeFileTasks {
    tasks: Arc<FileTasks>,
    host: Arc<Host>,
    client_id: String,
    stop: watch::Receiver<bool>,
}

impl NativeFileTasks {
    pub(crate) fn new(
        tasks: Arc<FileTasks>,
        host: Arc<Host>,
        client_id: String,
        stop: watch::Receiver<bool>,
    ) -> Self {
        Self {
            tasks,
            host,
            client_id,
            stop,
        }
    }

    fn ensure_running(&self) -> Result<(), String> {
        if *self.stop.borrow() || self.stop.has_changed().is_err() {
            return Err("Rust core stopped".into());
        }
        Ok(())
    }

    pub async fn create_directory(&self, path: String) -> Result<FileRecord, String> {
        self.ensure_running()?;
        crate::services::files::writes::execute(&self.host, Operation::CreateDirectory { path })
            .await
    }

    pub async fn create_file(&self, path: String) -> Result<FileRecord, String> {
        self.ensure_running()?;
        crate::services::files::writes::execute(&self.host, Operation::CreateFile { path }).await
    }

    pub async fn write_text(
        &self,
        path: String,
        content: String,
        overwrite: bool,
    ) -> Result<FileRecord, String> {
        self.ensure_running()?;
        crate::services::files::writes::execute(
            &self.host,
            Operation::WriteText {
                path,
                content,
                overwrite,
            },
        )
        .await
    }

    pub async fn copy(&self, title: String, ops: Vec<FileTaskOp>) -> Result<FileTask, String> {
        self.create(FileTaskType::Copy, title, ops).await
    }

    pub async fn move_files(
        &self,
        title: String,
        ops: Vec<FileTaskOp>,
    ) -> Result<FileTask, String> {
        self.create(FileTaskType::Move, title, ops).await
    }

    async fn create(
        &self,
        kind: FileTaskType,
        title: String,
        ops: Vec<FileTaskOp>,
    ) -> Result<FileTask, String> {
        self.ensure_running()?;
        self.tasks
            .create(&self.client_id, kind, &title, ops)
            .await
            .map_err(|error| error.to_string())
    }

    pub async fn list(&self) -> Result<Vec<FileTask>, String> {
        self.ensure_running()?;
        self.tasks
            .list(self.client_id.clone())
            .await
            .map_err(|error| error.to_string())
    }

    pub async fn recover(&self, id: String) -> Result<Option<FileTask>, String> {
        self.ensure_running()?;
        self.tasks
            .recover(self.client_id.clone(), id)
            .await
            .map_err(|error| error.to_string())
    }

    pub async fn remove(&self, id: String) -> Result<bool, String> {
        self.ensure_running()?;
        self.tasks
            .remove(self.client_id.clone(), id)
            .await
            .map_err(|error| error.to_string())
    }
}
