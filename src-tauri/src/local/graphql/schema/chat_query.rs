use async_graphql::{Context, Object};
use std::sync::Arc;

use super::super::context::AppCtx;
use super::types::{AppFile, ChatChannel, ChatItem, Peer};
use plain_rs::chat::app_file_store::{display_name, file_name_map};
use plain_rs::chat::enums::ChannelStatus;

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
            .map(|chat| ChatItem::with_data(chat, &c.token))
            .collect()
    }

    async fn chat_item(&self, ctx: &Context<'_>, id: String) -> Option<ChatItem> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        c.db.get_chat_by_id(&id)
            .map(|chat| ChatItem::with_data(chat, &c.token))
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
            .map(|chat| ChatItem::with_data(chat, &c.token))
            .collect()
    }

    async fn app_files(
        &self,
        ctx: &Context<'_>,
        offset: i32,
        limit: i32,
        query: String,
    ) -> Vec<AppFile> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        let files = c.db.get_app_file_page(limit, offset);
        let name_map = file_name_map(&c.db.get_all_chats());
        let text = query.trim();
        files
            .into_iter()
            .map(|f| {
                let display = display_name(&f, &name_map);
                AppFile::from_dappfile(f, display)
            })
            .filter(|f| text.is_empty() || f.file_name.contains(text))
            .collect()
    }

    async fn app_file_count(&self, ctx: &Context<'_>, query: String) -> i32 {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        let text = query.trim();
        if text.is_empty() {
            return c.db.count_app_files();
        }
        let name_map = file_name_map(&c.db.get_all_chats());
        c.db.get_all_app_files()
            .into_iter()
            .filter(|f| {
                let display = display_name(f, &name_map);
                display.contains(text)
            })
            .count() as i32
    }
}
