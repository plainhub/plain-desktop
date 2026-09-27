//! DLNA receiver preferences — the enable toggle (`dlna`) and the
//! allowed/denied sender lists (`dlna_allowed_senders` /
//! `dlna_denied_senders`, entries encoded `ip|name`). Mirrors plain-app's
//! `DlnaPreference`; previously duplicated inside the plain-desktop
//! Tauri shells behind AppHandle accessors.

use super::Prefs;

const SEP: char = '|';

/// Whether the DLNA receiver is enabled in host preferences.
pub fn enabled(prefs: &Prefs) -> bool {
    prefs.get_or("dlna", false)
}

pub fn set_enabled(prefs: &Prefs, enabled: bool) {
    let _ = prefs.set("dlna", enabled);
}

/// Decode an `ip|name` sender entry persisted by the allowed/denied
/// lists. Mirrors plain-app's `decodeSenderEntry`.
fn decode_sender_entry(entry: &str) -> (String, String) {
    match entry.split_once(SEP) {
        Some((ip, name)) => (ip.to_string(), name.to_string()),
        None => (entry.to_string(), String::new()),
    }
}

/// The `ip|name` sender list persisted under `key`.
pub fn senders(prefs: &Prefs, key: &str) -> Vec<String> {
    prefs.get_or::<Vec<String>>(key, Vec::new())
}

/// Mirrors plain-app's `containsIp`.
pub fn senders_contain_ip(entries: &[String], ip: &str) -> bool {
    entries.iter().any(|e| decode_sender_entry(e).0 == ip)
}

fn set_sender_list(prefs: &Prefs, key: &str, entries: &[String]) {
    let _ = prefs.set(key, entries);
}

/// Replace any existing entry for `ip` (any previous name) then add
/// `ip|name`. Mirrors plain-app's `addAsync`.
pub fn add_sender(prefs: &Prefs, key: &str, ip: &str, name: &str) {
    let mut next: Vec<String> = senders(prefs, key)
        .into_iter()
        .filter(|e| decode_sender_entry(e).0 != ip)
        .collect();
    next.push(format!("{ip}{SEP}{name}"));
    set_sender_list(prefs, key, &next);
}

/// Remove any entry for `ip`. Mirrors plain-app's `removeAsync`.
pub fn remove_sender(prefs: &Prefs, key: &str, ip: &str) {
    let next: Vec<String> = senders(prefs, key)
        .into_iter()
        .filter(|e| decode_sender_entry(e).0 != ip)
        .collect();
    set_sender_list(prefs, key, &next);
}

#[cfg(test)]
#[path = "../../tests/unit/prefs_dlna.rs"]
mod tests;
