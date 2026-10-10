use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::base64_decode;

use crate::chat::channel::messages::decode_members;
use crate::chat::enums::ChannelStatus;
use crate::db::Db;

pub const WS_MESSAGE_CREATED: &'static str = "MESSAGE_CREATED";
pub const WS_MESSAGE_DELETED: &'static str = "MESSAGE_DELETED";
pub const WS_MESSAGE_UPDATED: &'static str = "MESSAGE_UPDATED";
pub const WS_DOWNLOAD_PROGRESS: &'static str = "DOWNLOAD_PROGRESS";
pub const WS_CHANNELS_UPDATED: &'static str = "CHANNELS_UPDATED";
pub const WS_PEER_STATUS_UPDATED: &'static str = "PEER_STATUS_UPDATED";
pub const WS_PAIRING_REQUEST_RECEIVED: &'static str = "PAIRING_REQUEST_RECEIVED";
pub const WS_PAIRING_SUCCESS: &'static str = "PAIRING_SUCCESS";
pub const WS_PAIRING_CANCELLED: &'static str = "PAIRING_CANCELED";
pub const WS_PAIRING_STARTED: &'static str = "PAIRING_STARTED";
pub const WS_PAIRING_FAILED: &'static str = "PAIRING_FAILED";
pub const WS_NEARBY_DEVICE_FOUND: &'static str = "NEARBY_DEVICE_FOUND";
pub const WS_CHANNEL_INVITE_RECEIVED: &'static str = "CHANNEL_INVITE_RECEIVED";

#[derive(Clone, Debug)]
pub struct ChatEvent {
    pub event_type: &'static str,
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
pub fn refresh_peer_key_cache(db: &Db, cache: &PeerKeyCache) {
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
pub fn load_key_cache(db: &Db, peer_cache: &PeerKeyCache, channel_cache: &ChannelKeyCache) {
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

/// Public channel projection shared by GraphQL and WebSocket events.
pub fn channel_to_json(ch: &crate::db::DChannel) -> serde_json::Value {
    let members: Vec<serde_json::Value> = decode_members(&ch.members)
        .into_iter()
        .map(|m| serde_json::json!({"peerId":m.peer_id,"status":m.status.to_string()}))
        .collect();
    serde_json::json!({"id":ch.id,"name":ch.name,"ownerId":ch.owner_id,"members":members,
        "version":ch.version,"status":ch.status.to_string(),"createdAt":ch.created_at,"updatedAt":ch.updated_at})
}

pub fn public_channels(db: &Db) -> anyhow::Result<serde_json::Value> {
    let mut channels = crate::db::chat_store::channels::all(db)?;
    channels.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(serde_json::Value::Array(
        channels.iter().map(channel_to_json).collect(),
    ))
}

pub fn channels_updated_payload(db: &Db) -> String {
    let channels = db.get_channels(ChannelStatus::Joined);
    serde_json::to_string(&channels.iter().map(channel_to_json).collect::<Vec<_>>())
        .unwrap_or_else(|_| "[]".to_string())
}
