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
        let changed = plain_rs::prefs::system::set_entry(prefs(ctx)?, &key, value.0)?;
        notify(ctx, changed);
        Ok(ActionResult {
            affected_count: i32::from(changed),
        })
    }
    async fn remove_system_pref(&self, ctx: &Context<'_>, key: String) -> Result<ActionResult> {
        validate_dynamic_key(&key)?;
        let changed = plain_rs::prefs::system::remove_entry(prefs(ctx)?, &key)?;
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
        #[cfg(feature = "core")]
        if let plain_rs::prefs::system::Command::SetMdnsHostname { hostname } = &command {
            if let Some(state) = ctx.data_opt::<crate::assembly::server::ServerState>() {
                let changed = apply_mdns_hostname(state, hostname).await?;
                return updated(ctx, changed);
            }
        }
        let changed = command.apply(prefs)?;
        notify(ctx, changed);
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
        emit_user_events_ctx(ctx, &before)?;
        Ok(ActionResult {
            affected_count: i32::from(changed),
        })
    }
    async fn remove_recent_search(&self, ctx: &Context<'_>, query: String) -> Result<ActionResult> {
        let before = plain_rs::prefs::user::snapshot(prefs(ctx)?)?;
        let changed = plain_rs::prefs::user::Action::RemoveSearch(query).apply(prefs(ctx)?)?;
        notify(ctx, changed);
        emit_user_events_ctx(ctx, &before)?;
        Ok(ActionResult {
            affected_count: i32::from(changed),
        })
    }
    async fn clear_recent_searches(&self, ctx: &Context<'_>) -> Result<ActionResult> {
        let before = plain_rs::prefs::user::snapshot(prefs(ctx)?)?;
        let changed = plain_rs::prefs::user::Action::ClearSearches.apply(prefs(ctx)?)?;
        notify(ctx, changed);
        emit_user_events_ctx(ctx, &before)?;
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
        emit_user_events_ctx(ctx, &before)?;
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
        emit_user_events_ctx(ctx, &before)?;
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
        emit_user_events_ctx(ctx, &before)?;
        Ok(ActionResult {
            affected_count: i32::from(changed),
        })
    }
    async fn clear_notification_filter_apps(&self, ctx: &Context<'_>) -> Result<ActionResult> {
        let before = plain_rs::prefs::user::snapshot(prefs(ctx)?)?;
        let changed = plain_rs::prefs::user::Action::ClearNotificationApps.apply(prefs(ctx)?)?;
        notify(ctx, changed);
        emit_user_events_ctx(ctx, &before)?;
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
        emit_user_events_ctx(ctx, &before)?;
        Ok(ActionResult {
            affected_count: i32::from(changed),
        })
    }
}
/// Emits the events derived from one user-settings write. Takes the already
/// computed `after` snapshot so every caller shares one diff, and so a write
/// never re-reads the store it just persisted.
fn emit_user_events(
    events: Option<&broadcast::Sender<WsEvent>>,
    before: &plain_rs::prefs::user::UserSettings,
    after: &plain_rs::prefs::user::UserSettings,
) -> Result<()> {
    if let Some(events) = events {
        if before.device_name != after.device_name {
            let _ = events.send(WsEvent::broadcast(
                "DEVICE_NAME_UPDATED",
                serde_json::to_string(&after.device_name)?,
            ));
        }
        if before.notification_filter != after.notification_filter {
            let _ = events.send(WsEvent::broadcast("NOTIFICATION_REFRESHED", String::new()));
        }
        if before.pomodoro_settings != after.pomodoro_settings {
            let _ = events.send(WsEvent::broadcast(
                "POMODORO_SETTINGS_UPDATE",
                after.pomodoro_settings.to_string(),
            ));
        }
    }
    Ok(())
}

fn emit_user_events_ctx(
    ctx: &Context<'_>,
    before: &plain_rs::prefs::user::UserSettings,
) -> Result<()> {
    let after = plain_rs::prefs::user::snapshot(prefs(ctx)?)?;
    emit_user_events(events(ctx), before, &after)
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

#[cfg(all(test, feature = "core", feature = "http_transport"))]
#[path = "../../../../tests/unit/graphql/resolvers/settings/http.rs"]
mod http_tests;

pub(crate) async fn update_user(
    ctx: &Context<'_>,
    patch: plain_rs::prefs::user::UserSettingsPatch,
) -> Result<bool> {
    apply_user_patch(UserPatchEnv::from_ctx(ctx)?, patch).await
}

/// The handles a user-settings write needs, independent of how the caller
/// reached the store. GraphQL resolvers build one from the request context;
/// the App's native (JNI/C ABI) entry point builds one from the server state.
/// Every path then runs the same [`apply_user_patch`], so validation, the
/// persisted snapshot and the derived events/host effects cannot drift apart.
pub(crate) struct UserPatchEnv<'a> {
    prefs: &'a Arc<Prefs>,
    events: Option<&'a broadcast::Sender<WsEvent>>,
    /// Host (mobile/JNI) side: mDNS advertisement and desktop-access teardown.
    #[cfg(feature = "core")]
    server: Option<&'a crate::assembly::server::ServerState>,
    /// Desktop side: nearby-discover and chat identity follow a rename.
    #[cfg(feature = "api")]
    app: Option<&'a Arc<crate::assembly::context::AppCtx>>,
}

impl<'a> UserPatchEnv<'a> {
    pub(crate) fn from_ctx(ctx: &'a Context<'_>) -> Result<Self> {
        Ok(Self {
            prefs: prefs(ctx)?,
            events: events(ctx),
            #[cfg(feature = "core")]
            server: ctx.data_opt::<crate::assembly::server::ServerState>(),
            #[cfg(feature = "api")]
            app: ctx.data_opt::<Arc<crate::assembly::context::AppCtx>>(),
        })
    }

    /// `graphql/executor` puts this same `ServerState` into every local
    /// request context, so a local resolver and the native caller observe
    /// identical handles.
    #[cfg(feature = "core")]
    pub(crate) fn from_server(state: &'a crate::assembly::server::ServerState) -> Self {
        Self {
            prefs: &state.prefs,
            events: Some(&state.events),
            server: Some(state),
            #[cfg(feature = "api")]
            app: None,
        }
    }
}

/// Validates, persists and publishes one user-settings patch.
pub(crate) async fn apply_user_patch(
    env: UserPatchEnv<'_>,
    patch: plain_rs::prefs::user::UserSettingsPatch,
) -> Result<bool> {
    let before = plain_rs::prefs::user::snapshot(env.prefs)?;
    let changed = plain_rs::prefs::user::patch(env.prefs, patch)?;
    if changed {
        if let Some(events) = env.events {
            let _ = events.send(WsEvent::broadcast("PREFS_UPDATED", "{}".into()));
        }
    }
    // One read after the write feeds every effect below.
    let after = plain_rs::prefs::user::snapshot(env.prefs)?;
    #[cfg(feature = "core")]
    if before.desktop_access && !after.desktop_access {
        if let Some(state) = env.server {
            state.main_ws.close_client(state, None);
        }
    }
    if before.device_name != after.device_name {
        #[cfg(feature = "core")]
        if let Some(state) = env.server {
            state.mdns.update_hostname(state).await?;
        }
        #[cfg(feature = "api")]
        if let Some(app) = env.app {
            app.discover_manager.apply_device_rename(&after.device_name);
            app.chat.identity.set_device_name(&after.device_name);
        }
    }
    emit_user_events(env.events, &before, &after)?;
    Ok(changed)
}

/// Runs one user-settings action and publishes the same events a patch does.
/// Split from [`apply_user_patch`] so both entry points stay in step.
pub(crate) async fn apply_user_action(
    env: UserPatchEnv<'_>,
    action: plain_rs::prefs::user::Action,
) -> Result<bool> {
    let before = plain_rs::prefs::user::snapshot(env.prefs)?;
    let changed = action.apply(env.prefs)?;
    if changed {
        if let Some(events) = env.events {
            let _ = events.send(WsEvent::broadcast("PREFS_UPDATED", "{}".into()));
        }
    }
    let after = plain_rs::prefs::user::snapshot(env.prefs)?;
    emit_user_events(env.events, &before, &after)?;
    Ok(changed)
}

#[cfg(feature = "core")]
pub(crate) async fn apply_mdns_hostname(
    state: &crate::assembly::server::ServerState,
    hostname: &str,
) -> Result<bool> {
    let changed = plain_rs::prefs::system::Command::SetMdnsHostname {
        hostname: hostname.to_owned(),
    }
    .apply(&state.prefs)?;
    if changed {
        let _ = state
            .events
            .send(WsEvent::broadcast("PREFS_UPDATED", "{}".into()));
    }
    state.mdns.update_hostname(state).await?;
    Ok(changed)
}
