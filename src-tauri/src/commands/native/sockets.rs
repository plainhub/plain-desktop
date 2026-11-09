use super::{NativeTarget, views::NativeViews};
use plain_server::{
    native::sockets::{MAX_SOCKET_CONTROL_BYTES, SocketKind, SocketSession},
    server::ContentServer,
};
use std::sync::Arc;
use tauri::{State, WebviewWindow};

fn session(
    window: &WebviewWindow,
    views: &NativeViews,
    core: &ContentServer,
    scope: &str,
    handle: &str,
) -> Result<Arc<SocketSession>, String> {
    views.existing_scope(window.label(), scope)?;
    let session = core.native_sockets()?.session(handle)?;
    if session.scope_id() != scope {
        return Err("Socket view mismatch".into());
    }
    Ok(session)
}

#[tauri::command]
pub fn native_socket_open(
    window: WebviewWindow,
    views: State<'_, NativeViews>,
    core: State<'_, Arc<ContentServer>>,
    scope: String,
    target: NativeTarget,
    kind: SocketKind,
) -> Result<String, String> {
    let scope = views.existing_scope(window.label(), &scope)?;
    let peer = match target {
        NativeTarget::Local {} => None,
        NativeTarget::Peer { peer_id } => Some(peer_id),
    };
    core.native_sockets()?.open(kind, peer, scope)
}

#[tauri::command]
pub async fn native_socket_read(
    window: WebviewWindow,
    views: State<'_, NativeViews>,
    core: State<'_, Arc<ContentServer>>,
    scope: String,
    handle: String,
) -> Result<tauri::ipc::Response, String> {
    session(&window, &views, &core, &scope, &handle)?
        .read()
        .await
        .map(tauri::ipc::Response::new)
}

#[tauri::command]
pub async fn native_socket_send(
    window: WebviewWindow,
    views: State<'_, NativeViews>,
    core: State<'_, Arc<ContentServer>>,
    request: tauri::ipc::Request<'_>,
) -> Result<(), String> {
    let scope = request
        .headers()
        .get("x-plain-native-scope")
        .and_then(|v| v.to_str().ok())
        .ok_or("Native request scope required")?;
    let handle = request
        .headers()
        .get("x-plain-socket-handle")
        .and_then(|v| v.to_str().ok())
        .ok_or("Socket handle required")?;
    let session = session(&window, &views, &core, scope, handle)?;
    let bytes = match request.body() {
        tauri::ipc::InvokeBody::Raw(bytes)
            if !bytes.is_empty() && bytes.len() <= MAX_SOCKET_CONTROL_BYTES =>
        {
            bytes.clone()
        }
        _ => return Err("Invalid socket control bytes".into()),
    };
    session.send(bytes).await
}

#[tauri::command]
pub fn native_socket_close(
    window: WebviewWindow,
    views: State<'_, NativeViews>,
    core: State<'_, Arc<ContentServer>>,
    scope: String,
    handle: String,
) -> Result<(), String> {
    session(&window, &views, &core, &scope, &handle)?.close();
    Ok(())
}
