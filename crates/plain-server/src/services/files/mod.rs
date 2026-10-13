#[cfg(feature = "core")]
pub mod file_access;

#[cfg(feature = "core")]
pub mod file_task_media;

#[cfg(feature = "core")]
pub mod file_task_walk;

#[cfg(feature = "core")]
pub mod file_tasks;

#[cfg(feature = "core")]
pub mod media_buckets;

#[cfg(feature = "core")]
pub mod writes;

#[cfg(feature = "core")]
pub mod media_actions;

#[cfg(feature = "core")]
pub mod upload_chunks;

#[cfg(feature = "core")]
pub mod archives;

#[cfg(feature = "core")]
pub(crate) mod media_catalog;

#[cfg(feature = "core")]
pub(crate) mod media_catalog_facts;

#[cfg(all(feature = "core", feature = "media"))]
pub mod media_scans;
#[cfg(feature = "media")]
pub mod media_watch;

#[cfg(all(feature = "core", feature = "media"))]
pub(crate) mod media_scan_policy;

#[cfg(any(feature = "api", feature = "mounts"))]
pub mod mounts;

#[cfg(feature = "core")]
pub(crate) mod zip_entries;
