use plain_server::http::proxy::local::HttpProxyState;

#[tauri::command]
pub fn http_proxy_port(state: tauri::State<'_, HttpProxyState>) -> u16 {
    state.port
}
