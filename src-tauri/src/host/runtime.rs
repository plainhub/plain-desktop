use std::sync::Arc;

use plain_server::assembly::server::ContentServer;
use plain_server::host::platform::{SYSTEM_APP_FACTS, SYSTEM_APP_UPDATE_FACTS, SYSTEM_DISKS_FACTS};
use serde_json::Value;
use tauri::Emitter;

pub struct Runtime {
    core: Arc<ContentServer>,
    app: tauri::AppHandle,
    capabilities: Vec<String>,
}

impl Runtime {
    pub fn new(core: Arc<ContentServer>, app: tauri::AppHandle, capabilities: Vec<String>) -> Self {
        Self {
            core,
            app,
            capabilities,
        }
    }

    async fn dispatch(&self, method: &str, _params: Value) -> Result<Value, String> {
        match method {
            SYSTEM_APP_FACTS => serde_json::to_value(super::native::app::app_facts(
                self.core.data_directory(),
                self.capabilities.clone(),
            )?)
            .map_err(|error| error.to_string()),
            SYSTEM_DISKS_FACTS => {
                let facts = tokio::task::spawn_blocking(super::native::platform::disks)
                    .await
                    .map_err(|error| error.to_string())??;
                serde_json::to_value(facts).map_err(|error| error.to_string())
            }
            SYSTEM_APP_UPDATE_FACTS => {
                let facts = super::native::platform::app_update(&self.app).await?;
                serde_json::to_value(facts).map_err(|error| error.to_string())
            }
            _ => Err(format!("Unsupported desktop host method: {method}")),
        }
    }

    pub async fn run(self: Arc<Self>) -> Result<(), String> {
        let session = Arc::new(self.core.host_events());
        let requests = self.clone();
        session
            .serve(
                move |method, params| {
                    let requests = requests.clone();
                    async move { requests.dispatch(&method, params).await }
                },
                move |event| {
                    let kind = event
                        .get("type")
                        .and_then(Value::as_str)
                        .ok_or("missing host event type")?;
                    let payload = event
                        .get("payload")
                        .and_then(Value::as_str)
                        .ok_or("missing host event payload")?;
                    let payload = serde_json::from_str::<Value>(payload)
                        .unwrap_or_else(|_| Value::String(payload.to_owned()));
                    self.app
                        .emit(kind, payload)
                        .map_err(|error| error.to_string())
                },
            )
            .await
    }
}
