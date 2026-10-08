use std::collections::{HashMap, HashSet};
use std::sync::RwLock;

use crate::db::{
    DChat, Db,
    chat_store::{channels, messages, peers},
};
use anyhow::Result;

pub struct ChatCacher {
    latest_chat_map: RwLock<HashMap<String, DChat>>,
}

impl ChatCacher {
    pub fn new() -> Self {
        Self {
            latest_chat_map: RwLock::new(HashMap::new()),
        }
    }

    pub fn get_latest_chat(&self, chat_id: &str) -> Option<DChat> {
        self.latest_chat_map.read().unwrap().get(chat_id).cloned()
    }

    pub fn snapshot(db: &Db) -> Result<HashMap<String, DChat>> {
        let peer_ids: HashSet<String> = peers::all(db)?.into_iter().map(|p| p.id).collect();
        let channel_ids: HashSet<String> = channels::all(db)?.into_iter().map(|c| c.id).collect();
        let latest_chats: Vec<DChat> = serde_json::from_value(messages::list(
            db,
            &messages::Filter {
                peer: None,
                channel: None,
                text: String::new(),
                offset: 0,
                limit: None,
                descending: true,
                latest: true,
                count_only: false,
            },
        )?)?;
        let mut chat_cache = HashMap::<String, DChat>::new();
        for chat in latest_chats {
            let chat_id = if !chat.channel_id.is_empty() && channel_ids.contains(&chat.channel_id) {
                Some(chat.channel_id.clone())
            } else if (chat.from_id == "me" && chat.to_id == "local")
                || (chat.from_id == "local" && chat.to_id == "me")
            {
                Some("local".to_string())
            } else if chat.from_id == "me" && peer_ids.contains(&chat.to_id) {
                Some(chat.to_id.clone())
            } else if chat.to_id == "me" && peer_ids.contains(&chat.from_id) {
                Some(chat.from_id.clone())
            } else {
                None
            };
            if let Some(chat_id) = chat_id {
                let updated_at = chrono::DateTime::parse_from_rfc3339(&chat.updated_at)?;
                let should_replace = match chat_cache.get(&chat_id) {
                    None => true,
                    Some(existing) => {
                        updated_at > chrono::DateTime::parse_from_rfc3339(&existing.updated_at)?
                    }
                };
                if should_replace {
                    chat_cache.insert(chat_id, chat);
                }
            }
        }
        Ok(chat_cache)
    }

    pub fn load(&self, db: &Db) {
        match Self::snapshot(db) {
            Ok(snapshot) => *self.latest_chat_map.write().unwrap() = snapshot,
            Err(error) => log::error!("[chat] latest conversation snapshot failed: {error}"),
        }
    }
}

impl Default for ChatCacher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "../../tests/unit/chat/cacher.rs"]
mod tests;
