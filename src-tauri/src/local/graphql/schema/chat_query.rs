use async_graphql::{Context, Object};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

use super::super::context::AppCtx;
use super::types::{AppFile, ChatChannel, ChatItem, Peer};
use plain_rs::mime::mime_extension;

#[derive(Default)]
pub struct ChatQuery;

fn build_app_file_name_map(chats: &[crate::local::db::DChat]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let mut sorted: Vec<&crate::local::db::DChat> = chats.iter().collect();
    sorted.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    for chat in sorted {
        let Ok(v) = serde_json::from_str::<Value>(&chat.content) else {
            continue;
        };
        let Some(items) = v.get("value").and_then(|vv| vv.get("items")).and_then(|i| i.as_array()) else {
            continue;
        };
        for item in items {
            let (Some(uri), Some(name)) = (
                item.get("uri").and_then(|u| u.as_str()),
                item.get("fileName").and_then(|n| n.as_str()),
            ) else {
                continue;
            };
            if !uri.starts_with("fid:") || name.is_empty() {
                continue;
            }
            let hash = uri.strip_prefix("fid:").unwrap_or(uri);
            let key = hash.split('.').next().unwrap_or(hash);
            map.entry(key.to_string()).or_insert_with(|| name.to_string());
        }
    }
    map
}

fn resolve_display_name(file: &crate::local::db::DAppFile, name_map: &HashMap<String, String>) -> String {
    let from_chat = name_map.get(&file.id).cloned().unwrap_or_default().trim().to_string();
    if !from_chat.is_empty() {
        return from_chat;
    }
    let ext = mime_extension(&file.mime_type);
    if ext == "bin" {
        "file".to_string()
    } else {
        format!("file.{ext}")
    }
}

#[Object]
impl ChatQuery {
    async fn chat_items(&self, ctx: &Context<'_>, id: String) -> Vec<ChatItem> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        c.db
            .get_chats(&id)
            .into_iter()
            .map(|chat| ChatItem::with_data(chat, &c.token))
            .collect()
    }

    async fn chat_item(&self, ctx: &Context<'_>, id: String) -> Option<ChatItem> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        c.db
            .get_chat_by_id(&id)
            .map(|chat| ChatItem::with_data(chat, &c.token))
    }

    async fn chat_channels(&self, ctx: &Context<'_>) -> Vec<ChatChannel> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        c.db.get_channels(crate::local::enums::ChannelStatus::Joined)
            .into_iter()
            .map(ChatChannel::from)
            .collect()
    }

    async fn peers(&self, ctx: &Context<'_>) -> Vec<Peer> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        c.db
            .get_peers()
            .into_iter()
            .map(|p| {
                let online = c.peer_status.is_online(&p.id);
                Peer::from_dpeer(p, online)
            })
            .collect()
    }

    async fn latest_chat_items(&self, ctx: &Context<'_>) -> Vec<ChatItem> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        c.db
            .get_all_latest_chats()
            .into_iter()
            .map(|chat| ChatItem::with_data(chat, &c.token))
            .collect()
    }

    async fn app_files(&self, ctx: &Context<'_>, offset: i32, limit: i32) -> Vec<AppFile> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        let files = c.db.get_app_file_page(limit, offset);
        let name_map = build_app_file_name_map(&c.db.get_all_chats());
        files
            .into_iter()
            .map(|f| {
                let display = resolve_display_name(&f, &name_map);
                AppFile::from_dappfile(f, display)
            })
            .collect()
    }

    async fn app_file_count(&self, ctx: &Context<'_>) -> i32 {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        c.db.count_app_files()
    }
}
