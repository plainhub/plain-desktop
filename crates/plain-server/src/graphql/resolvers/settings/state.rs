use super::models::{SettingsUpdate, SystemSettings, UserSettings, UserSettingsInput};
use super::system_input::SystemSettingsInput;
use crate::graphql::types::ActionResult;
use async_graphql::{Context, Json, Object, Result};
use plain_rs::{events::WsEvent, prefs::Prefs};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::broadcast;

fn prefs<'a>(ctx: &'a Context<'_>) -> Result<&'a Arc<Prefs>> {
    if let Some(prefs) = ctx.data_opt::<Arc<Prefs>>() {
        return Ok(prefs);
    }
    #[cfg(feature = "api")]
    if let Some(context) = ctx.data_opt::<Arc<crate::assembly::context::AppCtx>>() {
        return Ok(&context.prefs);
    }
    Ok(ctx.data::<Arc<Prefs>>()?)
}
fn events<'a>(ctx: &'a Context<'_>) -> Option<&'a broadcast::Sender<WsEvent>> {
    if let Some(events) = ctx.data_opt::<broadcast::Sender<WsEvent>>() {
        return Some(events);
    }
    #[cfg(feature = "api")]
    if let Some(context) = ctx.data_opt::<Arc<crate::assembly::context::AppCtx>>() {
        return Some(&context.event_tx);
    }
    None
}
fn notify(ctx: &Context<'_>, changed: bool) {
    if changed {
        if let Some(events) = events(ctx) {
            let _ = events.send(WsEvent::broadcast("PREFS_UPDATED", "{}".into()));
        }
    }
}
#[derive(Default)]
pub struct PreferencesQuery;
#[Object]
impl PreferencesQuery {
    async fn system_settings(&self, ctx: &Context<'_>) -> Result<SystemSettings> {
        Ok(plain_rs::prefs::system::snapshot(prefs(ctx)?)?.try_into()?)
    }
    async fn user_settings(&self, ctx: &Context<'_>) -> Result<UserSettings> {
        Ok(plain_rs::prefs::user::snapshot(prefs(ctx)?)?.into())
    }
    async fn verify_adb_token(&self, ctx: &Context<'_>, token: String) -> Result<bool> {
        Ok(plain_rs::prefs::system::verify_adb_token(
            prefs(ctx)?,
            &token,
        )?)
    }
}
#[derive(Default)]
pub struct PreferencesMutation;
#[Object]
impl PreferencesMutation {
    async fn set_system_pref(
        &self,
        ctx: &Context<'_>,
        key: String,
        value: Json<Value>,
    ) -> Result<ActionResult> {
        validate_dynamic_key(&key)?;
        let changed = prefs(ctx)?.set(&key, value.0)?;
        notify(ctx, changed);
        Ok(ActionResult {
            affected_count: i32::from(changed),
        })
    }
    async fn remove_system_pref(&self, ctx: &Context<'_>, key: String) -> Result<ActionResult> {
        validate_dynamic_key(&key)?;
        let changed = prefs(ctx)?.remove(&key)?;
        notify(ctx, changed);
        Ok(ActionResult {
            affected_count: i32::from(changed),
        })
    }
    async fn update_system_settings(
        &self,
        ctx: &Context<'_>,
        input: SystemSettingsInput,
    ) -> Result<SettingsUpdate> {
        let prefs = prefs(ctx)?;
        let command: plain_rs::prefs::system::Command =
            serde_json::from_value(serde_json::to_value(input)?)?;
        let hostname_changed = matches!(
            &command,
            plain_rs::prefs::system::Command::SetMdnsHostname { .. }
        );
        let changed = command.apply(prefs)?;
        notify(ctx, changed);
        #[cfg(feature = "host-api")]
        if hostname_changed {
            if let Some(state) = ctx.data_opt::<crate::assembly::server::ServerState>() {
                state.mdns.update_hostname(state).await?;
            }
        }
        updated(ctx, changed)
    }
    async fn update_user_settings(
        &self,
        ctx: &Context<'_>,
        input: UserSettingsInput,
    ) -> Result<SettingsUpdate> {
        let patch: plain_rs::prefs::user::UserSettingsPatch =
            serde_json::from_value(serde_json::to_value(input)?)?;
        let changed = update_user(ctx, patch).await?;
        updated(ctx, changed)
    }
    async fn record_recent_search(&self, ctx: &Context<'_>, query: String) -> Result<ActionResult> {
        let before = plain_rs::prefs::user::snapshot(prefs(ctx)?)?;
        let changed = plain_rs::prefs::user::Action::RecordSearch(query).apply(prefs(ctx)?)?;
        notify(ctx, changed);
        emit_user_events(ctx, &before)?;
        Ok(ActionResult {
            affected_count: i32::from(changed),
        })
    }
    async fn remove_recent_search(&self, ctx: &Context<'_>, query: String) -> Result<ActionResult> {
        let before = plain_rs::prefs::user::snapshot(prefs(ctx)?)?;
        let changed = plain_rs::prefs::user::Action::RemoveSearch(query).apply(prefs(ctx)?)?;
        notify(ctx, changed);
        emit_user_events(ctx, &before)?;
        Ok(ActionResult {
            affected_count: i32::from(changed),
        })
    }
    async fn clear_recent_searches(&self, ctx: &Context<'_>) -> Result<ActionResult> {
        let before = plain_rs::prefs::user::snapshot(prefs(ctx)?)?;
        let changed = plain_rs::prefs::user::Action::ClearSearches.apply(prefs(ctx)?)?;
        notify(ctx, changed);
        emit_user_events(ctx, &before)?;
        Ok(ActionResult {
            affected_count: i32::from(changed),
        })
    }
    async fn record_recent_save_directory(
        &self,
        ctx: &Context<'_>,
        path: String,
    ) -> Result<ActionResult> {
        let before = plain_rs::prefs::user::snapshot(prefs(ctx)?)?;
        let changed =
            plain_rs::prefs::user::Action::RecordSaveDirectory(path).apply(prefs(ctx)?)?;
        notify(ctx, changed);
        emit_user_events(ctx, &before)?;
        Ok(ActionResult {
            affected_count: i32::from(changed),
        })
    }
    async fn toggle_notification_app(
        &self,
        ctx: &Context<'_>,
        package_name: String,
    ) -> Result<ActionResult> {
        let before = plain_rs::prefs::user::snapshot(prefs(ctx)?)?;
        let changed = plain_rs::prefs::user::Action::ToggleNotificationApp(package_name)
            .apply(prefs(ctx)?)?;
        notify(ctx, changed);
        emit_user_events(ctx, &before)?;
        Ok(ActionResult {
            affected_count: i32::from(changed),
        })
    }
    async fn set_notification_filter_mode(
        &self,
        ctx: &Context<'_>,
        mode: String,
    ) -> Result<ActionResult> {
        let before = plain_rs::prefs::user::snapshot(prefs(ctx)?)?;
        let changed =
            plain_rs::prefs::user::Action::SetNotificationMode(mode).apply(prefs(ctx)?)?;
        notify(ctx, changed);
        emit_user_events(ctx, &before)?;
        Ok(ActionResult {
            affected_count: i32::from(changed),
        })
    }
    async fn clear_notification_filter_apps(&self, ctx: &Context<'_>) -> Result<ActionResult> {
        let before = plain_rs::prefs::user::snapshot(prefs(ctx)?)?;
        let changed = plain_rs::prefs::user::Action::ClearNotificationApps.apply(prefs(ctx)?)?;
        notify(ctx, changed);
        emit_user_events(ctx, &before)?;
        Ok(ActionResult {
            affected_count: i32::from(changed),
        })
    }
    async fn delete_playlist_videos(
        &self,
        ctx: &Context<'_>,
        paths: Vec<String>,
    ) -> Result<ActionResult> {
        let before = plain_rs::prefs::user::snapshot(prefs(ctx)?)?;
        let changed = plain_rs::prefs::user::Action::DeleteVideos(paths).apply(prefs(ctx)?)?;
        notify(ctx, changed);
        emit_user_events(ctx, &before)?;
        Ok(ActionResult {
            affected_count: i32::from(changed),
        })
    }
}
fn emit_user_events(ctx: &Context<'_>, before: &plain_rs::prefs::user::UserSettings) -> Result<()> {
    let settings = plain_rs::prefs::user::snapshot(prefs(ctx)?)?;
    if let Some(events) = events(ctx) {
        if before.device_name != settings.device_name {
            let _ = events.send(WsEvent::broadcast(
                "DEVICE_NAME_UPDATED",
                serde_json::to_string(&settings.device_name)?,
            ));
        }
        if before.notification_filter != settings.notification_filter {
            let _ = events.send(WsEvent::broadcast("NOTIFICATION_REFRESHED", String::new()));
        }
        if before.pomodoro_settings != settings.pomodoro_settings {
            let _ = events.send(WsEvent::broadcast(
                "POMODORO_SETTINGS_UPDATE",
                settings.pomodoro_settings.to_string(),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../../../tests/unit/graphql/resolvers/settings/state.rs"]
mod tests;

fn validate_dynamic_key(key: &str) -> Result<()> {
    if key.is_empty()
        || key.len() > 128
        || !key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
    {
        return Err(async_graphql::Error::new("invalid_pref_key"));
    }
    if [
        plain_rs::prefs::system::keys::PASSWORD,
        plain_rs::prefs::system::keys::PASSWORD_TYPE,
        plain_rs::prefs::system::keys::AUTH_TWO_FACTOR,
        plain_rs::prefs::system::keys::ROTATE_URL_TOKEN_ON_RESTART,
        plain_rs::prefs::system::keys::ADB_TOKEN,
        plain_rs::prefs::system::keys::URL_TOKEN,
        plain_rs::prefs::system::keys::MASTER_SECRET,
        plain_rs::prefs::system::keys::UPDATE_INFO,
        plain_rs::prefs::system::keys::API_PERMISSIONS,
        plain_rs::prefs::system::keys::CLIENT_ID,
        plain_rs::prefs::system::keys::MDNS_HOSTNAME,
        plain_rs::prefs::system::keys::SIGNATURE_KEY_PAIR,
        plain_rs::prefs::system::keys::ONBOARDING_COMPLETED,
    ]
    .contains(&key)
    {
        return Err(async_graphql::Error::new(
            "Use the system settings operation",
        ));
    }
    Ok(())
}

fn updated(ctx: &Context<'_>, changed: bool) -> Result<SettingsUpdate> {
    Ok(SettingsUpdate {
        result: ActionResult {
            affected_count: i32::from(changed),
        },
        system_settings: plain_rs::prefs::system::snapshot(prefs(ctx)?)?.try_into()?,
        user_settings: plain_rs::prefs::user::snapshot(prefs(ctx)?)?.into(),
    })
}

#[cfg(all(test, feature = "host-api", feature = "http_transport"))]
#[path = "../../../../tests/unit/graphql/resolvers/settings/http.rs"]
mod http_tests;

pub(crate) async fn update_user(
    ctx: &Context<'_>,
    patch: plain_rs::prefs::user::UserSettingsPatch,
) -> Result<bool> {
    let before = plain_rs::prefs::user::snapshot(prefs(ctx)?)?;
    let changed = plain_rs::prefs::user::patch(prefs(ctx)?, patch)?;
    notify(ctx, changed);
    #[cfg(feature = "host-api")]
    if before.desktop_access && !plain_rs::prefs::user::snapshot(prefs(ctx)?)?.desktop_access {
        if let Some(state) = ctx.data_opt::<crate::assembly::server::ServerState>() {
            state.main_ws.close_client(state, None);
        }
    }
    let after = plain_rs::prefs::user::snapshot(prefs(ctx)?)?;
    if before.device_name != after.device_name {
        #[cfg(feature = "host-api")]
        if let Some(state) = ctx.data_opt::<crate::assembly::server::ServerState>() {
            state.mdns.update_hostname(state).await?;
        }
        #[cfg(feature = "api")]
        if let Some(context) = ctx.data_opt::<Arc<crate::assembly::context::AppCtx>>() {
            context
                .discover_manager
                .apply_device_rename(&after.device_name);
            context.chat.identity.set_device_name(&after.device_name);
        }
    }
    emit_user_events(ctx, &before)?;
    Ok(changed)
}
