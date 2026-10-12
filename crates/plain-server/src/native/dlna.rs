use crate::assembly::server::ServerState;
use crate::services::casting::receiver::control::{self, Request};
use serde_json::Value;

#[derive(serde::Serialize)]
pub struct DlnaSenders {
    pub allowed: Vec<String>,
    pub denied: Vec<String>,
}

#[derive(Clone)]
pub struct NativeDlnaReceiver {
    state: ServerState,
}

impl NativeDlnaReceiver {
    pub(crate) fn new(state: ServerState) -> Self {
        Self { state }
    }

    pub async fn snapshot(&self) -> Result<Value, String> {
        let _core = self.state.shutdown.lock().await;
        self.state.native_client_id()?;
        let mut snapshot = control::snapshot(&self.state.dlna.snapshot().await);
        snapshot["enabled"] = Value::Bool(plain_rs::prefs::dlna::enabled(&self.state.prefs));
        Ok(snapshot)
    }

    pub async fn set_enabled(&self, enabled: bool) -> Result<(), String> {
        let _core = self.state.shutdown.lock().await;
        self.state.native_client_id()?;
        plain_rs::prefs::dlna::set_enabled(&self.state.prefs, enabled)
            .map_err(|error| error.to_string())?;
        let request = if enabled {
            let port = self
                .state
                .public_ports()
                .await
                .ok_or("Public HTTP server is not running")?
                .0;
            Request::Start { port }
        } else {
            Request::Stop {}
        };
        control::execute(&self.state, request).await?;
        Ok(())
    }

    async fn perform(&self, request: Request) -> Result<(), String> {
        let _core = self.state.shutdown.lock().await;
        self.state.native_client_id()?;
        control::execute(&self.state, request).await?;
        Ok(())
    }

    pub async fn accept(&self, remember: bool) -> Result<(), String> {
        self.perform(Request::Accept { remember }).await
    }

    pub async fn reject(&self, remember: bool) -> Result<(), String> {
        self.perform(Request::Reject { remember }).await
    }

    pub fn senders(&self) -> Result<DlnaSenders, String> {
        self.state.native_client_id()?;
        Ok(DlnaSenders {
            allowed: plain_rs::prefs::dlna::senders(&self.state.prefs, "dlna_allowed_senders"),
            denied: plain_rs::prefs::dlna::senders(&self.state.prefs, "dlna_denied_senders"),
        })
    }

    pub async fn remove_sender(&self, allowed: bool, ip: String) -> Result<(), String> {
        self.perform(Request::RemoveRule { ip, allowed }).await
    }
}

#[cfg(test)]
#[path = "../../tests/unit/native/dlna.rs"]
mod tests;
