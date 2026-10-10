#[cfg(feature = "api")]
pub mod local;

#[cfg(feature = "host-api")]
pub mod system;

#[cfg(feature = "host-api")]
pub mod cors_host;

#[cfg(feature = "host-api")]
pub mod static_host;

#[cfg(feature = "host-api")]
pub mod graphql;

#[cfg(feature = "host-api")]
pub mod provider_query;

#[cfg(feature = "host-api")]
pub mod zip;

#[cfg(feature = "host-api")]
pub mod files;

#[cfg(feature = "host-api")]
pub mod file_reads;

#[cfg(feature = "host-api")]
pub mod file_writes;

#[cfg(feature = "host-api")]
pub mod file_mutations;

#[cfg(all(feature = "host-api", feature = "http_transport"))]
pub mod discovery;

#[cfg(feature = "api")]
pub mod peer_graphql;
