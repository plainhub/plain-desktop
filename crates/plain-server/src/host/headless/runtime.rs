use std::sync::Arc;

use crate::assembly::server::ContentServer;
use crate::host::platform::{SYSTEM_APP_FACTS, SYSTEM_APP_UPDATE_FACTS, SYSTEM_DISKS_FACTS};
use serde_json::Value;

#[cfg(target_os = "linux")]
mod linux;

pub struct Runtime {
    core: Arc<ContentServer>,
    capabilities: Vec<String>,
    multicast: std::sync::Mutex<crate::host::multicast::MulticastLeases>,
}

impl Runtime {
    pub fn new(core: Arc<ContentServer>, capabilities: Vec<String>) -> Self {
        Self {
            core,
            capabilities,
            multicast: Default::default(),
        }
    }

    async fn dispatch(&self, method: &str, params: Value) -> Result<Value, String> {
        #[cfg(target_os = "linux")]
        if linux::handles(method) {
            return linux::dispatch(&self.core, method, params).await;
        }
        match method {
            "systemSetTempValue" => {
                let key = params["key"]
                    .as_str()
                    .ok_or("Temporary value key required")?
                    .to_owned();
                let value = params["value"]
                    .as_str()
                    .ok_or("Temporary value required")?
                    .to_owned();
                self.core.set_temporary_value(key, value)?;
                Ok(Value::Bool(true))
            }
            "systemRecentFileFacts" => {
                let paths = plain_media::storage::kv::recent::get_recent_files(
                    self.core.preferences()?,
                    500,
                );
                let mut rows = Vec::new();
                for path in paths {
                    if let Ok(entry) =
                        plain_rs::files::filesystem::stat(std::path::Path::new(&path)).await
                    {
                        rows.push(serde_json::json!({
                            "name":std::path::Path::new(&entry.path).file_name().and_then(|name|name.to_str()).unwrap_or_default(),
                            "path":entry.path,"isDir":entry.is_dir,"size":entry.size,"childCount":entry.child_count,
                            "createdAt":entry.created_at.timestamp_millis(),"updatedAt":entry.updated_at.timestamp_millis(),
                        }));
                    }
                }
                Ok(Value::Array(rows))
            }
            "systemImageFileInfo" | "systemVideoFileInfo" | "systemAudioFileInfo" => {
                let path = params["path"]
                    .as_str()
                    .ok_or("Media path required")?
                    .to_owned();
                let kind = match method {
                    "systemImageFileInfo" => plain_rs::enums::DataType::Image,
                    "systemVideoFileInfo" => plain_rs::enums::DataType::Video,
                    _ => plain_rs::enums::DataType::Audio,
                };
                tokio::task::spawn_blocking(move || super::native::media::media_info(&path, kind))
                    .await
                    .map_err(|error| error.to_string())
            }
            "systemDeviceInfoFacts" => {
                tokio::task::spawn_blocking(super::native::platform::device_info)
                    .await
                    .map_err(|error| error.to_string())?
            }
            "systemDeviceStatusFacts" => {
                tokio::task::spawn_blocking(super::native::platform::device_status)
                    .await
                    .map_err(|error| error.to_string())?
            }
            "systemAppLogFacts" => {
                let path =
                    crate::platform::system::log::default_log_file(self.core.data_directory());
                let offset = params["offset"].as_u64().unwrap_or_default() as usize;
                let limit = params["limit"].as_u64().unwrap_or_default() as usize;
                let needle = params["query"]
                    .as_str()
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned);
                tokio::task::spawn_blocking(move || {
                    let lines = crate::platform::system::log::read_lines_newest_first(
                        &path,
                        needle.as_deref(),
                        offset,
                        limit,
                    );
                    serde_json::json!({"path":path.to_string_lossy(),"lines":lines})
                })
                .await
                .map_err(|error| error.to_string())
            }
            "systemClearAppLogs" => {
                let path =
                    crate::platform::system::log::default_log_file(self.core.data_directory());
                tokio::task::spawn_blocking(move || std::fs::write(path, []))
                    .await
                    .map_err(|error| error.to_string())?
                    .map_err(|error| error.to_string())?;
                Ok(Value::Bool(true))
            }
            crate::host::platform::SYSTEM_MOUNT_FACTS => {
                tokio::task::spawn_blocking(crate::services::files::mounts::facts)
                    .await
                    .map_err(|error| error.to_string())?
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
            "systemPermissionFacts" => {
                let app = super::native::app::app_facts(
                    self.core.data_directory(),
                    self.capabilities.clone(),
                )?;
                crate::host::permission_facts::facts(&app.permissions, params)
            }
            "fileTaskAuthorize" => {
                let allowed = tokio::task::spawn_blocking(move || {
                    crate::host::file_resource::authorize_read(params)
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

            "thumbnailAuthorize" => {
                let paths = params
                    .get("paths")
                    .cloned()
                    .ok_or("Thumbnail paths required")?;
                tokio::task::spawn_blocking(move || {
                    crate::host::file_resource::authorize_read(
                        serde_json::json!({"operation":"read","paths":paths}),
                    )
                })
                .await
                .map_err(|error| error.to_string())??;
                Ok(Value::String(
                    self.core
                        .headless_state()
                        .media_scans
                        .policy()?
                        .cache_directory
                        .to_string_lossy()
                        .into_owned(),
                ))
            }
            "thumbnailDecode" => super::native::media::thumbnail(self.core.clone(), params).await,
            "systemFileResource" => {
                super::native::media::file_resource(self.core.clone(), params).await
            }
            "mdnsMulticast" => {
                let result = self
                    .multicast
                    .lock()
                    .map_err(|_| "multicast lease state unavailable")?
                    .without_platform_lock(params)?;
                Ok(Value::Bool(result))
            }

            "systemCastAddressFacts" => {
                let ip = plain_rs::protocols::mdns::host_responder::local_ipv4_strs()
                    .into_iter()
                    .next()
                    .ok_or("No LAN address available for casting")?;
                Ok(
                    serde_json::json!({"ip":ip,"defaultDeviceName":plain_rs::prefs::identity::default_device_name()}),
                )
            }
            "discoveryFacts" => {
                let settings: plain_rs::prefs::user::UserSettings =
                    serde_json::from_str(&self.core.user_settings_json()?)
                        .map_err(|error| error.to_string())?;
                let facts = super::native::discovery::discovery_facts(
                    settings.device_name,
                    crate::platform::system::version::full_version(),
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
                let facts = tokio::task::spawn_blocking(super::native::platform::app_update)
                    .await
                    .map_err(|error| error.to_string())??;
                serde_json::to_value(facts).map_err(|error| error.to_string())
            }
            _ => Err(format!("Unsupported headless host method: {method}")),
        }
    }

    pub async fn run(self: Arc<Self>) -> Result<(), String> {
        let _multicast = crate::host::multicast::MulticastSession::new(&self.multicast);
        let session = Arc::new(self.core.host_events());
        let requests = self.clone();
        let result = session
            .serve(
                move |method, params| {
                    let requests = requests.clone();
                    async move { requests.dispatch(&method, params).await }
                },
                |event| {
                    let kind = event
                        .get("type")
                        .and_then(Value::as_str)
                        .ok_or("missing host event type")?;
                    let payload = event
                        .get("payload")
                        .and_then(Value::as_str)
                        .ok_or("missing host event payload")?;
                    if kind == plain_rs::events::HTTP_SERVER_FAILED {
                        return Err(format!("HTTP server failed: {payload}"));
                    }
                    log::info!("[shell] {kind}: {payload}");
                    Ok(())
                },
            )
            .await;
        result
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/host/headless/runtime.rs"]
mod tests;
