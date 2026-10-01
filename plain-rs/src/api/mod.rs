//! Shared API stack for the desktop and NAS shells: GraphQL schemas,
//! HTTP/WS/TLS server, reverse proxy, discovery, DLNA receiver,
//! downloads and link previews.
//!
//! Host integration goes through [`ShellHooks`] — everything the stack
//! needs from its host (persisted preferences, UI notifications, app
//! metadata) instead of a windowing-framework handle.

pub mod chat;
pub mod context;
pub mod db;
pub mod discover;
pub mod dlna;
pub mod download;
pub mod enums;
pub mod executor;
pub mod http_proxy;
pub mod link_preview;
pub mod server;
pub mod temp_store;
pub mod tls;

pub use context::ShellHooks;

// Identity lives with the preferences engine (`prefs::identity`).
pub use crate::prefs::identity::{default_device_name, generate_identity, AppIdentity};

#[cfg(feature = "system")]
pub mod chat_discovery;
