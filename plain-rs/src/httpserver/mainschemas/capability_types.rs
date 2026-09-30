use super::media::types::{Instant, Long};
use async_graphql::{Enum, ID, InputObject, SimpleObject};

#[derive(SimpleObject, Clone, Debug)]
pub struct StorageDisk {
    pub id: ID,
    pub name: String,
    pub path: String,
    pub size_bytes: Long,
    pub removable: bool,
    pub model: Option<String>,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct Session {
    pub client_id: String,
    pub client_name: String,
    pub last_active: Instant,
    pub created_at: Instant,
    pub updated_at: Instant,
}

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
#[allow(non_camel_case_types, clippy::upper_case_acronyms)]
pub enum AuditEventType {
    LOGIN,
    LOGIN_FAILED,
    LOGOUT,
    REVOKE,
    SET_HOSTNAME,
    UPDATE_DEVICE_NAME,
    MOUNT,
    MOUNT_FAILED,
    UNMOUNT,
    FORMAT_DISK,
    FORMAT_DISK_FAILED,
}

impl AuditEventType {
    pub fn from_kind(kind: &str) -> Option<Self> {
        match kind {
            "login" => Some(Self::LOGIN),
            "login_failed" => Some(Self::LOGIN_FAILED),
            "logout" => Some(Self::LOGOUT),
            "revoke" => Some(Self::REVOKE),
            "set_hostname" => Some(Self::SET_HOSTNAME),
            "update_device_name" => Some(Self::UPDATE_DEVICE_NAME),
            "mount" => Some(Self::MOUNT),
            "mount_failed" => Some(Self::MOUNT_FAILED),
            "unmount" => Some(Self::UNMOUNT),
            "format_disk" => Some(Self::FORMAT_DISK),
            "format_disk_failed" => Some(Self::FORMAT_DISK_FAILED),
            _ => None,
        }
    }
}

#[derive(SimpleObject, Clone, Debug)]
pub struct AuditEvent {
    pub id: ID,
    pub r#type: AuditEventType,
    pub message: String,
    pub client_id: String,
    pub created_at: Instant,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct AppUpdate {
    pub current_version: String,
    pub latest_version: Option<String>,
    pub has_update: bool,
    pub url: Option<String>,
}

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
#[allow(non_camel_case_types)]
pub enum SambaShareAuth {
    GUEST,
    PASSWORD,
}

#[derive(InputObject, Clone, Debug)]
pub struct SambaShareInput {
    pub name: String,
    pub share_path: String,
    pub auth: SambaShareAuth,
    pub read_only: bool,
}

#[derive(InputObject, Clone, Debug)]
pub struct SambaSettingsInput {
    pub enabled: bool,
    pub shares: Vec<SambaShareInput>,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct SambaShare {
    pub name: String,
    pub share_path: String,
    pub auth: SambaShareAuth,
    pub read_only: bool,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct SambaSettings {
    pub enabled: bool,
    pub username: String,
    pub has_password: bool,
    pub shares: Vec<SambaShare>,
    pub service_name: String,
    pub service_active: bool,
    pub service_enabled: bool,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct DlnaRenderer {
    pub udn: String,
    pub name: String,
    pub manufacturer: Option<String>,
    pub model_name: Option<String>,
    pub location: String,
}
