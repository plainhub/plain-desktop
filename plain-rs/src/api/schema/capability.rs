use super::capability_types::*;
use super::types::KeyValuePair;
use crate::api::context::AppCtx;
use crate::media::gql::types::{Instant, MediaDataType};
use async_graphql::{Context, ID, Object, Result};
use std::sync::Arc;

#[derive(Default)]
pub struct CapabilityQuery;

#[Object]
impl CapabilityQuery {
    async fn audio_lyrics(&self, path: String) -> Result<Option<String>> {
        tokio::task::spawn_blocking(move || crate::media::lyrics::extract_lyrics_from_path(&path))
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))
            .map(|value| (!value.is_empty()).then_some(value))
    }

    async fn disks(&self, ctx: &Context<'_>) -> Result<Vec<StorageDisk>> {
        ctx.data_unchecked::<Arc<AppCtx>>()
            .shell
            .disks()
            .map_err(gql_error)
    }

    async fn sessions(&self, ctx: &Context<'_>) -> Vec<Session> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        crate::media::kv::SessionStore::new(&c.media.db)
            .list()
            .into_iter()
            .map(|s| Session {
                client_id: s.client_id,
                client_name: s.client_name,
                last_active: Instant(s.last_active),
                created_at: Instant(s.created_at),
                updated_at: Instant(s.updated_at),
            })
            .collect()
    }

    async fn audit_events(
        &self,
        ctx: &Context<'_>,
        offset: i32,
        limit: i32,
        query: String,
    ) -> Result<Vec<AuditEvent>> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        let needle = crate::media::search::parse(&query)
            .into_iter()
            .find(|f| f.name == "text")
            .map(|f| f.value)
            .unwrap_or_default();
        let rows = crate::media::kv::EventLog::new(&c.media.db)
            .list(
                offset.max(0) as usize,
                limit.max(0) as usize,
                (!needle.trim().is_empty()).then_some(needle.as_str()),
            )
            .map_err(gql_error)?;
        Ok(rows
            .into_iter()
            .filter_map(|e| {
                Some(AuditEvent {
                    id: e.id.into(),
                    r#type: AuditEventType::from_kind(&e.r#type)?,
                    message: e.message,
                    client_id: e.client_id,
                    created_at: Instant(e.created_at),
                })
            })
            .collect())
    }

    async fn app_update(&self, ctx: &Context<'_>) -> Result<AppUpdate> {
        ctx.data_unchecked::<Arc<AppCtx>>()
            .shell
            .app_update()
            .map_err(gql_error)
    }

    async fn samba_settings(&self, ctx: &Context<'_>) -> Result<SambaSettings> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        c.shell.samba_settings(&c.prefs).map_err(gql_error)
    }

    async fn dlna_renderers(&self, ctx: &Context<'_>) -> Result<Vec<DlnaRenderer>> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        let cid = ctx
            .data_opt::<String>()
            .map(String::as_str)
            .unwrap_or_default();
        c.shell.dlna_renderers(cid).map_err(gql_error)
    }
}

#[derive(Default)]
pub struct CapabilityMutation;

#[Object]
impl CapabilityMutation {
    async fn set_hostname(&self, ctx: &Context<'_>, name: String) -> Result<bool> {
        let name = sanitize_hostname(&name);
        if name.is_empty() {
            return Err(async_graphql::Error::new("device_name_invalid"));
        }
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        c.shell.set_hostname(&name).map_err(gql_error)?;
        let cid = ctx
            .data_opt::<String>()
            .map(String::as_str)
            .unwrap_or_default();
        let _ = crate::media::kv::EventLog::new(&c.media.db).add("set_hostname", &name, cid);
        Ok(true)
    }

    async fn logout(&self, ctx: &Context<'_>) -> bool {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        let cid = ctx
            .data_opt::<String>()
            .map(String::as_str)
            .unwrap_or_default();
        if cid.is_empty() {
            return false;
        }
        let store = crate::media::kv::SessionStore::new(&c.media.db);
        let name = store.get(cid).map(|s| s.client_name).unwrap_or_default();
        let _ = crate::media::kv::EventLog::new(&c.media.db).add("logout", &name, cid);
        store.delete(cid).is_ok()
    }

    async fn revoke_session(&self, ctx: &Context<'_>, client_id: String) -> bool {
        if client_id.trim().is_empty() {
            return false;
        }
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        let store = crate::media::kv::SessionStore::new(&c.media.db);
        let name = store
            .get(&client_id)
            .map(|s| s.client_name)
            .unwrap_or_default();
        let _ = crate::media::kv::EventLog::new(&c.media.db).add("revoke", &name, &client_id);
        store.delete(&client_id).is_ok()
    }

    async fn format_disk(&self, ctx: &Context<'_>, path: String) -> Result<bool> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>().clone();
        let cid = ctx.data_opt::<String>().cloned().unwrap_or_default();
        tokio::task::spawn_blocking(move || c.shell.format_disk(&c.prefs, &path, &cid))
            .await
            .map_err(gql_error)?
            .map_err(gql_error)?;
        Ok(true)
    }

    async fn set_mount_alias(&self, ctx: &Context<'_>, id: ID, alias: String) -> Result<bool> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        crate::media::kv::storage::set_alias(&c.prefs, id.as_str(), &alias).map_err(gql_error)?;
        Ok(true)
    }

    async fn set_temp_value(&self, key: String, value: String) -> Result<KeyValuePair> {
        if key.trim().is_empty() {
            return Err(async_graphql::Error::new("key is empty"));
        }
        crate::api::temp_store::set(&key, &value);
        Ok(KeyValuePair { key, value })
    }

    async fn set_samba_settings(
        &self,
        ctx: &Context<'_>,
        input: SambaSettingsInput,
    ) -> Result<bool> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        c.shell
            .set_samba_settings(&c.prefs, input)
            .map_err(gql_error)?;
        Ok(true)
    }

    async fn set_samba_user_password(&self, ctx: &Context<'_>, password: String) -> Result<bool> {
        if password.trim().is_empty() {
            return Err(async_graphql::Error::new("password required"));
        }
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        c.shell
            .set_samba_user_password(&c.prefs, &password)
            .map_err(gql_error)?;
        Ok(true)
    }

    async fn dlna_cast(
        &self,
        ctx: &Context<'_>,
        renderer_udn: String,
        url: String,
        title: String,
        mime: String,
        #[graphql(name = "type")] media_type: MediaDataType,
    ) -> Result<bool> {
        if media_type == MediaDataType::DOC {
            return Err(async_graphql::Error::new("dlna_cast_doc_unsupported"));
        }
        let c = ctx.data_unchecked::<Arc<AppCtx>>().clone();
        tokio::task::spawn_blocking(move || {
            c.shell
                .dlna_cast(&renderer_udn, &url, &title, &mime, media_type, &c.prefs)
        })
        .await
        .map_err(gql_error)?
        .map_err(gql_error)?;
        Ok(true)
    }
}

fn gql_error(error: impl std::fmt::Display) -> async_graphql::Error {
    async_graphql::Error::new(error.to_string())
}

fn sanitize_hostname(input: &str) -> String {
    let value = input.trim().to_lowercase().replace(['_', ' ', '.'], "-");
    let mut result = String::with_capacity(value.len());
    let mut dash = false;
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            result.push(ch);
            dash = false;
        } else if ch == '-' && !dash && !result.is_empty() {
            result.push('-');
            dash = true;
        }
    }
    while result.ends_with('-') {
        result.pop();
    }
    result.truncate(result.len().min(63));
    result
}

#[cfg(test)]
#[path = "../../../tests/unit/api/schema/capability.rs"]
mod tests;
