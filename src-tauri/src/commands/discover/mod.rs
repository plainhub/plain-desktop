pub use plain_rs::libs::platform::discovery::MdnsFirewallStatus;
pub use plain_rs::libs::platform::discovery::firewall;

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
