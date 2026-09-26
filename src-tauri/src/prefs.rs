//! Thin desktop-shell preference helpers over the shared plain-rs
//! engine (`plain_rs::prefs`). The Tauri shell owns no storage code:
//! everything persists through the process-wide `Arc<Prefs>` managed
//! as Tauri state in `lib.rs` — the same instance the local API server
//! uses, so `<app_data_dir>/prefs.json` has exactly one writer.
//! Identity / url-token / mDNS-hostname bootstrap and the DLNA sender
//! lists live in plain-rs (`plain_rs::prefs::identity`,
//! `plain_rs::prefs::dlna`).

pub use plain_rs::prefs::Prefs;

pub use plain_rs::prefs::dlna;
pub use plain_rs::prefs::{AppIdentity, ensure_identity, ensure_mdns_hostname, ensure_url_token};

/// Persist the device display name (plain-app `device_name` key).
pub fn set_device_name(prefs: &Prefs, name: &str) {
    let _ = prefs.set("device_name", name);
}

/// Read the saved device display name ("" when unset).
pub fn get_device_name(prefs: &Prefs) -> String {
    prefs.get_or("device_name", String::new())
}

/// User-configured HTTP port (default 8080, set via DeviceInfo card).
/// Matches plain-app's `HttpPortPreference` — single field is both the
/// user preference and the bound port; no separate "preferred" slot.
pub fn get_http_port(prefs: &Prefs) -> u16 {
    prefs.get_or("http_port", 8080u64) as u16
}

pub fn set_http_port(prefs: &Prefs, port: u16) {
    let _ = prefs.set("http_port", port as u64);
}

/// User-configured HTTPS port (default 8443, set via DeviceInfo card).
pub fn get_https_port(prefs: &Prefs) -> u16 {
    prefs.get_or("https_port", 8443u64) as u16
}

pub fn set_https_port(prefs: &Prefs, port: u16) {
    let _ = prefs.set("https_port", port as u64);
}

/// User-configured global capture accelerator; empty string means
/// "platform default". The value is re-validated against the shortcut
/// parser before use.
pub fn get_capture_shortcut(prefs: &Prefs) -> Option<String> {
    let value: String = prefs.get_or("capture_shortcut", String::new());
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

pub fn set_capture_shortcut(prefs: &Prefs, value: Option<&str>) {
    let _ = prefs.set("capture_shortcut", value.unwrap_or(""));
}
