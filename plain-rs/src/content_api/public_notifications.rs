//! Public `/graphql` notification roots.
//!
//! Reads share the platform listener cache with the app's own provider view
//! ([`super::system_providers`]), so a notification dismissed on the device
//! disappears from both surfaces at once.

use super::public_facts::{flag, id, instant, strings, text};
use super::public_gate;
use super::system_providers;
use crate::content_api::host::Host;
use crate::content_types::{ActionResult, Notification};
use crate::prefs::Prefs;
use async_graphql::{Context, Object};
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
        offset: i32,
        limit: i32,
        query: String,
    ) -> async_graphql::Result<Vec<Notification>> {
        public_gate::require(ctx.data_unchecked::<Arc<Prefs>>(), &[PERMISSION])?;
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
        public_gate::require(ctx.data_unchecked::<Arc<Prefs>>(), &[PERMISSION])?;
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
        public_gate::require(ctx.data_unchecked::<Arc<Prefs>>(), &[PERMISSION])?;
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
        public_gate::require(ctx.data_unchecked::<Arc<Prefs>>(), &[PERMISSION])?;
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
    Ok(system_providers::notifications(
        ctx.data_unchecked::<Arc<Prefs>>(),
        facts,
        query,
    ))
}

fn notification(value: &Value) -> Notification {
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
#[path = "../../tests/unit/content_api/public_notifications.rs"]
mod tests;
