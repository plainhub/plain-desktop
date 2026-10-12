//! Tauri commands for desktop native file resources.
//!
//! A window asks for a handle by path; the handle lives in that window's
//! own registry slot and is only reachable through the opaque-id
//! [`crate::native_resource_protocol`] URL scheme.

use std::path::PathBuf;

use tauri::{Runtime, State, WebviewWindow};

use crate::native_resource_protocol::{OpenedResource, ResourceRegistry};

/// Open `path` as a handle owned by the calling window and return the
/// protocol URL that reads it. The URL carries only the opaque id.
///
/// Only an absolute on-disk path is accepted. `fid:` and `app://`
/// references need the shared core's resolver, which the core does not
/// expose to the shell yet; those keep their existing transport.
#[tauri::command]
pub async fn native_resource_open<R: Runtime>(
    window: WebviewWindow<R>,
    registry: State<'_, ResourceRegistry>,
    path: String,
) -> Result<OpenedResource, String> {
    let path = PathBuf::from(path);
    if !path.is_absolute() {
        return Err("native resource requires an absolute path".into());
    }
    registry.open(window.label(), &path).await
}

#[tauri::command]
pub fn native_resource_close<R: Runtime>(
    window: WebviewWindow<R>,
    registry: State<'_, ResourceRegistry>,
    id: String,
) -> bool {
    registry.close(window.label(), &id)
}

/// Release every handle the window opened. Codex calls this from the
/// window's destroy event; the protocol refuses the handles immediately
/// either way.
#[tauri::command]
pub fn native_resource_close_owner<R: Runtime>(
    window: WebviewWindow<R>,
    registry: State<'_, ResourceRegistry>,
) {
    registry.close_owner(window.label());
}
