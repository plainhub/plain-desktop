//! The desktop app's embedded local API stack, extracted from
//! plain-desktop's `src-tauri/local/*`: async-graphql schema + executor,
//! hand-rolled HTTP/WS/TLS server, chat wiring, peer status, nearby
//! discovery, DLNA receiver, downloads and link previews. The Tauri
//! crate consumes this and stays a thin shell (commands + capture).
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
pub mod link_preview;
pub mod peer_graphql;
pub mod schema;
pub mod server;
pub mod tls;

pub use context::ShellHooks;

// Identity lives with the preferences engine (`prefs::identity`); keep
// the historical `api::` paths working for the host shells.
pub use crate::prefs::identity::{AppIdentity, default_device_name, generate_identity};
