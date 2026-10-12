use plain_server::assembly::server::ContentServer;
use plain_server::http::proxy::local::HttpProxyState;
use std::sync::Arc;

#[tauri::command]
pub fn http_proxy_port(state: tauri::State<'_, HttpProxyState>) -> u16 {
    state.port
}

#[tauri::command]
pub async fn local_server_port(core: tauri::State<'_, Arc<ContentServer>>) -> Result<u16, String> {
    Ok(core.http_ports().await.map(|ports| ports.0).unwrap_or(0))
}

#[tauri::command]
pub async fn local_server_https_port(
    core: tauri::State<'_, Arc<ContentServer>>,
) -> Result<u16, String> {
    Ok(core.http_ports().await.map(|ports| ports.1).unwrap_or(0))
}

#[tauri::command]
pub fn local_server_token(core: tauri::State<'_, Arc<ContentServer>>) -> Result<String, String> {
    core.native_url_token()
}

#[tauri::command]
pub async fn local_ipv4_strs() -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(plain_rs::protocols::mdns::host_responder::local_ipv4_strs)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn set_http_port(
    core: tauri::State<'_, Arc<ContentServer>>,
    port: u16,
) -> Result<(), String> {
    core.set_public_ports(Some(port), None).await.map(|_| ())
}

#[tauri::command]
pub async fn set_https_port(
    core: tauri::State<'_, Arc<ContentServer>>,
    port: u16,
) -> Result<(), String> {
    core.set_public_ports(None, Some(port)).await.map(|_| ())
}

#[tauri::command]
pub async fn restart_server(core: tauri::State<'_, Arc<ContentServer>>) -> Result<(), String> {
    core.restart_public().await.map(|_| ())
}
