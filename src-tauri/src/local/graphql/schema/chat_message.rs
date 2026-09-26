//! `ChatMessageMutation` — thin GraphQL surface for chat-item mutations.
//!
//! No business logic lives here; every resolver delegates to the shared
//! chat service ([`crate::local::chat::ChatState`]). The GraphQL layer
//! only parses the wire arguments and forwards them.

use async_graphql::{Context, Object};
use std::sync::Arc;

use super::super::context::AppCtx;
use super::types::{ActionResult, ChatItem};

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
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        c.chat
            .service
            .send_chat_item(target, content)
            .into_iter()
            .map(|chat| ChatItem::with_data(chat, &c.token))
            .collect()
    }

    /// Delete a chat item, broadcasting `WS_MESSAGE_DELETED`.
    async fn delete_chat_item(&self, ctx: &Context<'_>, id: String) -> bool {
        ctx.data_unchecked::<Arc<AppCtx>>()
            .chat
            .service
            .delete_chat_item(id)
    }

    /// Bulk-delete chats by query (`ids:`, `channel:`, `peer:`).
    async fn delete_chat_items(&self, ctx: &Context<'_>, query: String) -> ActionResult {
        ActionResult {
            affected_count: ctx
                .data_unchecked::<Arc<AppCtx>>()
                .chat
                .service
                .delete_chat_items(query),
        }
    }

    /// Retry a failed chat item.
    async fn retry_chat_item(
        &self,
        ctx: &Context<'_>,
        id: String,
    ) -> Result<ChatItem, async_graphql::Error> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        c.chat
            .service
            .retry_chat_item(id)
            .map(|chat| ChatItem::with_data(chat, &c.token))
            .ok_or_else(|| async_graphql::Error::new("chat item not found"))
    }
}
