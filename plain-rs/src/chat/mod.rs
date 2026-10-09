//! Shared chat domain, extracted from plain-desktop's Rust port of the
//! plain-app chat stack (Kotlin `ChatManager` / `ChatSender` /
//! `PairingCore` / `ChannelSystemMessage*`).
//!
//! What lives here, in rough layers:
//! - [`enums`] — the wire enums (chat/peer/channel status, device type).
//! - [`pairing`] — LAN pairing wire protocol (`PAIR_REQUEST:` /
//!   `PAIR_RESPONSE:` / `PAIR_CANCEL:` over HTTPS `POST /nearby`).
//! - [`channel`] — group-chat system-message wire types + canonical
//!   signature payloads.
//! - [`crate::db`] — the SQLite storage layer, same schema as plain-app's
//!   Room DB (`plain.db`: chats / chat_channels / peers /
//!   nearby_device_cache / app_files / bookmarks / bookmark_groups).
//! - files/cacher/service — content-addressed attachments, the
//!   latest-chat cache, and the send/receive service.
//!
//! App-specific concerns (GraphQL resolvers, HTTP/WS servers, mDNS
//! discovery wiring, link-preview scraping) stay in each consumer and
//! plug in through small traits.

pub mod app_file_store;
pub mod attachment_imports;
pub mod cacher;
pub mod channel;
pub mod content;
pub mod delivery;
pub mod share_card;

pub mod discovery_advertisement;
pub mod enums;
pub mod events;
mod manager;
pub mod message_lifecycle;
#[cfg(any(feature = "api", feature = "content_api"))]
pub mod nearby_http;
pub mod nearby_wire;
pub mod pairing;
pub mod peer_auth;
mod peer_manager;
pub mod prewarm;
mod receiver;
mod sender;
pub mod service;
pub mod transport;
pub mod transport_router;

pub mod download_queue;
pub mod download_status;

pub mod lan_ip;
pub mod nearby_scan;

pub mod nearby_devices;

pub mod peer_status;

pub mod message_commands;

pub mod share_send;
