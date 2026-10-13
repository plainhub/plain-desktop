//! WebSocket event names shared by every host surface.
//!
//! These are re-exported from `plain_rs` wherever that crate already owns
//! the name; the few that only ever exist on the server live here so the
//! desktop, NAS and mobile hosts all emit identical wire strings.

pub use plain_rs::chat::events::WS_MESSAGE_UPDATED;
pub use plain_rs::chat::events::WS_PEER_STATUS_UPDATED;

pub use plain_rs::events::WS_BOOKMARK_UPDATED;
pub const WS_DEVICE_NAME_UPDATED: &'static str = "DEVICE_NAME_UPDATED";
/// Peer file download progress — payload is a JSON array of
/// `DownloadProgressItem` (id, messageId, downloaded, total, speed, status).
/// Mirrors plain-app's `EventType.DOWNLOAD_PROGRESS`.
pub use plain_rs::chat::events::WS_DOWNLOAD_PROGRESS;
/// Emitted for each LAN device that replied to a discover broadcast.
/// Payload is a single `DiscoveredDevice` JSON object.
pub use plain_rs::chat::events::WS_NEARBY_DEVICE_FOUND;
/// Mirrors plain-app's `PairingCanceledEvent` — fired when an in-progress
/// pairing is cancelled by either side.
pub use plain_rs::chat::events::WS_PAIRING_CANCELLED;
/// Mirrors plain-app's `PairingFailedEvent` — fired when a pairing
/// handshake fails or is rejected by the remote device.
pub use plain_rs::chat::events::WS_PAIRING_FAILED;
/// Mirrors plain-app's `PairingRequestReceivedEvent` — fired when the local
/// pairing manager receives an incoming PAIR_REQUEST that the user must
/// accept or reject. Payload is a `PairingEvent` JSON object.
pub use plain_rs::chat::events::WS_PAIRING_REQUEST_RECEIVED;
pub use plain_rs::chat::events::WS_PAIRING_STARTED;
/// Mirrors plain-app's `PairingSuccessEvent` — fired when a pairing
/// handshake completes successfully.
pub use plain_rs::chat::events::WS_PAIRING_SUCCESS;
pub const WS_NEARBY_DEVICE_UNREACHABLE: &'static str = "NEARBY_DEVICE_UNREACHABLE";
/// Mirrors plain-app's `StartNearbyDiscoveryEvent` — fired when the
/// `startDiscovery` mutation kicks off the background scan loop.
pub const WS_NEARBY_DISCOVERY_STARTED: &'static str = "NEARBY_DISCOVERY_STARTED";
/// Mirrors plain-app's `StopNearbyDiscoveryEvent` — fired when the
/// `stopDiscovery` mutation tears the background scan loop down.
pub const WS_NEARBY_DISCOVERY_STOPPED: &'static str = "NEARBY_DISCOVERY_STOPPED";
/// Result of an async chunk merge started by `mergeChunksAsync`. Payload is a
/// JSON object `{fileId, ok, value?, mergedSize?, error?}`.
pub const WS_UPLOAD_MERGE_RESULT: &'static str = "UPLOAD_MERGE_RESULT";
pub use plain_rs::events::WS_IMAGE_EDITOR_UPDATE;
pub use plain_rs::events::WS_POMODORO_ACTION;

pub use plain_rs::events::WsEvent;