#![allow(non_snake_case)]

#[path = "NearbyDiscoverManager.rs"]
mod nearby_discover_manager;
#[path = "PeerStatusManager.rs"]
mod peer_status_manager;
#[path = "firewall.rs"]
pub(crate) mod mdns_firewall;

pub use nearby_discover_manager::NearbyDiscoverManager;
pub use peer_status_manager::PeerStatusManager;
pub use mdns_firewall::MdnsFirewallStatus;
use crate::local::enums::DeviceType;
use crate::local::graphql::schema::types::Peer;
pub(crate) use plain_rs::mdns::host_responder::get_best_ip as discover_get_best_ip;
pub(crate) use plain_rs::mdns::host_responder::local_ipv4_strs as discover_local_ipv4_strs;

// ── Remote-device login sessions (peers.token) ───────────────────────────────

/// Records a successful login: creates the peer as UNPAIRED (or refreshes the
/// existing row) with the session token.
#[tauri::command]
pub async fn login_peer(
    state: tauri::State<'_, NearbyDiscoverManager>,
    id: String,
    name: String,
    host: String,
    deviceType: DeviceType,
    token: String,
    signaturePublicKey: String,
) -> Result<(), String> {
    let mgr = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        mgr.login_peer(&id, &name, &host, deviceType, &token, &signaturePublicKey);
    })
    .await
    .map_err(|e| e.to_string())
}

/// Clears the login token of a peer (logout / forget device).
#[tauri::command]
pub async fn logout_peer(
    state: tauri::State<'_, NearbyDiscoverManager>,
    id: String,
) -> Result<(), String> {
    let mgr = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || mgr.logout_peer(&id))
        .await
        .map_err(|e| e.to_string())
}

/// All peers with an active login token.
#[tauri::command]
pub async fn list_login_peers(
    state: tauri::State<'_, NearbyDiscoverManager>,
) -> Result<Vec<Peer>, String> {
    let mgr = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || mgr.login_peers())
        .await
        .map_err(|e| e.to_string())
}

/// Updates a peer's display name.
#[tauri::command]
pub async fn update_peer_name(
    state: tauri::State<'_, NearbyDiscoverManager>,
    id: String,
    name: String,
) -> Result<(), String> {
    let mgr = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || mgr.update_peer_name(&id, &name))
        .await
        .map_err(|e| e.to_string())
}

/// Current `ip:port` of a paired peer from the peers table, kept fresh by
/// the resident mDNS listener. Used by the frontend to heal a stale login
/// session host without a multicast round-trip.
#[tauri::command]
pub async fn peer_address(
    state: tauri::State<'_, NearbyDiscoverManager>,
    id: String,
) -> Result<Option<String>, String> {
    let mgr = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || mgr.peer_address(&id))
        .await
        .map_err(|e| e.to_string())
}

// ── mDNS debug surface — mirrors plain-app's MdnsDebugPage + WebAddressBar ───

/// Read-only snapshot of every currently-known `_plainapp._tcp.local`
/// instance (raw wire data as parsed) — mirrors `MdnsServiceBrowser.snapshot`.
#[tauri::command]
pub async fn mdns_snapshot(
    state: tauri::State<'_, NearbyDiscoverManager>,
) -> Result<Vec<plain_rs::mdns::service_browser::MdnsServiceSnapshot>, String> {
    let mgr = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || mgr.mdns_snapshot())
        .await
        .map_err(|e| e.to_string())
}

/// Triggers an immediate one-shot mDNS PTR browse. The resident listener
/// refreshes a paired peer's IP/port, healing a stale WS dial on the next
/// reconnect.
#[tauri::command]
pub async fn mdns_browse(state: tauri::State<'_, NearbyDiscoverManager>) -> Result<(), String> {
    let mgr = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || mgr.browse())
        .await
        .map_err(|e| e.to_string())
}

/// Starts periodic mDNS browsing (idempotent). Returns whether THIS call
/// started it, so the page can stop it on exit only when it started it —
/// mirrors `MdnsDebugPage.startedByPage`.
#[tauri::command]
pub async fn mdns_start_browse(
    state: tauri::State<'_, NearbyDiscoverManager>,
) -> Result<bool, String> {
    let mgr = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || mgr.start_discovery())
        .await
        .map_err(|e| e.to_string())
}

/// Stops periodic mDNS browsing started by `mdns_start_browse`.
#[tauri::command]
pub async fn mdns_stop_browse(
    state: tauri::State<'_, NearbyDiscoverManager>,
) -> Result<bool, String> {
    let mgr = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || mgr.stop_discovery())
        .await
        .map_err(|e| e.to_string())
}

/// Current mDNS hostname — mirrors plain-app's `TempData.mdnsHostname`.
#[tauri::command]
pub async fn mdns_get_hostname(
    state: tauri::State<'_, NearbyDiscoverManager>,
) -> Result<String, String> {
    let mgr = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || mgr.mdns_hostname())
        .await
        .map_err(|e| e.to_string())
}

/// Persists and applies a new mDNS hostname. Validation mirrors plain-app's
/// `MdnsAndPortEditDialog`: non-blank with a `.local` suffix.
#[tauri::command]
pub async fn mdns_set_hostname(
    handle: tauri::AppHandle,
    state: tauri::State<'_, NearbyDiscoverManager>,
    hostname: String,
) -> Result<(), String> {
    let hostname = hostname.trim().to_string();
    if hostname.is_empty() || !hostname.ends_with(".local") {
        return Err("mdns hostname must end with .local".to_string());
    }
    let mgr = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || mgr.set_mdns_hostname(&handle, &hostname))
        .await
        .map_err(|e| e.to_string())
}

// ── Windows Firewall repair (LAN/mDNS discovery) ─────────────────────────────

/// Read-only firewall state for the current exe. No admin token required.
#[tauri::command]
pub async fn mdns_firewall_status() -> Result<MdnsFirewallStatus, String> {
    tauri::async_runtime::spawn_blocking(mdns_firewall::probe_status)
        .await
        .map_err(|e| e.to_string())?
}

/// Adds the inbound Allow rule via a UAC-elevated PowerShell script.
/// Returns Ok as soon as the elevated child is spawned — the frontend polls
/// `mdns_firewall_status` until the rule shows up.
#[tauri::command]
pub async fn fix_mdns_firewall() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(mdns_firewall::apply_fix)
        .await
        .map_err(|e| e.to_string())?
}
