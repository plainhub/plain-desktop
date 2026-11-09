use std::sync::Arc;

pub use plain_server::native::uploads::UploadWriters;
use plain_server::server::ContentServer;
use tauri::{Runtime, State, WebviewWindow};

use super::{NativeTarget, views::NativeViews};

pub const UPLOAD_HANDLE_HEADER: &str = "x-plain-upload-handle";
pub const NATIVE_SCOPE_HEADER: &str = "x-plain-native-scope";

#[tauri::command]
pub async fn native_upload_chunk_open<R: Runtime>(
    core: State<'_, Arc<ContentServer>>,
    window: WebviewWindow<R>,
    views: State<'_, NativeViews>,
    writers: State<'_, UploadWriters>,
    target: NativeTarget,
    file_id: String,
    index: i64,
    size: u64,
    scope: String,
) -> Result<String, String> {
    let scope = views.existing_scope(window.label(), &scope)?;
    match target {
        NativeTarget::Local {} => {
            writers
                .open(&core.native_uploads()?, &scope, &file_id, index, size)
                .await
        }
        NativeTarget::Peer { peer_id } => {
            writers
                .open_peer(
                    &core.native_uploads()?,
                    &scope,
                    &peer_id,
                    &file_id,
                    index,
                    size,
                )
                .await
        }
    }
}

#[tauri::command]
pub async fn native_upload_chunk_write<R: Runtime>(
    window: WebviewWindow<R>,
    views: State<'_, NativeViews>,
    writers: State<'_, UploadWriters>,
    request: tauri::ipc::Request<'_>,
) -> Result<(), String> {
    let scope = request
        .headers()
        .get(NATIVE_SCOPE_HEADER)
        .and_then(|value| value.to_str().ok())
        .ok_or("Native request scope required")?;
    let scope = views.existing_scope(window.label(), scope)?;
    let handle = request
        .headers()
        .get(UPLOAD_HANDLE_HEADER)
        .and_then(|value| value.to_str().ok())
        .ok_or("upload chunk unavailable")?
        .to_string();
    let bytes = match request.body() {
        tauri::ipc::InvokeBody::Raw(bytes) => {
            if bytes.is_empty()
                || bytes.len() > plain_server::native::uploads::MAX_UPLOAD_WRITE_BYTES
            {
                return Err("invalid upload write length".into());
            }
            bytes.clone()
        }
        tauri::ipc::InvokeBody::Json(_) => return Err("upload chunk requires raw bytes".into()),
    };
    writers.write(&scope, &handle, &bytes).await
}

#[tauri::command]
pub async fn native_upload_chunk_finish<R: Runtime>(
    window: WebviewWindow<R>,
    views: State<'_, NativeViews>,
    writers: State<'_, UploadWriters>,
    handle: String,
    scope: String,
) -> Result<u64, String> {
    writers
        .finish(&views.existing_scope(window.label(), &scope)?, &handle)
        .await
}

#[tauri::command]
pub fn native_upload_chunk_abort<R: Runtime>(
    window: WebviewWindow<R>,
    views: State<'_, NativeViews>,
    writers: State<'_, UploadWriters>,
    handle: String,
    scope: String,
) -> Result<(), String> {
    writers.abort(&views.existing_scope(window.label(), &scope)?, &handle);
    Ok(())
}

#[tauri::command]
pub fn native_upload_chunks_abort_owner<R: Runtime>(
    window: WebviewWindow<R>,
    views: State<'_, NativeViews>,
    writers: State<'_, UploadWriters>,
    scope: String,
) -> Result<(), String> {
    writers.abort_owner(views.existing_scope(window.label(), &scope)?.id());
    Ok(())
}
