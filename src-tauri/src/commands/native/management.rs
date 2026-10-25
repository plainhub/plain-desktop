use plain_server::{assembly::server::ContentServer, native::management::HttpServerState};
use std::sync::Arc;

#[tauri::command]
pub async fn native_lan_ipv4_addresses(
    core: tauri::State<'_, Arc<ContentServer>>,
) -> Result<Vec<String>, String> {
    core.native_management()?.lan_ipv4_addresses().await
}

#[tauri::command]
pub async fn native_confirm_web_login(
    core: tauri::State<'_, Arc<ContentServer>>,
    request_id: String,
    allowed: bool,
) -> Result<bool, String> {
    core.native_management()?
        .confirm_web_login(request_id, allowed)
        .await
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
    let preferences = core.native_management()?.get_prefs(&["client_id".into(), "signature_public_key".into()])?;
    Ok(serde_json::json!({
        "clientId": preferences["client_id"],
        "signaturePublicKey": preferences["signature_public_key"],
    }))
}

#[tauri::command]
pub fn native_file_paths(
    core: tauri::State<'_, Arc<ContentServer>>,
    paths: Vec<String>,
) -> Result<Vec<Option<String>>, String> {
    core.native_management()?.file_paths(paths)
}

#[tauri::command]
pub async fn native_pairing_code(
    core: tauri::State<'_, Arc<ContentServer>>,
) -> Result<String, String> {
    core.native_management()?.pairing_code().await
}

#[tauri::command]
pub async fn native_logout_peer(
    core: tauri::State<'_, Arc<ContentServer>>,
    id: String,
) -> Result<bool, String> {
    core.native_management()?.logout_peer(id).await
}

#[tauri::command]
pub async fn native_dlna_receiver(
    core: tauri::State<'_, Arc<ContentServer>>,
) -> Result<serde_json::Value, String> {
    core.native_management()?.dlna_receiver().await
}

#[tauri::command]
pub async fn native_acquire_mdns_debug_session(
    core: tauri::State<'_, Arc<ContentServer>>,
    token: String,
) -> Result<serde_json::Value, String> {
    core.native_management()?
        .acquire_mdns_debug_session(token)
        .await
}

#[tauri::command]
pub async fn native_release_mdns_debug_session(
    core: tauri::State<'_, Arc<ContentServer>>,
    token: String,
) -> Result<serde_json::Value, String> {
    core.native_management()?
        .release_mdns_debug_session(token)
        .await
}
