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
        let active = self.state.mdns.snapshot()["manualScanning"]
            .as_bool()
            .ok_or_else(|| "Invalid discovery state".to_string())?;
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
