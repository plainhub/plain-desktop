//! App-level preferences that surface directly on GraphQL — device
//! display name and the stable server identity. Both are plain
//! preference entries (plain-app: `device_name`, `client_id`).

use crate::prefs::Prefs;

/// Device display name override (plain-app `DeviceNamePreference`). An
/// empty value means "no override" — `App.deviceName` falls back to the
/// system hostname, mirroring plain-app's `.ifEmpty { getDeviceName() }`.
pub fn device_display_name(prefs: &Prefs) -> String {
    prefs
        .get::<String>("device_name")
        .ok()
        .flatten()
        .unwrap_or_default()
}

/// Stable per-server identifier (generated on first use). Returned as
/// `clientId` in auth responses and `App.clientId` — web clients use it
/// as the remote peer identity for TOFU verification.
pub fn server_client_id(prefs: &Prefs) -> String {
    if let Ok(Some(id)) = prefs.get::<String>("client_id") {
        return id;
    }
    let id = crate::crypto::gen_token();
    let _ = prefs.set("client_id", &id);
    id
}
