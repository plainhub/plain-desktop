#[cfg(feature = "api")]
pub mod local;

#[cfg(feature = "core")]
pub mod system;

#[cfg(feature = "core")]
pub mod cors_host;

#[cfg(feature = "core")]
pub mod static_host;

#[cfg(feature = "core")]
pub mod graphql;

#[cfg(feature = "core")]
pub mod provider_query;

#[cfg(feature = "core")]
pub mod zip;

#[cfg(feature = "core")]
pub mod files;

#[cfg(feature = "core")]
pub mod file_reads;

#[cfg(feature = "core")]
pub mod file_writes;

#[cfg(feature = "core")]
pub mod file_mutations;

#[cfg(all(feature = "core", feature = "http_transport"))]
pub mod discovery;

#[cfg(feature = "api")]
pub mod peer_graphql;

#[cfg(feature = "core")]
pub(crate) mod public;
