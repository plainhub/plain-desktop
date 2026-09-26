//! Host-shell implementation of `plain_rs::local_api::ShellHooks` over
//! the Tauri AppHandle + preferences store. Everything the shared local
//! API stack needs from the desktop shell funnels through here.

use plain_rs::local_api::ShellHooks;

pub struct DesktopShell(pub tauri::AppHandle);

impl ShellHooks for DesktopShell {
    fn device_name(&self) -> String {
        crate::prefs::get_device_name(&self.0)
    }

    fn set_device_name(&self, name: &str) {
        crate::prefs::set_device_name(&self.0, name);
    }

    fn set_mdns_hostname(&self, hostname: &str) {
        crate::prefs::set_mdns_hostname(&self.0, hostname);
    }

    fn notify(&self, event: &str, payload: String) {
        use tauri::Emitter;
        let value = serde_json::from_str::<serde_json::Value>(&payload)
            .unwrap_or(serde_json::Value::String(payload));
        let _ = self.0.emit(event, value);
    }

    fn app_version(&self) -> String {
        self.0.package_info().version.to_string()
    }

    fn dlna_enabled(&self) -> bool {
        crate::prefs::get_dlna_enabled(&self.0)
    }

    fn dlna_senders(&self, key: &str) -> Vec<String> {
        match key {
            "dlna_allowed_senders" => crate::prefs::get_dlna_allowed_senders(&self.0),
            "dlna_denied_senders" => crate::prefs::get_dlna_denied_senders(&self.0),
            _ => Vec::new(),
        }
    }

    fn dlna_add_sender(&self, key: &str, ip: &str, name: &str) {
        crate::prefs::add_dlna_sender(&self.0, key, ip, name);
    }

    fn dlna_remove_sender(&self, key: &str, ip: &str) {
        crate::prefs::remove_dlna_sender(&self.0, key, ip);
    }
}
