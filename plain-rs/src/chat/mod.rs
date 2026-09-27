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
//! - [`db`] — the SQLite storage layer, same schema as plain-app's
//!   Room DB (`local_chat.db`: chats / chat_channels / peers /
//!   nearby_device_cache / app_files / bookmarks / bookmark_groups).
//! - files/cacher/service — content-addressed attachments, the
//!   latest-chat cache, and the send/receive service.
//!
//! App-specific concerns (GraphQL resolvers, HTTP/WS servers, mDNS
//! discovery wiring, link-preview scraping) stay in each consumer and
//! plug in through small traits.

pub mod app_file_store;
pub mod cacher;
pub mod channel;
pub mod content;
pub mod db;
pub mod enums;
pub mod events;
pub mod pairing;
pub mod peer_auth;
pub mod service;
pub mod transport;
