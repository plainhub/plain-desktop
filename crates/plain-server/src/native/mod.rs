#[cfg(feature = "core")]
pub mod graphql;

#[cfg(any(feature = "api", feature = "core"))]
pub mod resources;

#[cfg(feature = "core")]
pub mod resource_stream;

#[cfg(feature = "core")]
pub mod file_tasks;

#[cfg(feature = "core")]
pub mod uploads;

#[cfg(feature = "core")]
pub mod archives;

#[cfg(feature = "core")]
pub mod pairing;

#[cfg(feature = "core")]
pub mod peers;

#[cfg(feature = "core")]
pub mod dlna;

#[cfg(feature = "core")]
pub mod discovery;

#[cfg(feature = "core")]
pub mod media_catalog;
