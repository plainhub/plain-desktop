//! Host-shell implementation of `plain_rs::local_api::ShellHooks` — the
//! UI-only hooks. All persisted state goes through the shared
//! `plain_rs::prefs::Prefs`; nothing here touches storage anymore.

use plain_rs::local_api::ShellHooks;

pub struct DesktopShell(pub tauri::AppHandle);

impl ShellHooks for DesktopShell {
    fn notify(&self, event: &str, payload: String) {
        use tauri::Emitter;
        let value = serde_json::from_str::<serde_json::Value>(&payload)
            .unwrap_or(serde_json::Value::String(payload));
        let _ = self.0.emit(event, value);
    }

    fn app_version(&self) -> String {
        self.0.package_info().version.to_string()
    }
}
