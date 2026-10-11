//! Public `/graphql` notification roots.
//!
//! Reads share the platform listener cache with the app's own provider view
//! ([`crate::http::routes::system`]), so a notification dismissed on the device
//! disappears from both surfaces at once.

use crate::graphql::resolvers::notifications::types::Notification;
use crate::graphql::types::ActionResult;
use crate::graphql::types::facts::flag;
use crate::graphql::types::facts::id;
use crate::graphql::types::facts::instant;
use crate::graphql::types::facts::strings;
use crate::graphql::types::facts::text;
use crate::host::bridge::Host;
use crate::http::auth::public_gate;
use crate::http::routes::system as system_providers;
use async_graphql::{Context, Object};
use plain_rs::prefs::Prefs;
use serde_json::Value;
use std::sync::Arc;

const PERMISSION: &str = "NOTIFICATION_LISTENER";

#[derive(Default)]
pub struct NotificationsQuery;

#[Object]
impl NotificationsQuery {
    async fn notifications(
        &self,
        ctx: &Context<'_>,
        query: String,
        offset: i32,
        limit: i32,
    ) -> async_graphql::Result<Vec<Notification>> {
        public_gate::require(ctx, ctx.data_unchecked::<Arc<Prefs>>(), &[PERMISSION])?;
        let items = notification_items(ctx, &query).await?;
        Ok(system_providers::page(items, offset.into(), limit.into())
            .iter()
            .map(notification)
            .collect())
    }

    async fn notification_count(
        &self,
        ctx: &Context<'_>,
        query: String,
    ) -> async_graphql::Result<i32> {
        public_gate::require(ctx, ctx.data_unchecked::<Arc<Prefs>>(), &[PERMISSION])?;
        Ok(notification_items(ctx, &query).await?.len() as i32)
    }
}

#[derive(Default)]
pub struct NotificationsMutation;

#[Object]
impl NotificationsMutation {
    async fn delete_notifications(
        &self,
        ctx: &Context<'_>,
        ids: Vec<async_graphql::ID>,
    ) -> async_graphql::Result<ActionResult> {
        public_gate::require(ctx, ctx.data_unchecked::<Arc<Prefs>>(), &[PERMISSION])?;
        let ids: Vec<String> = ids.iter().map(|id| id.0.clone()).collect();
        let receipt = ctx
            .data_unchecked::<Arc<Host>>()
            .call(
                "systemCancelNotifications",
                serde_json::json!({ "ids": ids }),
            )
            .await
            .map_err(|error| async_graphql::Error::new(error))?;
        let cancelled: Vec<String> = serde_json::from_value(receipt)
            .map_err(|error| async_graphql::Error::new(error.to_string()))?;
        // The platform only confirms what it actually dismissed; a receipt
        // naming an id that was never sent is a host contract violation.
        if cancelled.iter().any(|id| !ids.contains(id)) {
            return Err(async_graphql::Error::new(
                "invalid notification cancellation receipt",
            ));
        }
        Ok(ActionResult {
            affected_count: cancelled.len() as i32,
        })
    }

    /// `actionIndex` indexes [`Notification::reply_actions`], the
    /// reply-capable subset — not the plain action list.
    async fn reply_notification(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
        action_index: i32,
        text: String,
    ) -> async_graphql::Result<bool> {
        public_gate::require(ctx, ctx.data_unchecked::<Arc<Prefs>>(), &[PERMISSION])?;
        if action_index < 0 {
            return Err(async_graphql::Error::new("action_not_found"));
        }
        let receipt = ctx
            .data_unchecked::<Arc<Host>>()
            .call(
                "systemReplyNotification",
                serde_json::json!({
                    "id": id.0,
                    "actionIndex": action_index,
                    "text": text,
                }),
            )
            .await
            .map_err(|error| async_graphql::Error::new(error))?;
        if receipt != Value::Bool(true) {
            return Err(async_graphql::Error::new("action_not_found"));
        }
        Ok(true)
    }
}

async fn notification_items(ctx: &Context<'_>, query: &str) -> async_graphql::Result<Vec<Value>> {
    let facts = ctx
        .data_unchecked::<Arc<Host>>()
        .call("systemNotificationFacts", serde_json::json!({}))
        .await
        .map_err(|error| async_graphql::Error::new(error))?;
    let facts: Vec<Value> = serde_json::from_value(facts)
        .map_err(|error| async_graphql::Error::new(error.to_string()))?;
    crate::services::notifications::query::notifications(
        ctx.data_unchecked::<Arc<Prefs>>(),
        facts,
        query,
    )
    .map_err(|error| async_graphql::Error::new(error.to_string()))
}

pub(crate) fn notification(value: &Value) -> Notification {
    Notification {
        id: id(value, "id"),
        only_once: flag(value, "onlyOnce"),
        is_clearable: flag(value, "isClearable"),
        app_id: id(value, "appId"),
        app_name: text(value, "appName"),
        posted_at: instant(value, "time"),
        silent: flag(value, "silent"),
        title: text(value, "title"),
        body: text(value, "body"),
        actions: strings(value, "actions"),
        reply_actions: strings(value, "replyActions"),
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/graphql/resolvers/notifications/public.rs"]
mod tests;

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EventFacts {
    #[serde(rename = "type")]
    event_type: String,
    notification: Value,
}
pub(crate) async fn publish(
    axum::extract::State(state): axum::extract::State<crate::assembly::server::ServerState>,
    headers: axum::http::HeaderMap,
    axum::Json(facts): axum::Json<EventFacts>,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    if !state.authenticated(&headers) {
        return axum::http::StatusCode::UNAUTHORIZED.into_response();
    }
    let kind = match facts.event_type.as_str() {
        "NOTIFICATION_CREATED" => "NOTIFICATION_CREATED",
        "NOTIFICATION_UPDATED" => "NOTIFICATION_UPDATED",
        "NOTIFICATION_DELETED" => "NOTIFICATION_DELETED",
        _ => return axum::http::StatusCode::BAD_REQUEST.into_response(),
    };
    if facts.event_type != "NOTIFICATION_DELETED"
        && (!public_gate::is_enabled(&state.prefs, &[PERMISSION])
            || crate::services::notifications::query::notifications(
                &state.prefs,
                vec![facts.notification.clone()],
                "",
            )
            .map_or(true, |items| items.is_empty()))
    {
        return axum::Json(true).into_response();
    }
    let item = notification(&facts.notification);
    let payload = serde_json::to_string(&item).unwrap();
    let _ = state
        .events
        .send(plain_rs::events::WsEvent::broadcast(kind, payload));
    axum::Json(true).into_response()
}
