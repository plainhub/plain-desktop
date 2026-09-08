//! Shared types, WebSocket event infrastructure, and resolver context.

use plain_rs::base64_decode;
use crate::commands::discover::{NearbyDiscoverManager, PeerStatusManager};
use crate::local::chat_cacher::ChatCacher;
use crate::local::db::ChatDb;
use crate::local::enums::ChannelStatus;
use crate::local::pairing::PairingManager;
use crate::prefs::AppIdentity;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::sync::atomic::AtomicU16;
use tauri::AppHandle;
use tokio::sync::broadcast;

pub const WS_MESSAGE_CREATED: i32 = 1;
pub const WS_MESSAGE_DELETED: i32 = 2;
pub const WS_MESSAGE_UPDATED: i32 = 3;
pub const WS_BOOKMARK_UPDATED: i32 = 15;
pub const WS_CHANNELS_UPDATED: i32 = 18;
pub const WS_PEER_STATUS_UPDATED: i32 = 20;
pub const WS_DEVICE_NAME_UPDATED: i32 = 21;
/// Peer file download progress — payload is a JSON array of
/// `DownloadProgressItem` (id, messageId, downloaded, total, speed, status).
/// Mirrors plain-app's `EventType.DOWNLOAD_PROGRESS`. The web client maps
/// event type 16 to `download_progress` (see `app-socket.ts`).
pub const WS_DOWNLOAD_PROGRESS: i32 = 16;
/// Mirrors plain-app's `PairingRequestReceivedEvent` — fired when the local
/// pairing manager receives an incoming PAIR_REQUEST that the user must
/// accept or reject. Payload is a `PairingEvent` JSON object.
pub const WS_PAIRING_REQUEST_RECEIVED: i32 = 22;
/// Mirrors plain-app's `PairingSuccessEvent` — fired when a pairing
/// handshake completes successfully.
pub const WS_PAIRING_SUCCESS: i32 = 23;
/// Mirrors plain-app's `PairingFailedEvent` — fired when a pairing
/// handshake fails or is rejected by the remote device.
pub const WS_PAIRING_FAILED: i32 = 24;
/// Mirrors plain-app's `PairingCanceledEvent` — fired when an in-progress
/// pairing is cancelled by either side.
pub const WS_PAIRING_CANCELLED: i32 = 25;
pub const WS_PAIRING_STARTED: i32 = 26;
/// Emitted for each LAN device that replied to a discover broadcast.
/// Payload is a single `DiscoveredDevice` JSON object.
pub const WS_NEARBY_DEVICE_FOUND: i32 = 27;
pub const WS_CHANNEL_INVITE_RECEIVED: i32 = 28;
/// Mirrors plain-app's `StartNearbyDiscoveryEvent` — fired when the
/// `startDiscovery` mutation kicks off the background scan loop.
pub const WS_NEARBY_DISCOVERY_STARTED: i32 = 29;
/// Mirrors plain-app's `StopNearbyDiscoveryEvent` — fired when the
/// `stopDiscovery` mutation tears the background scan loop down.
pub const WS_NEARBY_DISCOVERY_STOPPED: i32 = 30;

#[derive(Clone, Debug)]
pub struct WsEvent {
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
pub fn load_key_cache(
    db: &ChatDb,
    peer_cache: &PeerKeyCache,
    channel_cache: &ChannelKeyCache,
) {
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

/// Encode a WsEvent for wire: [4-byte i32 BE event_type][xchacha encrypted payload].
/// Framing comes from the shared plain_rs::ws_frame codec.
pub fn encode_ws_event(ev: &WsEvent, token: &str) -> Option<Vec<u8>> {
    plain_rs::ws_frame::encode_with_token(ev.event_type, ev.payload.as_bytes(), token)
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
            let members: Vec<serde_json::Value> =
                crate::local::channel::messages::decode_members(&ch.members)
                    .into_iter()
                    .map(|m| {
                        serde_json::json!({
                            "id": m.id,
                            "status": m.status.to_string(),
                        })
                    })
                    .collect();
            serde_json::json!({
                "id": ch.id,
                "name": ch.name,
                "owner": ch.owner,
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

/// All server-level dependencies bundled for injection into async-graphql resolvers.
/// Passed per-request via `Request::data(Arc<AppCtx>)`.
pub struct AppCtx {
    pub db: Arc<ChatDb>,
    pub identity: Arc<AppIdentity>,
    pub peer_status: PeerStatusManager,
    pub discover_manager: NearbyDiscoverManager,
    pub pairing_manager: PairingManager,
    pub peer_key_cache: PeerKeyCache,
    pub channel_key_cache: ChannelKeyCache,
    #[allow(dead_code)]
    pub chat_cacher: Arc<ChatCacher>,
    pub dlna_engine: Arc<crate::local::dlna::receiver_engine::DlnaEngine>,
    pub event_tx: broadcast::Sender<WsEvent>,
    pub token: String,
    pub port: Arc<AtomicU16>,
    pub https_port: Arc<AtomicU16>,
    /// App data directory — used by debug resolvers to read prefs.json.
    pub data_dir: std::path::PathBuf,
    /// App log directory — used by debug resolvers to read/clear plain.log.
    pub log_dir: std::path::PathBuf,
    /// Mutable device display name — updated by the updateDeviceName mutation.
    pub device_name: Arc<std::sync::RwLock<String>>,
    /// Tauri AppHandle — used by resolvers that need tauri_plugin_store.
    pub handle: AppHandle,
}
