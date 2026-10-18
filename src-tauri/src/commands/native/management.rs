use plain_server::{assembly::server::ContentServer, native::management::HttpServerState};
use std::sync::Arc;

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
pub fn native_system_settings(
    core: tauri::State<'_, Arc<ContentServer>>,
) -> Result<plain_server::native::management::NativeSystemSettings, String> {
    core.native_management()?.system_settings()
}

#[tauri::command]
pub fn native_system_preferences(
    core: tauri::State<'_, Arc<ContentServer>>,
) -> Result<serde_json::Value, String> {
    core.native_management()?.system_preferences()
}
