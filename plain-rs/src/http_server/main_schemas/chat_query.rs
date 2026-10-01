use async_graphql::{Context, Object, ID};
use std::sync::Arc;

use super::types::{ChatChannel, ChatItem, Peer};
use crate::api::context::AppCtx;
use crate::chat::enums::ChannelStatus;

#[derive(Default)]
pub struct ChatQuery;

#[Object]
impl ChatQuery {
    async fn chat_items(
        &self,
        ctx: &Context<'_>,
        target: String,
        offset: i32,
        limit: i32,
        query: String,
    ) -> Vec<ChatItem> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        c.db.get_chats_page(&target, &query, offset, limit)
            .into_iter()
            .map(|chat| ChatItem::from(chat))
            .collect()
    }

    async fn chat_item(&self, ctx: &Context<'_>, id: ID) -> Option<ChatItem> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        c.db.get_chat_by_id(id.as_str())
            .map(|chat| ChatItem::from(chat))
    }

    async fn chat_channels(&self, ctx: &Context<'_>) -> Vec<ChatChannel> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        c.db.get_channels(ChannelStatus::Joined)
            .into_iter()
            .map(ChatChannel::from)
            .collect()
    }

    async fn peers(&self, ctx: &Context<'_>) -> Vec<Peer> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        c.db.get_peers()
            .into_iter()
            .map(|p| {
                let online = c.peer_status.is_online(&p.id);
                Peer::from_dpeer(p, online)
            })
            .collect()
    }

    async fn latest_chat_items(&self, ctx: &Context<'_>) -> Vec<ChatItem> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        c.db.get_all_latest_chats()
            .into_iter()
            .map(|chat| ChatItem::from(chat))
            .collect()
    }
}
