#![allow(non_snake_case)]

#[allow(unused_imports)]
pub(crate) use plain_rs::protocols::mdns::host_responder::get_best_ip as discover_get_best_ip;
pub(crate) use plain_rs::protocols::mdns::host_responder::local_ipv4_strs as discover_local_ipv4_strs;
use plain_server::assembly::enums::DeviceType;
use plain_server::assembly::server::ContentServer;
use plain_server::native::peers::{LoginPeer, RemoteLogin};
#[allow(unused_imports)]
pub use plain_server::services::discovery::local::MdnsActivity;
pub use plain_server::services::discovery::local::MdnsFirewallStatus;
pub use plain_server::services::discovery::local::firewall;
pub use plain_server::services::discovery::local::macos_dns_sd;
use std::sync::Arc;

// ── Remote-device login sessions (peers.token) ───────────────────────────────

/// Records a successful login with its session token and optional chat key.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn login_peer(
    core: tauri::State<'_, Arc<ContentServer>>,
    id: String,
    name: String,
    host: String,
    deviceType: DeviceType,
    token: String,
    signaturePublicKey: String,
    chatKey: Option<String>,
) -> Result<(), String> {
    let core = core.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        core.native_peers()?.login(RemoteLogin {
            id,
            name,
            host,
            device_type: deviceType.into(),
            token,
            signature_public_key: signaturePublicKey,
            chat_key: chatKey.unwrap_or_default(),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Clears the login token of a peer (logout / forget device).
#[tauri::command]
pub async fn logout_peer(
    core: tauri::State<'_, Arc<ContentServer>>,
    id: String,
) -> Result<(), String> {
    let core = core.inner().clone();
    tauri::async_runtime::spawn_blocking(move || core.native_peers()?.logout(&id))
        .await
        .map_err(|e| e.to_string())?
}

/// All peers with an active login token.
#[tauri::command]
pub async fn list_login_peers(
    core: tauri::State<'_, Arc<ContentServer>>,
) -> Result<Vec<LoginPeer>, String> {
    let core = core.inner().clone();
    tauri::async_runtime::spawn_blocking(move || core.native_peers()?.logged_in())
        .await
        .map_err(|e| e.to_string())?
}

/// Updates a peer's display name.
#[tauri::command]
pub async fn update_peer_name(
    core: tauri::State<'_, Arc<ContentServer>>,
    id: String,
    name: String,
) -> Result<(), String> {
    let core = core.inner().clone();
    tauri::async_runtime::spawn_blocking(move || core.native_peers()?.update_name(&id, &name))
        .await
        .map_err(|e| e.to_string())?
}

// ── mDNS debug surface — mirrors plain-app's MdnsDebugPage + WebAddressBar ───

/// Read-only snapshot of every currently-known `_plainapp._tcp.local`
/// instance (raw wire data as parsed) — mirrors `MdnsServiceBrowser.snapshot`.
#[tauri::command]
pub async fn mdns_snapshot(
    core: tauri::State<'_, Arc<ContentServer>>,
) -> Result<Vec<plain_rs::protocols::mdns::service_browser::MdnsServiceSnapshot>, String> {
    let core = core.inner().clone();
    core.native_discovery()?.snapshot().await
}

#[tauri::command]
pub async fn mdns_activity(
    core: tauri::State<'_, Arc<ContentServer>>,
) -> Result<Vec<MdnsActivity>, String> {
    let core = core.inner().clone();
    core.native_discovery()?.activity().await
}

#[tauri::command]
pub async fn mdns_browse(core: tauri::State<'_, Arc<ContentServer>>) -> Result<(), String> {
    core.native_discovery()?.browse().await
}

#[tauri::command]
pub async fn mdns_start_browse(core: tauri::State<'_, Arc<ContentServer>>) -> Result<bool, String> {
    core.native_discovery()?.start_browse().await
}

#[tauri::command]
pub async fn mdns_stop_browse(core: tauri::State<'_, Arc<ContentServer>>) -> Result<bool, String> {
    core.native_discovery()?.stop_browse().await
}

#[tauri::command]
pub async fn mdns_get_hostname(
    core: tauri::State<'_, Arc<ContentServer>>,
) -> Result<String, String> {
    core.native_discovery()?.hostname()
}

#[tauri::command]
pub async fn mdns_set_hostname(
    core: tauri::State<'_, Arc<ContentServer>>,
    hostname: String,
) -> Result<(), String> {
    core.native_discovery()?.set_hostname(&hostname).await
}

// ── Windows Firewall repair (LAN/mDNS discovery) ─────────────────────────────

/// Read-only firewall state for the current exe. No admin token required.
#[tauri::command]
pub async fn mdns_firewall_status() -> Result<MdnsFirewallStatus, String> {
    tauri::async_runtime::spawn_blocking(firewall::probe_status)
        .await
        .map_err(|e| e.to_string())?
}

/// Adds the inbound Allow rule via a UAC-elevated PowerShell script.
/// Returns Ok as soon as the elevated child is spawned — the frontend polls
/// `mdns_firewall_status` until the rule shows up.
#[tauri::command]
pub async fn fix_mdns_firewall() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(firewall::apply_fix)
        .await
        .map_err(|e| e.to_string())?
}
