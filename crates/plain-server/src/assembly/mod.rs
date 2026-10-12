//! Shared API assembly state and GraphQL execution support.
//!
//! Host integration goes through [`ShellHooks`] — everything the stack
//! needs from its host (persisted preferences, UI notifications, app
//! metadata) instead of a windowing-framework handle.

#[cfg(feature = "api")]
pub mod context;
#[cfg(feature = "api")]
pub mod db;
#[cfg(feature = "api")]
pub mod enums;
#[cfg(feature = "api")]
pub mod executor;

#[cfg(feature = "api")]
pub use context::ShellHooks;

// Identity lives with the preferences engine (`prefs::identity`).
pub use plain_rs::prefs::identity::AppIdentity;
pub use plain_rs::prefs::identity::default_device_name;
pub use plain_rs::prefs::identity::generate_identity;

#[cfg(all(feature = "host-api", feature = "http_transport"))]
pub mod lifecycle;

#[cfg(feature = "host-api")]
pub mod server;

#[cfg(feature = "host-api")]
pub mod services;

#[cfg(any(feature = "api", feature = "host-api"))]
pub(crate) mod shared_services;

#[cfg(feature = "host-api")]
pub(crate) mod core_dependencies;
