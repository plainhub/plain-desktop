//! Tauri commands for desktop native file resources.
//!
//! A window asks for a handle by reference; the handle lives in that
//! window's own registry slot and is only reachable through the opaque-id
//! [`crate::native_resource_protocol`] URL scheme.

use std::sync::Arc;

use plain_server::server::ContentServer;
use tauri::{Runtime, State, WebviewWindow};

use super::{NativeTarget, views::NativeViews};
use crate::native_resource_protocol::{OpenedResource, ResourceRegistry};

#[tauri::command]
pub async fn native_file_preview(
    core: State<'_, Arc<ContentServer>>,
    id: String,
) -> Result<plain_server::native::resources::FilePreview, String> {
    core.preview_native_file(id).await
}

/// Open `path` as a handle owned by the calling window and return the
/// protocol URL that reads it. The URL carries only the opaque id.
///
/// `path` is a reference the shared core understands: a plaintext absolute
/// path, a `fid:` file id or an `app://` path. Resolution and authorization
/// belong to the core, which also refuses references once it has stopped.
#[tauri::command]
pub async fn native_resource_open<R: Runtime>(
    window: WebviewWindow<R>,
    core: tauri::State<'_, Arc<ContentServer>>,
    registry: State<'_, Arc<ResourceRegistry>>,
    views: State<'_, NativeViews>,
    path: String,
    scope: String,
    target: NativeTarget,
    thumbnail: Option<plain_rs::features::services::files::content::Size>,
) -> Result<OpenedResource, String> {
    let scope = views.existing_scope(window.label(), &scope)?;
    let resources = core.native_resources()?;
    match target {
        NativeTarget::Local {} => {
            registry
                .open_reference(&resources, &scope, &path, thumbnail)
                .await
        }
        NativeTarget::Peer { peer_id } => {
            registry
                .open_peer_reference(&resources, &scope, &peer_id, &path, thumbnail)
                .await
        }
    }
}

#[tauri::command]
pub fn native_resource_close<R: Runtime>(
    window: WebviewWindow<R>,
    registry: State<'_, Arc<ResourceRegistry>>,
    views: State<'_, NativeViews>,
    id: String,
    scope: String,
) -> Result<bool, String> {
    Ok(registry.close(&views.existing_scope(window.label(), &scope)?, &id))
}

#[tauri::command]
pub fn native_resource_close_owner<R: Runtime>(
    window: WebviewWindow<R>,
    registry: State<'_, Arc<ResourceRegistry>>,
    views: State<'_, NativeViews>,
    scope: String,
) -> Result<(), String> {
    registry.close_owner(views.existing_scope(window.label(), &scope)?.id());
    Ok(())
}

#[tauri::command]
pub async fn native_resource_read<R: Runtime>(
    window: WebviewWindow<R>,
    registry: State<'_, Arc<ResourceRegistry>>,
    views: State<'_, NativeViews>,
    id: String,
    scope: String,
    offset: u64,
    length: u64,
) -> Result<tauri::ipc::Response, String> {
    let scope = views.existing_scope(window.label(), &scope)?;
    registry
        .read(&scope, &id, offset, length)
        .await
        .map(tauri::ipc::Response::new)
}

#[tauri::command]
pub async fn native_resource_download<R: Runtime>(
    window: WebviewWindow<R>,
    core: State<'_, Arc<ContentServer>>,
    views: State<'_, NativeViews>,
    target: NativeTarget,
    path: String,
    scope: String,
    directory: String,
    name: String,
) -> Result<String, String> {
    let scope = views.existing_scope(window.label(), &scope)?;
    let peer_id = match &target {
        NativeTarget::Local {} => None,
        NativeTarget::Peer { peer_id } => Some(peer_id.as_str()),
    };
    core.native_resources()?
        .export(
            &scope,
            peer_id,
            &path,
            plain_server::native::resources::ResourceDestination::Directory { directory, name },
        )
        .await
}

#[tauri::command]
pub async fn native_resource_save_as<R: Runtime>(
    window: WebviewWindow<R>,
    core: State<'_, Arc<ContentServer>>,
    views: State<'_, NativeViews>,
    target: NativeTarget,
    path: String,
    scope: String,
    name: String,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let scope = views.existing_scope(window.label(), &scope)?;
    let (sender, receiver) = tokio::sync::oneshot::channel();
    window
        .dialog()
        .file()
        .set_file_name(name)
        .save_file(move |path| {
            let _ = sender.send(path);
        });
    let selected = tokio::select! {
        biased;
        _ = scope.wait_closed() => return Err("resource owner closed".into()),
        result = receiver => result.map_err(|error| error.to_string())?,
    };
    let Some(destination) = selected else {
        return Ok(None);
    };
    let destination = destination.into_path().map_err(|error| error.to_string())?;
    let peer_id = match &target {
        NativeTarget::Local {} => None,
        NativeTarget::Peer { peer_id } => Some(peer_id.as_str()),
    };
    core.native_resources()?
        .export(
            &scope,
            peer_id,
            &path,
            plain_server::native::resources::ResourceDestination::File(destination),
        )
        .await
        .map(Some)
}
