//! `ChatMessageMutation` — thin GraphQL surface for chat-item mutations.
//!
//! No business logic lives here; every resolver delegates to the shared
//! chat service ([`crate::chat_service::ChatState`]). The GraphQL layer
//! only parses the wire arguments and forwards them.

use async_graphql::{Context, ID, Object};
use std::sync::Arc;

use super::types::{ActionResult, ChatItem};
use crate::api::context::AppCtx;

#[derive(Default)]
pub struct ChatMessageMutation;

#[Object]
impl ChatMessageMutation {
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
            .map(|chat| ChatItem::from(chat))
            .collect()
    }

    /// Delete a chat item, broadcasting `WS_MESSAGE_DELETED`.
    async fn delete_chat_item(&self, ctx: &Context<'_>, id: ID) -> bool {
        ctx.data_unchecked::<Arc<AppCtx>>()
            .chat
            .service
            .delete_chat_item(id.to_string())
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
        id: ID,
    ) -> Result<ChatItem, async_graphql::Error> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        c.chat
            .service
            .retry_chat_item(id.to_string())
            .map(|chat| ChatItem::from(chat))
            .ok_or_else(|| async_graphql::Error::new("chat item not found"))
    }
}
