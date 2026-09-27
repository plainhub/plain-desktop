//! Direct translation of plain-app `ChatCacher.kt`.
//!
//! Maintains an in-memory cache of the latest chat per conversation,
//! keyed by chat ID (channel ID, peer ID, or "local").
//!
//! ```kotlin
//! object ChatCacher {
//!     val latestChatMap = MutableStateFlow<Map<String, DChat>>(emptyMap())
//!     fun getLatestChat(chatId: String): DChat? = latestChatMap.value[chatId]
//!     suspend fun load() = withIO { ... }
//! }
//! ```

use std::collections::{HashMap, HashSet};
use std::sync::RwLock;

use crate::chat::db::{ChatDb, DChat};

pub struct ChatCacher {
    latest_chat_map: RwLock<HashMap<String, DChat>>,
}

impl ChatCacher {
    pub fn new() -> Self {
        Self {
            latest_chat_map: RwLock::new(HashMap::new()),
        }
    }

    /// `fun getLatestChat(chatId: String): DChat?`
    pub fn get_latest_chat(&self, chat_id: &str) -> Option<DChat> {
        self.latest_chat_map.read().unwrap().get(chat_id).cloned()
    }

    /// `suspend fun load() = withIO { ... }`
    ///
    /// Rebuild the cache from the database:
    /// 1. Fetch all peers and channels to build ID sets.
    /// 2. Fetch latest chats per conversation via `getAllLatestChats()`.
    /// 3. Map each chat to its conversation ID (channel / peer / "local").
    /// 4. Keep the most recently updated chat per conversation ID.
    pub fn load(&self, db: &ChatDb) {
        let peer_ids: HashSet<String> = db.get_peers().iter().map(|p| p.id.clone()).collect();
        let channel_ids: HashSet<String> =
            db.get_all_channels().iter().map(|c| c.id.clone()).collect();
        let latest_chats = db.get_all_latest_chats();

        let mut chat_cache: HashMap<String, DChat> = HashMap::new();
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
                let should_replace = match chat_cache.get(&chat_id) {
                    None => true,
                    Some(existing) => chat.updated_at > existing.updated_at,
                };
                if should_replace {
                    chat_cache.insert(chat_id, chat);
                }
            }
        }

        let mut map = self.latest_chat_map.write().unwrap();
        *map = chat_cache;
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
