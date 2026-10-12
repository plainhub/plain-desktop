//! Host-shell implementation of `plain_server::assembly::ShellHooks` — the
//! UI-only hooks. All persisted state goes through the shared
//! `plain_rs::prefs::Prefs`; nothing here touches storage anymore.

use plain_server::assembly::ShellHooks;
use plain_server::graphql::resolvers::app::types::Capability;

/// What this shell can actually drive, in the wire form the schema parses.
/// The host bridge reports the same list, so the UI must not see a section
/// from one surface and not the other.
pub fn desktop_capabilities() -> &'static [&'static str] {
    &["DOC_PREVIEW", "IMAGE_EDITOR", "NOTIFICATIONS"]
}

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
        desktop_capabilities()
            .iter()
            .filter_map(|value| Capability::parse(value))
            .collect()
    }

    fn relaunch_app(&self) -> bool {
        self.0.restart()
    }
}

#[cfg(test)]
mod tests {
    use super::desktop_capabilities;
    use plain_server::graphql::resolvers::app::types::Capability;

    /// The host bridge reports these as wire strings; the schema parses the
    /// same list. A name one side cannot read leaves the UI with a section
    /// it cannot drive.
    #[test]
    fn every_desktop_capability_parses() {
        for name in desktop_capabilities() {
            assert!(
                Capability::parse(name).is_some(),
                "{name} is reported but the schema cannot parse it"
            );
        }
    }

    #[test]
    fn the_desktop_list_is_not_empty() {
        assert!(
            !desktop_capabilities().is_empty(),
            "an empty capability list hides every optional section"
        );
    }
}
