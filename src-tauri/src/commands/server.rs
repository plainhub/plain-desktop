use plain_rs::http_proxy::HttpProxyState;
use plain_rs::server::runtime::ServerRuntime;

#[tauri::command]
pub fn http_proxy_port(state: tauri::State<'_, HttpProxyState>) -> u16 {
    state.port
}

#[tauri::command]
pub fn local_server_port(state: tauri::State<'_, ServerRuntime>) -> u16 {
    state.port()
}

#[tauri::command]
pub fn local_server_https_port(state: tauri::State<'_, ServerRuntime>) -> u16 {
    state.https_port()
}

#[tauri::command]
pub fn local_server_token(state: tauri::State<'_, ServerRuntime>) -> String {
    state.token().to_string()
}

#[tauri::command]
pub async fn local_ipv4_strs() -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(plain_rs::mdns::host_responder::local_ipv4_strs)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn set_http_port(handle: tauri::AppHandle, port: u16) -> Result<(), String> {
    use tauri::Manager;
    let prefs = handle
        .state::<std::sync::Arc<crate::prefs::Prefs>>()
        .inner()
        .clone();
    tauri::async_runtime::spawn_blocking(move || {
        plain_rs::prefs::server::set_http_port(&prefs, port)
    })
    .await
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn set_https_port(handle: tauri::AppHandle, port: u16) -> Result<(), String> {
    use tauri::Manager;
    let prefs = handle
        .state::<std::sync::Arc<crate::prefs::Prefs>>()
        .inner()
        .clone();
    tauri::async_runtime::spawn_blocking(move || {
        plain_rs::prefs::server::set_https_port(&prefs, port)
    })
    .await
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn restart_server(state: tauri::State<'_, ServerRuntime>) -> Result<(), String> {
    state.restart().await
}
