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
pub mod peer_graphql;
pub mod schema;
pub mod server;
pub mod temp_store;
pub mod tls;

pub use context::ShellHooks;

// Identity lives with the preferences engine (`prefs::identity`); keep
// the historical `api::` paths working for the host shells.
pub use crate::prefs::identity::{AppIdentity, default_device_name, generate_identity};
