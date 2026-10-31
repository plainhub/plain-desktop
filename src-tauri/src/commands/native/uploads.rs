use std::sync::Arc;

use plain_server::server::ContentServer;
pub use plain_server::native::uploads::UploadWriters;
use tauri::{Runtime, State, WebviewWindow};

use super::views::NativeViews;

pub const UPLOAD_HANDLE_HEADER: &str = "x-plain-upload-handle";

#[tauri::command]
pub async fn native_upload_chunk_open<R: Runtime>(
    core: State<'_, Arc<ContentServer>>,
    window: WebviewWindow<R>,
    views: State<'_, NativeViews>,
    writers: State<'_, UploadWriters>,
    file_id: String,
    index: i64,
    size: u64,
) -> Result<String, String> {
    let scope = views.scope(window.label());
    let uploads = core.native_uploads()?;
    writers.open(&uploads, &scope, &file_id, index, size).await
}

#[tauri::command]
pub async fn native_upload_chunk_write<R: Runtime>(
    window: WebviewWindow<R>,
    views: State<'_, NativeViews>,
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
    writers
        .write(&views.scope(window.label()), &handle, &bytes)
        .await
}

#[tauri::command]
pub async fn native_upload_chunk_finish<R: Runtime>(
    window: WebviewWindow<R>,
    views: State<'_, NativeViews>,
    writers: State<'_, UploadWriters>,
    handle: String,
) -> Result<u64, String> {
    writers.finish(&views.scope(window.label()), &handle).await
}

#[tauri::command]
pub fn native_upload_chunk_abort<R: Runtime>(
    window: WebviewWindow<R>,
    views: State<'_, NativeViews>,
    writers: State<'_, UploadWriters>,
    handle: String,
) {
    writers.abort(&views.scope(window.label()), &handle);
}

#[tauri::command]
pub fn native_upload_chunks_abort_owner<R: Runtime>(
    window: WebviewWindow<R>,
    views: State<'_, NativeViews>,
    writers: State<'_, UploadWriters>,
) {
    writers.abort_owner(views.scope(window.label()).id());
}
