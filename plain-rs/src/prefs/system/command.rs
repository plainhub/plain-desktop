use super::*;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "camelCase", deny_unknown_fields)]
pub enum Command {
    SetPassword { password: String },
    SetPasswordType { value: i32 },
    SetTwoFactor { enabled: bool },
    SetRotateUrlToken { enabled: bool },
    ResetPassword,
    ResetUrlToken,
    ResetAdbToken,
    SetApiPermission { permission: String, enabled: bool },
    SetApiPermissions { permissions: Vec<String> },
    CompleteOnboarding,
    SetMdnsHostname { hostname: String },
    PatchUpdate { patch: UpdatePatch },
}

impl Command {
    pub fn apply(self, prefs: &Prefs) -> anyhow::Result<bool> {
        match self {
            Self::SetPasswordType { value } if ![1, 2].contains(&value) => {
                anyhow::bail!("Invalid password type")
            }
            Self::SetMdnsHostname { ref hostname } => validate_hostname(hostname)?,
            Self::SetApiPermission { ref permission, .. } => anyhow::ensure!(
                PERMISSIONS.contains(&permission.as_str()),
                "Unknown permission"
            ),
            Self::SetApiPermissions { ref permissions } => anyhow::ensure!(
                permissions
                    .iter()
                    .all(|p| PERMISSIONS.contains(&p.as_str())),
                "Unknown permission"
            ),
            _ => {}
        }
        let mut changed = false;
        prefs.update_system(|map| {
            let before = map.clone();
            match self {
                Self::SetPassword { password } => put(map, PASSWORD, password)?,
                Self::SetPasswordType { value } => put(map, PASSWORD_TYPE, value)?,
                Self::SetTwoFactor { enabled } => put(map, AUTH_TWO_FACTOR, enabled)?,
                Self::SetRotateUrlToken { enabled } => {
                    put(map, ROTATE_URL_TOKEN_ON_RESTART, enabled)?
                }
                Self::ResetPassword => put(map, PASSWORD, random_password(6))?,
                Self::ResetUrlToken => put(map, URL_TOKEN, crate::gen_token())?,
                Self::ResetAdbToken => put(map, ADB_TOKEN, random_password(32))?,
                Self::SetApiPermission {
                    permission,
                    enabled,
                } => {
                    let mut permissions: Vec<String> = read(map, API_PERMISSIONS)?;
                    permissions.retain(|p| p != &permission);
                    if enabled {
                        permissions.push(permission);
                    }
                    permissions.sort();
                    put(map, API_PERMISSIONS, permissions)?;
                }
                Self::SetApiPermissions { mut permissions } => {
                    permissions.sort();
                    permissions.dedup();
                    put(map, API_PERMISSIONS, permissions)?;
                }
                Self::CompleteOnboarding => put(map, ONBOARDING_COMPLETED, true)?,
                Self::SetMdnsHostname { hostname } => put(map, MDNS_HOSTNAME, hostname)?,
                Self::PatchUpdate { patch } => {
                    let mut info: UpdateInfo = read(map, UPDATE_INFO)?;
                    patch.apply(&mut info);
                    put(map, UPDATE_INFO, info)?;
                }
            }
            changed = before != *map;
            Ok(())
        })?;
        Ok(changed)
    }
}

fn validate_hostname(hostname: &str) -> anyhow::Result<()> {
    let label = hostname
        .strip_suffix(".local")
        .ok_or_else(|| anyhow::anyhow!("Hostname must end in .local"))?;
    anyhow::ensure!(
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-'),
        "Invalid mDNS hostname"
    );
    Ok(())
}
