use std::sync::Arc;

use plain_server::assembly::server::ContentServer;
use plain_server::host::platform::{SYSTEM_APP_FACTS, SYSTEM_APP_UPDATE_FACTS, SYSTEM_DISKS_FACTS};
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
            "systemPermissionFacts" => {
                let app = super::native::app::app_facts(
                    self.core.data_directory(),
                    self.capabilities.clone(),
                )?;
                plain_server::host::permission_facts::facts(&app.permissions, params)
            }
            "fileTaskAuthorize" => {
                let allowed = tokio::task::spawn_blocking(move || {
                    plain_server::host::file_resource::authorize_read(params)
                })
                .await
                .map_err(|error| error.to_string())??;
                Ok(Value::Bool(allowed))
            }
            "systemMediaBucketItemFacts" => {
                let core = self.core.clone();
                let rows = tokio::task::spawn_blocking(move || {
                    core.native_media_catalog()?.bucket_item_facts(params)
                })
                .await
                .map_err(|error| error.to_string())??;
                Ok(Value::Array(rows))
            }

            "systemMediaRows" => {
                let core = self.core.clone();
                let rows = tokio::task::spawn_blocking(move || {
                    core.native_media_catalog()?.rows_facts(params)
                })
                .await
                .map_err(|error| error.to_string())??;
                Ok(Value::Array(rows))
            }

            "mediaAction" => {
                let core = self.core.clone();
                let outcome = tokio::task::spawn_blocking(move || {
                    core.native_media_catalog()?.delete_action_files(params)
                })
                .await
                .map_err(|error| error.to_string())??;
                serde_json::to_value(outcome).map_err(|error| error.to_string())
            }
            "systemMediaIdsFacts" => {
                let core = self.core.clone();
                let ids = tokio::task::spawn_blocking(move || {
                    core.native_media_catalog()?.ids_facts(params)
                })
                .await
                .map_err(|error| error.to_string())??;
                Ok(serde_json::json!(ids))
            }
            "systemMediaCount" => {
                let core = self.core.clone();
                let count = tokio::task::spawn_blocking(move || {
                    core.native_media_catalog()?.count_facts(params)
                })
                .await
                .map_err(|error| error.to_string())??;
                Ok(serde_json::json!(count))
            }

            "systemFileResource" => {
                plain_server::host::file_resource::facts(self.core.data_directory(), params)
            }
            "scanFilesFacts" => {
                let core = self.core.clone();
                let completed = tokio::task::spawn_blocking(move || {
                    core.native_media_catalog()?.scan_completed_files(params)
                })
                .await
                .map_err(|error| error.to_string())??;
                Ok(Value::Bool(completed))
            }
            "mdnsMulticast" => {
                let result = self
                    .multicast
                    .lock()
                    .map_err(|_| "multicast lease state unavailable")?
                    .without_platform_lock(params)?;
                Ok(Value::Bool(result))
            }

            "discoveryFacts" => {
                let settings: plain_rs::prefs::user::UserSettings =
                    serde_json::from_str(&self.core.user_settings_json()?)
                        .map_err(|error| error.to_string())?;
                let facts = super::native::discovery::discovery_facts(
                    settings.device_name,
                    self.app.package_info().version.to_string(),
                )?;
                serde_json::to_value(facts).map_err(|error| error.to_string())
            }
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
        let _multicast = plain_server::host::multicast::MulticastSession::new(&self.multicast);
        let session = Arc::new(self.core.host_events());
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
