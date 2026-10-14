use std::sync::Arc;

use plain_rs::files::filesystem::record::FileRecord;
use plain_rs::files::tasks::{FileTask, FileTaskOp};
use plain_server::assembly::server::ContentServer;
use plain_server::native::file_tasks::NativeFileTasks;
use tauri::State;

fn tasks(core: &ContentServer) -> Result<NativeFileTasks, String> {
    core.native_file_tasks()
}

#[tauri::command]
pub async fn native_file_create_directory(
    core: State<'_, Arc<ContentServer>>,
    path: String,
) -> Result<FileRecord, String> {
    tasks(&core)?.create_directory(path).await
}

#[tauri::command]
pub async fn native_file_create_file(
    core: State<'_, Arc<ContentServer>>,
    path: String,
) -> Result<FileRecord, String> {
    tasks(&core)?.create_file(path).await
}

#[tauri::command]
pub async fn native_file_write_text(
    core: State<'_, Arc<ContentServer>>,
    path: String,
    content: String,
    overwrite: bool,
) -> Result<FileRecord, String> {
    tasks(&core)?.write_text(path, content, overwrite).await
}

#[tauri::command]
pub async fn native_file_copy(
    core: State<'_, Arc<ContentServer>>,
    title: String,
    ops: Vec<FileTaskOp>,
) -> Result<FileTask, String> {
    tasks(&core)?.copy(title, ops).await
}

#[tauri::command]
pub async fn native_file_move(
    core: State<'_, Arc<ContentServer>>,
    title: String,
    ops: Vec<FileTaskOp>,
) -> Result<FileTask, String> {
    tasks(&core)?.move_files(title, ops).await
}

#[tauri::command]
pub async fn native_file_tasks_list(
    core: State<'_, Arc<ContentServer>>,
) -> Result<Vec<FileTask>, String> {
    tasks(&core)?.list().await
}

#[tauri::command]
pub async fn native_file_task_remove(
    core: State<'_, Arc<ContentServer>>,
    id: String,
) -> Result<bool, String> {
    tasks(&core)?.remove(id).await
}
