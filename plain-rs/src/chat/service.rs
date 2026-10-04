//! Chat service — the single place where chat items are created,
//! delivered, retried, and received from peers. Port of plain-desktop's
//! `chat_handler` (itself the port of plain-app `ChatManager` /
//! `ChatSender`).
//!
//! GraphQL resolvers (local and peer) deliberately contain no business
//! logic: they parse the wire arguments and delegate here. App-specific
//! side effects plug in through [`ChatHooks`], the [`LinkPreviewFn`]
//! async closure, and the [`PeerTransport`] type parameter.

use std::path::PathBuf;
use std::sync::Arc;

use futures_core::future::BoxFuture;
use serde_json::Value;
use tokio::sync::broadcast;

use crate::chat::cacher::ChatCacher;
use crate::chat::enums::DeviceType;
use crate::chat::events::{
    ChannelKeyCache, ChatEvent, PeerKeyCache, load_key_cache, new_channel_key_cache,
    new_peer_key_cache,
};
use crate::chat::transport::PeerTransport;
use crate::db::{DChat, Db};

/// Local device identity for pairing and message signing.
pub struct ChatIdentity {
    pub client_id: String,
    /// Display name — interior-mutable so a runtime rename (the
    /// `updateDeviceName` mutation) propagates to every reader of the
    /// shared `Arc<ChatIdentity>`.
    device_name: std::sync::RwLock<String>,
    /// Base64 64-byte Ed25519 keypair — same value as plain-app's
    /// `TempData.ed25519Keypair`.
    pub ed25519_keypair: String,
}

impl ChatIdentity {
    pub fn new(
        client_id: impl Into<String>,
        device_name: impl Into<String>,
        ed25519_keypair: impl Into<String>,
    ) -> Self {
        Self {
            client_id: client_id.into(),
            device_name: std::sync::RwLock::new(device_name.into()),
            ed25519_keypair: ed25519_keypair.into(),
        }
    }

    pub fn device_name(&self) -> String {
        self.device_name.read().unwrap().clone()
    }

    pub fn set_device_name(&self, name: &str) {
        *self.device_name.write().unwrap() = name.to_string();
    }
}

/// App-specific side effects of the chat flow.
pub trait ChatHooks: Send + Sync {
    /// Called after a failed peer delivery — usually means the peer's
    /// IP/port changed. plain-desktop kicks an mDNS re-browse here.
    fn rebrowse_peers(&self) {}
}

/// No-op hooks.
pub struct NoChatHooks;
impl ChatHooks for NoChatHooks {}

/// Fetch previews for a stored message ID and return the committed row.
/// The consumer must not rewrite content from an earlier snapshot.
pub type LinkPreviewFn =
    Arc<dyn Fn(Db, PathBuf, String) -> BoxFuture<'static, Option<DChat>> + Send + Sync>;

/// A `LinkPreviewFn` that never rewrites content.
pub fn no_link_previews() -> LinkPreviewFn {
    Arc::new(|_db, _data_dir, _content| Box::pin(async { None }))
}

/// All chat-domain state and the entry points of the business logic.
/// Generic over the app's HTTP transport to peers.
pub struct ChatService<T: PeerTransport> {
    pub db: Db,
    pub delivery: std::sync::Arc<super::delivery::Delivery>,
    pub cacher: ChatCacher,
    /// Base64 local URL token — the key behind `/fs` file ids.
    pub token: String,
    /// Shared with PairingManager so a device-name update propagates.
    pub identity: std::sync::Arc<ChatIdentity>,
    /// Device type advertised in channel wire traffic
    /// (COMPUTER on desktop, NAS on plain-nas).
    pub wire_device_type: DeviceType,
    /// App data directory (app-file store root).
    pub data_dir: PathBuf,
    pub transport: Arc<T>,
    pub event_tx: broadcast::Sender<ChatEvent>,
    pub peer_key_cache: PeerKeyCache,
    pub channel_key_cache: ChannelKeyCache,
    pub hooks: Arc<dyn ChatHooks>,
    pub link_previews: LinkPreviewFn,
}

impl<T: PeerTransport + 'static> ChatService<T> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        db: Db,
        token: String,
        identity: std::sync::Arc<ChatIdentity>,
        wire_device_type: DeviceType,
        data_dir: PathBuf,
        transport: Arc<T>,
        hooks: Arc<dyn ChatHooks>,
        link_previews: LinkPreviewFn,
    ) -> Self {
        let (event_tx, _) = broadcast::channel(256);
        let peer_key_cache = new_peer_key_cache();
        let channel_key_cache = new_channel_key_cache();
        let cacher = ChatCacher::new();
        cacher.load(&db);
        load_key_cache(&db, &peer_key_cache, &channel_key_cache);
        Self {
            delivery: std::sync::Arc::new(super::delivery::Delivery::new(db.clone())),
            db,
            cacher,
            token,
            identity,
            wire_device_type,
            data_dir,
            transport,
            event_tx,
            peer_key_cache,
            channel_key_cache,
            hooks,
            link_previews,
        }
    }

    pub(super) fn emit(&self, event_type: i32, payload: String) {
        let _ = self.event_tx.send(ChatEvent {
            event_type,
            payload,
        });
    }

    /// Build the wire JSON for a single chat item. Used for both GraphQL
    /// response data and `WS_MESSAGE_CREATED` / `WS_MESSAGE_UPDATED`
    /// payloads. File ids are no longer embedded — clients derive them
    /// from `content` with their own urlToken.
    pub fn chat_to_json(&self, c: &DChat) -> Value {
        crate::chat::manager::chat_to_json(c)
    }
}

pub use crate::chat::content::to_peer_content;
pub use crate::chat::manager::{chat_to_json, resolve_chat_ids};

#[cfg(test)]
#[path = "../../tests/unit/chat/service.rs"]
mod tests;
