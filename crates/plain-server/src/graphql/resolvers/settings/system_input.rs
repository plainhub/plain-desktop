use async_graphql::{Enum, InputObject, Json};
use serde::Serialize;
use serde_json::Value;

#[derive(Enum, Copy, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SystemSettingsAction {
    SetPassword,
    SetPasswordType,
    SetTwoFactor,
    SetRotateUrlToken,
    ResetPassword,
    ResetUrlToken,
    ResetAdbToken,
    SetApiPermission,
    SetApiPermissions,
    CompleteOnboarding,
    SetMdnsHostname,
    PatchUpdate,
}

/// The action selects a command; only its required arguments may be supplied.
/// PATCH_UPDATE changes supplied fields and leaves omitted fields untouched.
#[derive(InputObject, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemSettingsInput {
    pub action: SystemSettingsAction,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permission: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch: Option<Json<Value>>,
}
