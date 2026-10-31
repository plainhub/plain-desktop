//! Tauri commands for desktop native file resources.
//!
//! A window asks for a handle by reference; the handle lives in that
//! window's own registry slot and is only reachable through the opaque-id
//! [`crate::native_resource_protocol`] URL scheme.

use std::sync::Arc;

use plain_server::server::ContentServer;
use tauri::{Runtime, State, WebviewWindow};

use super::views::NativeViews;
use crate::native_resource_protocol::{OpenedResource, ResourceRegistry};

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
) -> Result<OpenedResource, String> {
    let scope = views.scope(window.label());
    let resolved = core.resolve_native_file(&path).await?;
    registry.open(&scope, &resolved).await
}

#[tauri::command]
pub fn native_resource_close<R: Runtime>(
    window: WebviewWindow<R>,
    registry: State<'_, Arc<ResourceRegistry>>,
    views: State<'_, NativeViews>,
    id: String,
) -> bool {
    registry.close(&views.scope(window.label()), &id)
}

#[tauri::command]
pub fn native_resource_close_owner<R: Runtime>(
    window: WebviewWindow<R>,
    registry: State<'_, Arc<ResourceRegistry>>,
    views: State<'_, NativeViews>,
) {
    registry.close_owner(views.scope(window.label()).id());
}
