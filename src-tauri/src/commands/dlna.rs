use plain_server::assembly::server::ContentServer;
use plain_server::native::dlna::DlnaSenders;
use std::sync::Arc;

#[tauri::command]
pub async fn dlna_state(
    core: tauri::State<'_, Arc<ContentServer>>,
) -> Result<serde_json::Value, String> {
    core.native_dlna()?.snapshot().await
}

#[tauri::command]
pub async fn dlna_set_enabled(
    core: tauri::State<'_, Arc<ContentServer>>,
    enabled: bool,
) -> Result<(), String> {
    core.native_dlna()?.set_enabled(enabled).await
}

#[tauri::command]
pub async fn dlna_accept_cast(
    core: tauri::State<'_, Arc<ContentServer>>,
    remember: bool,
) -> Result<(), String> {
    core.native_dlna()?.accept(remember).await
}

#[tauri::command]
pub async fn dlna_reject_cast(
    core: tauri::State<'_, Arc<ContentServer>>,
    remember: bool,
) -> Result<(), String> {
    core.native_dlna()?.reject(remember).await
}

#[tauri::command]
pub fn dlna_senders(core: tauri::State<'_, Arc<ContentServer>>) -> Result<DlnaSenders, String> {
    core.native_dlna()?.senders()
}

#[tauri::command]
pub async fn dlna_remove_sender(
    core: tauri::State<'_, Arc<ContentServer>>,
    kind: String,
    ip: String,
) -> Result<(), String> {
    core.native_dlna()?
        .remove_sender(kind == "allowed", ip)
        .await
}
