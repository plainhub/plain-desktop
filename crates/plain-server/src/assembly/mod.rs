//! Shared API assembly state and GraphQL execution support.
//!
//! Host integration goes through [`ShellHooks`] — everything the stack
//! needs from its host (persisted preferences, UI notifications, app
//! metadata) instead of a windowing-framework handle.

#[cfg(feature = "api")]
pub mod context;
#[cfg(feature = "api")]
pub mod db;
#[cfg(any(feature = "api", feature = "core"))]
pub mod enums;
#[cfg(feature = "api")]
pub mod executor;

pub mod events;

#[cfg(feature = "api")]
pub use context::ShellHooks;

// Identity lives with the preferences engine (`prefs::identity`).
pub use plain_rs::prefs::identity::AppIdentity;
pub use plain_rs::prefs::identity::default_device_name;
pub use plain_rs::prefs::identity::generate_identity;

#[cfg(all(feature = "core", feature = "http_transport"))]
pub mod lifecycle;

#[cfg(feature = "core")]
pub mod server;

#[cfg(feature = "headless")]
pub mod headless;

#[cfg(feature = "core")]
pub mod services;

#[cfg(any(feature = "api", feature = "core"))]
pub(crate) mod shared_services;

#[cfg(feature = "core")]
pub(crate) mod core_dependencies;
