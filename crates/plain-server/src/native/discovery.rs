use crate::assembly::server::ServerState;
use crate::services::discovery::mdns_runtime::Request;

#[derive(Clone)]
pub struct NativeDiscovery {
    state: ServerState,
}

impl NativeDiscovery {
    pub(crate) fn new(state: ServerState) -> Self {
        Self { state }
    }

    pub fn hostname(&self) -> Result<String, String> {
        self.state.native_client_id()?;
        self.state
            .prefs
            .get::<String>(plain_rs::prefs::system::keys::MDNS_HOSTNAME)
            .map_err(|error| error.to_string())?
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "Missing mDNS hostname".to_string())
    }

    pub async fn set_hostname(&self, hostname: &str) -> Result<(), String> {
        let _shutdown = self.state.shutdown.lock().await;
        self.state.native_client_id()?;
        crate::graphql::resolvers::settings::state::apply_mdns_hostname(
            &self.state,
            hostname.trim(),
        )
        .await
        .map_err(|error| error.message)?;
        Ok(())
    }

    /// Every `_plainapp._tcp.local` instance either browser saw. On macOS
    /// that includes the system DNS-SD browser, so the debug page shows
    /// what discovery can actually reach.
    pub async fn snapshot(
        &self,
    ) -> Result<Vec<plain_rs::protocols::mdns::service_browser::MdnsServiceSnapshot>, String> {
        let _shutdown = self.state.shutdown.lock().await;
        self.state.native_client_id()?;
        Ok(self.state.mdns.browser_snapshot())
    }

    /// The system browser's recent activity, for the debug page. Empty on
    /// the platforms without one.
    pub async fn activity(
        &self,
    ) -> Result<Vec<crate::services::discovery::local::MdnsActivity>, String> {
        let _shutdown = self.state.shutdown.lock().await;
        self.state.native_client_id()?;
        Ok(self.state.mdns.browser_activity())
    }

    /// Announce this device on the LAN so peers can find it. Without this
    /// the app is reachable but invisible, which is the failure the old
    /// bootstrap's unconditional `discover_manager.start()` hid.
    pub async fn publish(&self) -> Result<(), String> {
        let _shutdown = self.state.shutdown.lock().await;
        self.state.native_client_id()?;
        self.state
            .mdns
            .execute(
                &self.state,
                crate::services::discovery::mdns_runtime::Request::Publish {},
            )
            .await
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub async fn browse(&self) -> Result<(), String> {
        let _shutdown = self.state.shutdown.lock().await;
        self.state.native_client_id()?;
        self.state
            .mdns
            .execute(&self.state, Request::Browse {})
            .await
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub async fn start_browse(&self) -> Result<bool, String> {
        let _shutdown = self.state.shutdown.lock().await;
        self.state.native_client_id()?;
        let result = self
            .state
            .mdns
            .execute(&self.state, Request::Start {})
            .await
            .map_err(|error| error.to_string())?;
        result["started"]
            .as_bool()
            .ok_or_else(|| "Invalid discovery state".to_string())
    }

    pub async fn stop_browse(&self) -> Result<bool, String> {
        let _shutdown = self.state.shutdown.lock().await;
        self.state.native_client_id()?;
        let active = self.state.mdns.manual_scanning();
        self.state
            .mdns
            .execute(&self.state, Request::Stop {})
            .await
            .map_err(|error| error.to_string())?;
        Ok(active)
    }
}

#[cfg(test)]
#[path = "../../tests/unit/native/discovery.rs"]
mod tests;
