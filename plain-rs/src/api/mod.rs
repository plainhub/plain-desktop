//! Shared API assembly state and GraphQL execution support.
//!
//! Host integration goes through [`ShellHooks`] — everything the stack
//! needs from its host (persisted preferences, UI notifications, app
//! metadata) instead of a windowing-framework handle.

pub mod context;
pub mod db;
pub mod enums;
pub mod executor;

pub use context::ShellHooks;

// Identity lives with the preferences engine (`prefs::identity`).
pub use crate::prefs::identity::{default_device_name, generate_identity, AppIdentity};
