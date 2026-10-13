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
