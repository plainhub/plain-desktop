use super::{Prefs, PrefsError};
use serde::{Deserialize, Serialize};

const KEY_SAMBA_SETTINGS: &str = "samba_settings";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SambaShareAuth {
    Guest,
    Password,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SambaShare {
    pub name: String,
    #[serde(rename = "sharePath")]
    pub share_path: String,
    pub auth: SambaShareAuth,
    #[serde(rename = "readOnly")]
    pub read_only: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SambaSettings {
    pub enabled: bool,
    pub username: String,
    #[serde(rename = "hasPassword")]
    pub has_password: bool,
    pub shares: Vec<SambaShare>,
    #[serde(rename = "serviceName")]
    pub service_name: String,
    #[serde(rename = "serviceActive")]
    pub service_active: bool,
    #[serde(rename = "serviceEnabled")]
    #[serde(default)]
    pub service_enabled: bool,
}

impl Default for SambaSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            username: "nas".to_string(),
            has_password: false,
            shares: Vec::new(),
            service_name: "smbd".to_string(),
            service_active: false,
            service_enabled: false,
        }
    }
}

pub fn get_samba_settings(prefs: &Prefs) -> Result<SambaSettings, PrefsError> {
    Ok(prefs
        .get::<SambaSettings>(KEY_SAMBA_SETTINGS)?
        .unwrap_or_default())
}

pub fn set_samba_settings(prefs: &Prefs, settings: &SambaSettings) -> Result<(), PrefsError> {
    prefs.set(KEY_SAMBA_SETTINGS, settings)?;
    Ok(())
}

pub fn configure(
    prefs: &Prefs,
    enabled: bool,
    shares: Vec<SambaShare>,
) -> anyhow::Result<SambaSettings> {
    let previous = get_samba_settings(prefs)?;
    anyhow::ensure!(!enabled || !shares.is_empty(), "no shares configured");
    anyhow::ensure!(
        !shares
            .iter()
            .any(|share| share.auth == SambaShareAuth::Password)
            || previous.has_password,
        "password required"
    );
    let desired = SambaSettings {
        enabled,
        shares,
        ..previous
    };
    set_samba_settings(prefs, &desired)?;
    Ok(desired)
}

#[cfg(test)]
#[path = "../../tests/unit/prefs/samba.rs"]
mod tests;
