//! `ChatMessageMutation` — thin GraphQL surface for chat-item mutations.
//!
//! No business logic lives here; every resolver delegates to
//! [`crate::local::chat_handler`] (the chat service layer). The GraphQL
//! layer only parses the wire arguments and forwards them.

use async_graphql::{Context, Object};
use std::sync::Arc;

use super::super::context::AppCtx;
use super::types::{ActionResult, ChatItem};
use crate::local::chat_handler;

#[derive(Default)]
pub struct ChatMessageMutation;

#[Object]
impl ChatMessageMutation {
    /// Send a chat item. Routes by `to_id` prefix:
    ///   * `peer:<id>`      — peer-to-peer
    ///   * `channel:<id>`   — channel
    ///   * anything else    — local note
    ///
    /// Delivery is fire-and-forget; returns the initially-inserted `ChatItem`
    /// (status `pending` for remote targets).
    async fn send_chat_item(
        &self,
        ctx: &Context<'_>,
        target: String,
        content: String,
    ) -> Vec<ChatItem> {
        chat_handler::send_chat_item(ctx.data_unchecked::<Arc<AppCtx>>(), target, content)
    }

    /// Delete a chat item, broadcasting `WS_MESSAGE_DELETED`.
    async fn delete_chat_item(&self, ctx: &Context<'_>, id: String) -> bool {
        chat_handler::delete_chat_item(ctx.data_unchecked::<Arc<AppCtx>>(), id)
    }

    /// Bulk-delete chats by query (`ids:`, `channel:`, `peer:`).
    async fn delete_chat_items(&self, ctx: &Context<'_>, query: String) -> ActionResult {
        ActionResult {
            affected_count: chat_handler::delete_chat_items(ctx.data_unchecked::<Arc<AppCtx>>(), query),
        }
    }

    /// Retry a failed chat item.
    async fn retry_chat_item(&self, ctx: &Context<'_>, id: String) -> Result<ChatItem, async_graphql::Error> {
        chat_handler::retry_chat_item(ctx.data_unchecked::<Arc<AppCtx>>(), id)
            .ok_or_else(|| async_graphql::Error::new("chat item not found"))
    }
}
