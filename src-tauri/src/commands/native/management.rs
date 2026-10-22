use plain_server::{assembly::server::ContentServer, native::management::HttpServerState};
use std::sync::Arc;

#[tauri::command]
pub async fn native_confirm_web_login(
    core: tauri::State<'_, Arc<ContentServer>>,
    request_id: String,
    allowed: bool,
) -> Result<bool, String> {
    core.native_management()?.confirm_web_login(request_id, allowed).await
}

#[tauri::command]
pub async fn native_restart_http_server(
    core: tauri::State<'_, Arc<ContentServer>>,
    http_port: Option<u16>,
    https_port: Option<u16>,
) -> Result<HttpServerState, String> {
    let management = core.native_management()?;
    tokio::time::timeout(
        std::time::Duration::from_secs(30),
        management.restart_http_server(http_port, https_port),
    )
    .await
    .map_err(|_| "connection_timeout".to_owned())?
}

#[tauri::command]
pub fn native_device_identity(
    core: tauri::State<'_, Arc<ContentServer>>,
) -> Result<serde_json::Value, String> {
    let snapshot = serde_json::to_value(core.native_management()?.prefs_snapshot()?)
        .map_err(|error| error.to_string())?;
    Ok(serde_json::json!({
        "clientId": snapshot["clientId"],
        "signaturePublicKey": snapshot["signaturePublicKey"],
    }))
}
