//! Shared types, WebSocket event infrastructure, and resolver context.

use crate::commands::discover::{NearbyDiscoverManager, PeerStatusManager};
use crate::local::chat::ChatState;
use crate::local::db::ChatDb;
use crate::prefs::AppIdentity;
use std::sync::Arc;
use std::sync::atomic::AtomicU16;
use tauri::AppHandle;
use tokio::sync::broadcast;

pub use plain_rs::chat::events::{WS_MESSAGE_UPDATED, WS_PEER_STATUS_UPDATED};

pub const WS_BOOKMARK_UPDATED: i32 = 15;
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
pub const WS_NEARBY_DEVICE_UNREACHABLE: i32 = 46;
/// Mirrors plain-app's `StartNearbyDiscoveryEvent` — fired when the
/// `startDiscovery` mutation kicks off the background scan loop.
pub const WS_NEARBY_DISCOVERY_STARTED: i32 = 29;
/// Mirrors plain-app's `StopNearbyDiscoveryEvent` — fired when the
/// `stopDiscovery` mutation tears the background scan loop down.
pub const WS_NEARBY_DISCOVERY_STOPPED: i32 = 30;
/// Result of an async chunk merge started by `mergeChunksAsync`. Payload is a
/// JSON object `{fileId, ok, value?, mergedSize?, error?}`. plain-app's event
/// enum occupies 1..=37 (with gaps), so this contract appends at 38.
pub const WS_UPLOAD_MERGE_RESULT: i32 = 38;

#[derive(Clone, Debug)]
pub struct WsEvent {
    pub event_type: i32,
    pub payload: String,
}

/// Encode a WsEvent for wire: [4-byte i32 BE event_type][xchacha encrypted payload].
/// Framing comes from the shared plain_rs::ws_frame codec.
pub fn encode_ws_event(ev: &WsEvent, token: &str) -> Option<Vec<u8>> {
    plain_rs::ws_frame::encode_with_token(ev.event_type, ev.payload.as_bytes(), token)
}

/// All server-level dependencies bundled for injection into async-graphql resolvers.
/// Passed per-request via `Request::data(Arc<AppCtx>)`.
pub struct AppCtx {
    pub db: Arc<ChatDb>,
    pub identity: Arc<AppIdentity>,
    pub peer_status: PeerStatusManager,
    pub discover_manager: NearbyDiscoverManager,
    /// The assembled chat stack (service + pairing manager + key caches).
    pub chat: Arc<ChatState>,
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
