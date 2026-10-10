use crate::graphql::types::Instant;
use crate::graphql::types::Long;
use async_graphql::Enum;
use async_graphql::SimpleObject;

#[derive(Enum, Copy, Clone, Eq, PartialEq)]
#[graphql(name = "DevicePlatform")]
pub enum DevicePlatform {
    Android,
    Ios,
    Macos,
    Windows,
    Linux,
}

#[derive(SimpleObject)]
#[graphql(name = "AndroidExtras")]
pub struct AndroidExtras {
    pub sdk_version: i32,
    pub version_code_name: String,
    pub security_patch: String,
    pub bootloader: String,
    pub fingerprint: String,
    pub hardware: String,
    pub radio_version: String,
    pub board: String,
    pub build_brand: String,
    pub build_number: String,
    pub device: String,
    pub java_vm_version: String,
    pub gl_es_version: String,
    pub build_time: Instant,
}

#[derive(SimpleObject)]
#[graphql(name = "DisplayInfo")]
pub struct DisplayInfo {
    pub width: i32,
    pub height: i32,
    pub density: f64,
}

#[derive(SimpleObject)]
#[graphql(name = "DeviceInfo")]
pub struct DeviceInfo {
    pub name: String,
    pub platform: DevicePlatform,
    pub manufacturer: String,
    pub model: String,
    pub os_name: String,
    pub os_version: String,
    pub kernel_version: String,
    pub app_version: String,
    pub app_build_number: String,
    pub language: String,
    pub cpu_arch: String,
    pub cpu_model: Option<String>,
    pub total_memory: Long,
    pub total_storage: Long,
    pub display: Option<DisplayInfo>,
    pub android: Option<AndroidExtras>,
}

#[derive(SimpleObject)]
#[graphql(name = "Temperature")]
pub struct Temperature {
    pub label: String,
    pub celsius: f64,
}

#[derive(SimpleObject, Default)]
#[graphql(name = "DeviceStatus")]
pub struct DeviceStatus {
    pub uptime_sec: Long,
    /// 0-100; None when the device has no battery or the level is unknown.
    pub battery_level: Option<i32>,
    /// True only while actively charging; full-while-plugged is false.
    pub charging: bool,
    /// Empty when the platform exposes no temperature sources.
    pub temperatures: Vec<Temperature>,
    /// 0-100 percent, diffed from two CPU counter samples.
    pub cpu_usage: f64,
    /// OS-level available memory; None when the platform does not expose it.
    pub memory_available: Option<Long>,
    /// Available bytes on the primary data volume.
    pub storage_available: Long,
}

#[derive(SimpleObject)]
#[graphql(name = "Sim")]
pub struct Sim {
    pub id: String,
    pub label: String,
    pub number: String,
    pub subscription_id: i32,
}
