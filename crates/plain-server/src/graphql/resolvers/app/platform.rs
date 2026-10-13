use crate::assembly::server::ServerState;
use crate::graphql::resolvers::app::capability_types::{AppUpdate, Disk};
use crate::graphql::resolvers::app::capability_types::{
    SambaSettings, SambaSettingsInput, SambaShare, SambaShareAuth,
};
use crate::graphql::types::Long;
use crate::host::bridge::Host;
use crate::host::platform::{
    AppUpdateFacts, DisksFacts, SYSTEM_APP_UPDATE_FACTS, SYSTEM_DISKS_FACTS,
};
use crate::host::platform::{
    SYSTEM_SAMBA_APPLY, SYSTEM_SAMBA_PASSWORD, SYSTEM_SAMBA_SERVICE_FACTS, SambaApplyRequest,
    SambaPasswordRequest, SambaServiceFacts,
};
use async_graphql::{Context, Object, Result};
use std::sync::Arc;

#[derive(Default)]
pub struct PlatformQuery;

#[Object]
impl PlatformQuery {
    async fn audit_events(
        &self,
        ctx: &Context<'_>,
        query: String,
        offset: i32,
        limit: i32,
    ) -> Result<Vec<crate::graphql::resolvers::app::capability_types::AuditEvent>> {
        use crate::graphql::resolvers::app::capability_types::{AuditEvent, AuditEventType};
        let state = ctx.data::<ServerState>()?;
        let needle = plain_rs::utils::search_dsl::parse(&query)
            .into_iter()
            .find(|field| field.name == "text")
            .map(|field| field.value)
            .unwrap_or_default();
        let rows = state.db.audit_events(
            offset.max(0) as usize,
            limit.max(0) as usize,
            (!needle.trim().is_empty()).then_some(needle.as_str()),
        )?;
        rows.into_iter()
            .filter_map(|row| {
                let kind = AuditEventType::from_kind(&row.r#type)?;
                Some(
                    chrono::DateTime::parse_from_rfc3339(&row.created_at)
                        .map(|at| AuditEvent {
                            id: row.id.into(),
                            r#type: kind,
                            message: row.message,
                            client_id: row.client_id,
                            created_at: crate::graphql::types::Instant(
                                at.with_timezone(&chrono::Utc),
                            ),
                        })
                        .map_err(|error| async_graphql::Error::new(error.to_string())),
                )
            })
            .collect()
    }

    async fn samba_settings(&self, ctx: &Context<'_>) -> Result<SambaSettings> {
        let state = ctx.data::<ServerState>()?;
        let settings = plain_rs::prefs::samba::get_samba_settings(&state.prefs)?;
        let service = samba_service(ctx).await?;
        Ok(samba_facts(settings, service))
    }

    async fn disks(&self, ctx: &Context<'_>) -> Result<Vec<Disk>> {
        let value = ctx
            .data::<Arc<Host>>()?
            .call(SYSTEM_DISKS_FACTS, serde_json::json!({}))
            .await
            .map_err(async_graphql::Error::new)?;
        let facts: DisksFacts = serde_json::from_value(value)
            .map_err(|error| async_graphql::Error::new(error.to_string()))?;
        Ok(facts
            .disks
            .into_iter()
            .map(|disk| Disk {
                id: disk.id.into(),
                name: disk.name,
                path: disk.path,
                size_bytes: Long(disk.size_bytes),
                removable: disk.removable,
                model: disk.model,
            })
            .collect())
    }

    async fn app_update(&self, ctx: &Context<'_>) -> Result<AppUpdate> {
        let value = ctx
            .data::<Arc<Host>>()?
            .call(SYSTEM_APP_UPDATE_FACTS, serde_json::json!({}))
            .await
            .map_err(async_graphql::Error::new)?;
        let facts: AppUpdateFacts = serde_json::from_value(value)
            .map_err(|error| async_graphql::Error::new(error.to_string()))?;
        Ok(AppUpdate {
            current_version: facts.current_version,
            latest_version: facts.latest_version,
            has_update: facts.has_update,
            url: facts.url,
        })
    }
}

async fn samba_service(ctx: &Context<'_>) -> Result<SambaServiceFacts> {
    let value = ctx
        .data::<Arc<Host>>()?
        .call(SYSTEM_SAMBA_SERVICE_FACTS, serde_json::json!({}))
        .await
        .map_err(async_graphql::Error::new)?;
    Ok(serde_json::from_value(value)?)
}

fn samba_facts(
    settings: plain_rs::prefs::samba::SambaSettings,
    service: SambaServiceFacts,
) -> SambaSettings {
    let (service_name, service_active, service_enabled) = if service.name.is_empty() {
        (
            settings.service_name,
            settings.service_active,
            settings.service_enabled,
        )
    } else {
        (service.name, service.active, service.enabled)
    };
    SambaSettings {
        enabled: settings.enabled,
        username: settings.username,
        has_password: settings.has_password,
        shares: settings
            .shares
            .into_iter()
            .map(|share| SambaShare {
                name: share.name,
                share_path: share.share_path,
                read_only: share.read_only,
                auth: match share.auth {
                    plain_rs::prefs::samba::SambaShareAuth::Guest => SambaShareAuth::GUEST,
                    plain_rs::prefs::samba::SambaShareAuth::Password => SambaShareAuth::PASSWORD,
                },
            })
            .collect(),
        service_name,
        service_active,
        service_enabled,
    }
}

async fn samba_apply(
    ctx: &Context<'_>,
    settings: plain_rs::prefs::samba::SambaSettings,
) -> Result<()> {
    let value = ctx
        .data::<Arc<Host>>()?
        .call(
            SYSTEM_SAMBA_APPLY,
            serde_json::to_value(SambaApplyRequest { settings })?,
        )
        .await
        .map_err(async_graphql::Error::new)?;
    if value != serde_json::Value::Bool(true) {
        return Err("Samba apply failed".into());
    }
    Ok(())
}

#[derive(Default)]
pub struct PlatformMutation;

#[Object]
impl PlatformMutation {
    async fn format_disk(&self, ctx: &Context<'_>, path: String) -> Result<bool> {
        let state = ctx.data::<ServerState>()?;
        let request = crate::host::platform::DiskFormatRequest {
            path,
            client_id: ctx.data_opt::<String>().cloned().unwrap_or_default(),
        };
        let value = state
            .host
            .call_wait(
                crate::host::platform::SYSTEM_DISK_FORMAT,
                serde_json::to_value(request)?,
            )
            .await
            .map_err(async_graphql::Error::new)?;
        if value != serde_json::Value::Bool(true) {
            return Err("Disk format failed".into());
        }
        Ok(true)
    }

    async fn set_hostname(&self, ctx: &Context<'_>, name: String) -> Result<bool> {
        let name = plain_rs::platform_hostname(&name);
        if name.is_empty() {
            return Err("device_name_invalid".into());
        }
        let state = ctx.data::<ServerState>()?;
        let value = state
            .host
            .call(
                crate::host::platform::SYSTEM_HOSTNAME_SET,
                serde_json::to_value(crate::host::platform::HostnameRequest {
                    name: name.clone(),
                })?,
            )
            .await
            .map_err(async_graphql::Error::new)?;
        if value != serde_json::Value::Bool(true) {
            return Err("Hostname update failed".into());
        }
        state.db.audit_event_add(
            "set_hostname",
            &name,
            ctx.data_opt::<String>()
                .map(String::as_str)
                .unwrap_or_default(),
        )?;
        Ok(true)
    }

    async fn set_mount_alias(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
        alias: String,
    ) -> Result<bool> {
        let state = ctx.data::<ServerState>()?;
        plain_rs::prefs::storage::set_alias(&state.prefs, id.as_str(), &alias)?;
        Ok(true)
    }

    async fn set_samba_settings(
        &self,
        ctx: &Context<'_>,
        input: SambaSettingsInput,
    ) -> Result<bool> {
        let state = ctx.data::<ServerState>()?;
        samba_service(ctx).await?;
        let shares = input
            .shares
            .into_iter()
            .map(|share| plain_rs::prefs::samba::SambaShare {
                name: share.name,
                share_path: share.share_path,
                read_only: share.read_only,
                auth: match share.auth {
                    SambaShareAuth::GUEST => plain_rs::prefs::samba::SambaShareAuth::Guest,
                    SambaShareAuth::PASSWORD => plain_rs::prefs::samba::SambaShareAuth::Password,
                },
            })
            .collect();
        let settings = plain_rs::prefs::samba::configure(&state.prefs, input.enabled, shares)
            .map_err(|error| async_graphql::Error::new(error.to_string()))?;
        samba_apply(ctx, settings).await?;
        Ok(true)
    }

    async fn set_samba_user_password(&self, ctx: &Context<'_>, password: String) -> Result<bool> {
        if password.trim().is_empty() {
            return Err("password required".into());
        }
        let state = ctx.data::<ServerState>()?;
        let mut settings = plain_rs::prefs::samba::get_samba_settings(&state.prefs)?;
        let value = ctx
            .data::<Arc<Host>>()?
            .call(
                SYSTEM_SAMBA_PASSWORD,
                serde_json::to_value(SambaPasswordRequest { password })?,
            )
            .await
            .map_err(async_graphql::Error::new)?;
        if value != serde_json::Value::Bool(true) {
            return Err("Samba password update failed".into());
        }
        settings.has_password = true;
        plain_rs::prefs::samba::set_samba_settings(&state.prefs, &settings)?;
        if settings.enabled {
            samba_apply(ctx, settings).await?;
        }
        Ok(true)
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/graphql/resolvers/app/platform.rs"]
mod tests;
