//! Chat WebSocket events + key caches — the shared event vocabulary of
//! the chat stack. Event types use plain-app's wire numbers so the same
//! web client works against any server.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::base64_decode;

use crate::chat::channel::messages::decode_members;
use crate::chat::db::ChatDb;
use crate::chat::enums::ChannelStatus;

pub const WS_MESSAGE_CREATED: i32 = 1;
pub const WS_MESSAGE_DELETED: i32 = 2;
pub const WS_MESSAGE_UPDATED: i32 = 3;
pub const WS_CHANNELS_UPDATED: i32 = 18;
pub const WS_PEER_STATUS_UPDATED: i32 = 20;
pub const WS_CHANNEL_INVITE_RECEIVED: i32 = 28;

/// A chat-domain push event: wire framing (`[i32 BE event_type][encrypted
/// payload]`) comes from the shared `ws_frame` codec at the app's WS layer.
#[derive(Clone, Debug)]
pub struct ChatEvent {
    pub event_type: i32,
    pub payload: String,
}

pub type PeerKeyCache = Arc<RwLock<HashMap<String, Vec<u8>>>>;
pub type ChannelKeyCache = Arc<RwLock<HashMap<String, Vec<u8>>>>;

pub fn new_peer_key_cache() -> PeerKeyCache {
    Arc::new(RwLock::new(HashMap::new()))
}

pub fn new_channel_key_cache() -> ChannelKeyCache {
    Arc::new(RwLock::new(HashMap::new()))
}

/// Rebuild peer key cache from the DB. Call after any peers table mutation.
pub fn refresh_peer_key_cache(db: &ChatDb, cache: &PeerKeyCache) {
    let peers = db.get_peers();
    let mut map = cache.write().unwrap();
    map.clear();
    for p in peers {
        if !p.key.is_empty() && p.is_paired() {
            let raw = base64_decode(&p.key);
            if raw.len() == 32 {
                map.insert(p.id, raw);
            }
        }
    }
}

/// Rebuild both peer and channel key caches from the DB.
/// Mirrors `ChatCacheManager.loadKeyCacheAsync()` in plain-app.
pub fn load_key_cache(db: &ChatDb, peer_cache: &PeerKeyCache, channel_cache: &ChannelKeyCache) {
    refresh_peer_key_cache(db, peer_cache);

    let mut cm = channel_cache.write().unwrap();
    cm.clear();
    for ch in db.get_channels_with_key() {
        let raw = base64_decode(&ch.key);
        if raw.len() == 32 {
            cm.insert(ch.id, raw);
        }
    }
}

/// Serialize all joined channels into the wire format the web client's
/// `channels_updated` handler expects — a JSON array of channel models
/// with camelCase fields. Mirrors plain-app's `channelsToJsonModelString`
/// (`ChannelManager.kt`), which wraps `channels.map { it.toModel() }`.
pub fn channels_updated_payload(db: &ChatDb) -> String {
    let channels = db.get_channels(ChannelStatus::Joined);
    let arr: Vec<serde_json::Value> = channels
        .iter()
        .map(|ch| {
            let members: Vec<serde_json::Value> = decode_members(&ch.members)
                .into_iter()
                .map(|m| {
                    serde_json::json!({
                        "peerId": m.peer_id,
                        "status": m.status.to_string(),
                    })
                })
                .collect();
            serde_json::json!({
                "id": ch.id,
                "name": ch.name,
                "ownerId": ch.owner_id,
                "members": members,
                "version": ch.version,
                "status": ch.status.to_string(),
                "createdAt": ch.created_at,
                "updatedAt": ch.updated_at,
            })
        })
        .collect();
    serde_json::to_string(&arr).unwrap_or_else(|_| "[]".to_string())
}
