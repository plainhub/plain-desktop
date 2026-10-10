//! Host-shell implementation of `plain_server::api::ShellHooks` — the
//! UI-only hooks. All persisted state goes through the shared
//! `plain_server::prefs::Prefs`; nothing here touches storage anymore.

use plain_server::api::ShellHooks;
use plain_server::http_server::main_schemas::types::Capability;

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

    fn capabilities(&self) -> Vec<Capability> {
        vec![
            Capability::DocPreview,
            Capability::ImageEditor,
            Capability::Notifications,
        ]
    }

    fn relaunch_app(&self) -> bool {
        self.0.restart()
    }
}
