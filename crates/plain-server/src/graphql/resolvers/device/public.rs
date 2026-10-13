use crate::graphql::resolvers::app::types::Capability;
use crate::graphql::resolvers::device::permissions::Permission;
use crate::graphql::types::Instant;
use crate::graphql::types::KeyValuePair;
use crate::graphql::types::Long;
use crate::graphql::types::facts::flag;
use crate::graphql::types::facts::integer;
use crate::graphql::types::facts::list;
use crate::graphql::types::facts::optional_instant;
use crate::graphql::types::facts::text;
use crate::host::bridge::Host;
use async_graphql::{Context, Enum, Object, SimpleObject};
use serde_json::{Value, json};
use std::sync::Arc;

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug, Default)]
pub enum DeviceType {
    #[default]
    Computer,
    Phone,
    Tablet,
    Tv,
    Nas,
    Other,
}

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug, Default)]
pub enum DevicePlatform {
    #[default]
    Android,
    Ios,
    Macos,
    Windows,
    Linux,
}

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug, Default)]
pub enum AppChannelType {
    #[default]
    Github,
    Google,
    Fdroid,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct DisplayInfo {
    pub width: i32,
    pub height: i32,
    pub density: f64,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct AndroidExtras {
    #[graphql(name = "sdkVersion")]
    pub sdk_version: i32,
    #[graphql(name = "versionCodeName")]
    pub version_code_name: String,
    #[graphql(name = "securityPatch")]
    pub security_patch: String,
    pub bootloader: String,
    pub fingerprint: String,
    pub hardware: String,
    #[graphql(name = "radioVersion")]
    pub radio_version: String,
    pub board: String,
    #[graphql(name = "buildBrand")]
    pub build_brand: String,
    #[graphql(name = "buildNumber")]
    pub build_number: String,
    pub device: String,
    #[graphql(name = "javaVmVersion")]
    pub java_vm_version: String,
    #[graphql(name = "glEsVersion")]
    pub gl_es_version: String,
    #[graphql(name = "buildTime")]
    pub build_time: Instant,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct DeviceInfo {
    pub name: String,
    pub platform: DevicePlatform,
    pub manufacturer: String,
    pub model: String,
    #[graphql(name = "osName")]
    pub os_name: String,
    #[graphql(name = "osVersion")]
    pub os_version: String,
    #[graphql(name = "kernelVersion")]
    pub kernel_version: String,
    #[graphql(name = "appVersion")]
    pub app_version: String,
    #[graphql(name = "appBuildNumber")]
    pub app_build_number: String,
    pub language: String,
    #[graphql(name = "cpuArch")]
    pub cpu_arch: String,
    #[graphql(name = "cpuModel")]
    pub cpu_model: Option<String>,
    #[graphql(name = "totalMemory")]
    pub total_memory: Long,
    #[graphql(name = "totalStorage")]
    pub total_storage: Long,
    pub display: Option<DisplayInfo>,
    pub android: Option<AndroidExtras>,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct Temperature {
    pub label: String,
    pub celsius: f64,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct DeviceStatus {
    #[graphql(name = "uptimeSec")]
    pub uptime_sec: Long,
    #[graphql(name = "batteryLevel")]
    pub battery_level: Option<i32>,
    pub charging: bool,
    pub temperatures: Vec<Temperature>,
    #[graphql(name = "cpuUsage")]
    pub cpu_usage: f64,
    #[graphql(name = "memoryAvailable")]
    pub memory_available: Option<Long>,
    #[graphql(name = "storageAvailable")]
    pub storage_available: Long,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct App {
    #[graphql(name = "clientId")]
    pub client_id: String,
    #[graphql(name = "urlToken")]
    pub url_token: String,
    #[graphql(name = "httpPort")]
    pub http_port: i32,
    #[graphql(name = "httpsPort")]
    pub https_port: i32,
    #[graphql(name = "appDir")]
    pub app_dir: String,
    #[graphql(name = "deviceName")]
    pub device_name: String,
    #[graphql(name = "deviceType")]
    pub device_type: DeviceType,
    pub capabilities: Vec<Capability>,
    #[graphql(name = "buildChannel")]
    pub build_channel: AppChannelType,
    pub permissions: Vec<Permission>,
    #[graphql(name = "downloadsDir")]
    pub downloads_dir: String,
    #[graphql(name = "developerMode")]
    pub developer_mode: bool,
    pub debug: bool,
}

impl DeviceType {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            DeviceType::Computer => "COMPUTER",
            DeviceType::Phone => "PHONE",
            DeviceType::Tablet => "TABLET",
            DeviceType::Tv => "TV",
            DeviceType::Nas => "NAS",
            DeviceType::Other => "OTHER",
        }
    }
}

#[derive(Default)]
pub struct DeviceQuery;

#[Object]
impl DeviceQuery {
    /// Battery level is null when the platform cannot report it; CPU and
    /// storage are not, because a zero there is a real reading.
    async fn device_status(&self, ctx: &Context<'_>) -> async_graphql::Result<DeviceStatus> {
        let facts = host_call(ctx, "systemDeviceStatusFacts", json!({})).await?;
        Ok(DeviceStatus {
            uptime_sec: Long(integer(&facts, "uptimeSec")),
            battery_level: facts["batteryLevel"].as_i64().map(|value| value as i32),
            charging: flag(&facts, "charging"),
            temperatures: list(&facts, "temperatures", |item| Temperature {
                label: text(item, "label"),
                celsius: item["celsius"].as_f64().unwrap_or_default(),
            }),
            cpu_usage: facts["cpuUsage"].as_f64().unwrap_or_default(),
            memory_available: facts["memoryAvailable"].as_i64().map(Long),
            storage_available: Long(integer(&facts, "storageAvailable")),
        })
    }

    async fn device_info(&self, ctx: &Context<'_>) -> async_graphql::Result<DeviceInfo> {
        let facts = host_call(ctx, "systemDeviceInfoFacts", json!({})).await?;
        Ok(DeviceInfo {
            name: text(&facts, "name"),
            platform: device_platform(&text(&facts, "platform")),
            manufacturer: text(&facts, "manufacturer"),
            model: text(&facts, "model"),
            os_name: text(&facts, "osName"),
            os_version: text(&facts, "osVersion"),
            kernel_version: text(&facts, "kernelVersion"),
            app_version: text(&facts, "appVersion"),
            app_build_number: text(&facts, "appBuildNumber"),
            language: text(&facts, "language"),
            cpu_arch: text(&facts, "cpuArch"),
            cpu_model: facts["cpuModel"].as_str().map(str::to_string),
            total_memory: Long(integer(&facts, "totalMemory")),
            total_storage: Long(integer(&facts, "totalStorage")),
            display: optional_display(&facts["display"]),
            android: optional_android(&facts["android"]),
        })
    }

    /// Everything a client needs to reach this device: its identity, the
    /// port it listens on and the token that authorises it.
    async fn app(&self, ctx: &Context<'_>) -> async_graphql::Result<App> {
        let facts: crate::host::platform::AppFacts = serde_json::from_value(
            host_call(ctx, crate::host::platform::SYSTEM_APP_FACTS, json!({})).await?,
        )
        .map_err(|error| async_graphql::Error::new(format!("invalid app facts: {error}")))?;
        let prefs = ctx.data::<Arc<plain_rs::prefs::Prefs>>()?;
        let user = plain_rs::prefs::user::snapshot(prefs)?;
        let system = plain_rs::prefs::system::snapshot(prefs)?;
        let grants = &facts.permissions;
        let local = crate::http::auth::public_gate::is_local(ctx);
        let mut permissions: Vec<_> = grants
            .iter()
            .filter(|name| local || system.api_permissions.contains(name))
            .filter_map(|name| Permission::parse(name))
            .collect();
        for (name, permission) in [
            ("RECORD_AUDIO", Permission::RecordAudio),
            ("ADB", Permission::Adb),
        ] {
            if grants.iter().any(|grant| grant == name) && !permissions.contains(&permission) {
                permissions.push(permission);
            }
        }
        let (http_port, https_port) = match ctx.data_opt::<crate::assembly::server::ServerState>() {
            Some(state) => state
                .public_ports()
                .await
                .map(|(http, https)| (i32::from(http), i32::from(https)))
                .unwrap_or((0, 0)),
            None => (user.http_port, user.https_port),
        };
        Ok(App {
            client_id: system.client_id,
            url_token: system.url_token,
            http_port,
            https_port,
            app_dir: facts.app_dir,
            device_name: user.device_name,
            device_type: device_type(&facts.device_type),
            capabilities: facts
                .capabilities
                .iter()
                .filter_map(|value| Capability::parse(value))
                .collect(),
            build_channel: app_channel(&facts.build_channel),
            permissions,
            downloads_dir: facts.downloads_dir,
            developer_mode: user.developer_mode,
            debug: facts.debug,
        })
    }

    /// Log lines, newest first. A blank `query` pages the buffer directly;
    /// a non-blank one filters the whole buffer and then pages, so a
    /// search cannot be cut short by the page size.
    async fn app_logs(
        &self,
        ctx: &Context<'_>,
        query: String,
        offset: i32,
        limit: i32,
    ) -> async_graphql::Result<Vec<String>> {
        let text = text_of(&query);
        let facts = host_call(
            ctx,
            "systemAppLogFacts",
            json!({ "offset": offset, "limit": limit, "query": text }),
        )
        .await?;
        Ok(crate::graphql::types::facts::strings(&facts, "lines"))
    }

    async fn app_log_path(&self, ctx: &Context<'_>) -> async_graphql::Result<String> {
        let facts = host_call(ctx, "systemAppLogFacts", json!({ "pathOnly": true })).await?;
        Ok(text(&facts, "path"))
    }

    /// Whether the mDNS advertiser is currently scanning. Starting and
    /// stopping are commands, not state, so they report success rather
    /// than a snapshot the caller would have to re-read.
    async fn is_discovering(&self, ctx: &Context<'_>) -> async_graphql::Result<bool> {
        let facts = ctx
            .data::<crate::assembly::server::ServerState>()?
            .mdns
            .snapshot();
        Ok(flag(&facts, "scanning"))
    }
}

#[derive(Default)]
pub struct DeviceMutation;

#[Object]
impl DeviceMutation {
    async fn clear_app_logs(&self, ctx: &Context<'_>) -> async_graphql::Result<bool> {
        host_call(ctx, "systemClearAppLogs", json!({})).await?;
        Ok(true)
    }

    async fn set_temp_value(
        &self,
        ctx: &Context<'_>,
        key: String,
        value: String,
    ) -> async_graphql::Result<KeyValuePair> {
        host_call(
            ctx,
            "systemSetTempValue",
            json!({ "key": key, "value": value }),
        )
        .await?;
        // The pair echoes what was stored rather than re-reading it: the
        // platform keeps this in memory, so a read-back would race.
        Ok(KeyValuePair { key, value })
    }

    /// The app restarts itself; the response only acknowledges that the
    /// request was accepted, since the connection dies with the process.
    async fn relaunch_app(&self, ctx: &Context<'_>) -> async_graphql::Result<bool> {
        host_call(ctx, "systemRelaunchApp", json!({})).await?;
        Ok(true)
    }

    /// Renaming the device also re-publishes the mDNS advertisement, so a
    /// client that waits for the new name to appear does see it.
    async fn update_device_name(
        &self,
        ctx: &Context<'_>,
        name: String,
    ) -> async_graphql::Result<bool> {
        crate::graphql::resolvers::settings::state::update_user(
            ctx,
            plain_rs::prefs::user::UserSettingsPatch {
                device_name: Some(name),
                ..Default::default()
            },
        )
        .await?;
        Ok(true)
    }

    async fn start_discovery(&self, ctx: &Context<'_>) -> async_graphql::Result<bool> {
        let state = ctx.data::<crate::assembly::server::ServerState>()?;
        state
            .mdns
            .execute(
                state,
                crate::services::discovery::mdns_runtime::Request::Start {},
            )
            .await?;
        Ok(true)
    }

    async fn stop_discovery(&self, ctx: &Context<'_>) -> async_graphql::Result<bool> {
        let state = ctx.data::<crate::assembly::server::ServerState>()?;
        state
            .mdns
            .execute(
                state,
                crate::services::discovery::mdns_runtime::Request::Stop {},
            )
            .await?;
        Ok(true)
    }
}

/// The `text` field of the search DSL, which is what the log view filters
/// on. A query with other fields but no `text` still filters on nothing.
fn text_of(query: &str) -> String {
    plain_rs::utils::search_dsl::parse(query)
        .into_iter()
        .find(|field| field.name == "text")
        .map(|field| field.value)
        .unwrap_or_default()
}

fn optional_display(value: &Value) -> Option<DisplayInfo> {
    if value.is_null() {
        return None;
    }
    Some(DisplayInfo {
        width: integer(value, "width") as i32,
        height: integer(value, "height") as i32,
        density: value["density"].as_f64().unwrap_or_default(),
    })
}

fn optional_android(value: &Value) -> Option<AndroidExtras> {
    if value.is_null() {
        return None;
    }
    Some(AndroidExtras {
        sdk_version: integer(value, "sdkVersion") as i32,
        version_code_name: text(value, "versionCodeName"),
        security_patch: text(value, "securityPatch"),
        bootloader: text(value, "bootloader"),
        fingerprint: text(value, "fingerprint"),
        hardware: text(value, "hardware"),
        radio_version: text(value, "radioVersion"),
        board: text(value, "board"),
        build_brand: text(value, "buildBrand"),
        build_number: text(value, "buildNumber"),
        device: text(value, "device"),
        java_vm_version: text(value, "javaVmVersion"),
        gl_es_version: text(value, "glEsVersion"),
        build_time: optional_instant(value, "buildTime").unwrap_or_else(epoch),
    })
}

fn device_platform(value: &str) -> DevicePlatform {
    match value {
        "IOS" => DevicePlatform::Ios,
        "MACOS" => DevicePlatform::Macos,
        "WINDOWS" => DevicePlatform::Windows,
        "LINUX" => DevicePlatform::Linux,
        _ => DevicePlatform::Android,
    }
}

pub(crate) fn device_type(value: &str) -> DeviceType {
    match value {
        "PHONE" => DeviceType::Phone,
        "TABLET" => DeviceType::Tablet,
        "TV" => DeviceType::Tv,
        "NAS" => DeviceType::Nas,
        "OTHER" => DeviceType::Other,
        _ => DeviceType::Computer,
    }
}

fn app_channel(value: &str) -> AppChannelType {
    match value {
        "GOOGLE" => AppChannelType::Google,
        "FDROID" => AppChannelType::Fdroid,
        _ => AppChannelType::Github,
    }
}

fn epoch() -> Instant {
    Instant(chrono::DateTime::from_timestamp(0, 0).unwrap_or_default())
}

async fn host_call(ctx: &Context<'_>, method: &str, params: Value) -> async_graphql::Result<Value> {
    ctx.data_unchecked::<Arc<Host>>()
        .call(method, params)
        .await
        .map_err(async_graphql::Error::new)
}

#[cfg(test)]
#[path = "../../../../tests/unit/graphql/resolvers/device/public.rs"]
mod tests;
