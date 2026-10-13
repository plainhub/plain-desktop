pub const SYSTEM_MOUNT_FACTS: &str = "systemMountFacts";
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskFacts {
    pub id: String,
    pub name: String,
    pub path: String,
    pub size_bytes: i64,
    pub removable: bool,
    pub model: Option<String>,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct DisksFacts {
    pub disks: Vec<DiskFacts>,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateFacts {
    pub current_version: String,
    pub latest_version: Option<String>,
    pub has_update: bool,
    pub url: Option<String>,
}

pub const SYSTEM_DISKS_FACTS: &str = "systemDisksFacts";
pub const SYSTEM_APP_UPDATE_FACTS: &str = "systemAppUpdateFacts";

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppFacts {
    pub app_dir: String,
    pub device_type: String,
    pub capabilities: Vec<String>,
    pub build_channel: String,
    pub permissions: Vec<String>,
    pub downloads_dir: String,
    pub debug: bool,
}

pub const SYSTEM_APP_FACTS: &str = "systemAppFacts";

pub const SYSTEM_SAMBA_SERVICE_FACTS: &str = "systemSambaServiceFacts";
pub const SYSTEM_SAMBA_APPLY: &str = "systemSambaApply";
pub const SYSTEM_SAMBA_PASSWORD: &str = "systemSambaPassword";

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SambaServiceFacts {
    pub name: String,
    pub active: bool,
    pub enabled: bool,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SambaApplyRequest {
    pub settings: plain_rs::prefs::samba::SambaSettings,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SambaPasswordRequest {
    pub password: String,
}

pub const SYSTEM_DISK_FORMAT: &str = "systemDiskFormat";
pub const SYSTEM_HOSTNAME_SET: &str = "systemHostnameSet";

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiskFormatRequest {
    pub path: String,
    pub client_id: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HostnameRequest {
    pub name: String,
}
