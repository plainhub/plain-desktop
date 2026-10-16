use std::sync::Arc;

use plain_server::assembly::server::ContentServer;
use plain_server::host::platform::{SYSTEM_APP_UPDATE_FACTS, SYSTEM_DISKS_FACTS};
use serde_json::Value;
use tauri::Emitter;

pub struct Runtime {
    core: Arc<ContentServer>,
    app: tauri::AppHandle,
    capabilities: Vec<String>,
    multicast: std::sync::Mutex<plain_server::host::multicast::MulticastLeases>,
}

impl Runtime {
    pub fn new(core: Arc<ContentServer>, app: tauri::AppHandle, capabilities: Vec<String>) -> Self {
        Self {
            core,
            app,
            capabilities,
            multicast: Default::default(),
        }
    }

    async fn dispatch(&self, method: &str, params: Value) -> Result<Value, String> {
        match method {
            "systemCapabilities" => {
                serde_json::to_value(&self.capabilities).map_err(|error| error.to_string())
            }
            plain_server::host::platform::SYSTEM_MOUNT_FACTS => {
                tokio::task::spawn_blocking(plain_server::services::files::mounts::facts)
                    .await
                    .map_err(|error| error.to_string())?
            }
            "systemPermissionFacts" => plain_server::host::permission_facts::facts(
                &["WRITE_EXTERNAL_STORAGE".to_owned()],
                params,
            ),
            "fileTaskAuthorize" => {
                let allowed = tokio::task::spawn_blocking(move || {
                    plain_server::host::file_resource::authorize_read(params)
                })
                .await
                .map_err(|error| error.to_string())??;
                Ok(Value::Bool(allowed))
            }
            method if plain_server::native::audio::handles(method) => {
                self.core.native_audio()?.handle(method, params).await
            }

            "mdnsMulticast" => {
                let result = self
                    .multicast
                    .lock()
                    .map_err(|_| "multicast lease state unavailable")?
                    .without_platform_lock(params)?;
                Ok(Value::Bool(result))
            }

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

    pub fn start(self: Arc<Self>) -> tokio::task::JoinHandle<Result<(), String>> {
        let session = Arc::new(self.core.host_events());
        tokio::spawn(self.run(session))
    }

    async fn run(
        self: Arc<Self>,
        session: Arc<plain_server::host::events::HostEvents>,
    ) -> Result<(), String> {
        let _multicast = plain_server::host::multicast::MulticastSession::new(&self.multicast);
        let requests = self.clone();
        let result = session
            .serve(
                move |method, params| {
                    let requests = requests.clone();
                    async move { requests.dispatch(&method, params).await }
                },
                {
                    let app = self.app.clone();
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
                        app.emit(kind, payload).map_err(|error| error.to_string())
                    }
                },
            )
            .await;
        result
    }
}
