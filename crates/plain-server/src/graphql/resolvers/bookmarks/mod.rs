#[cfg(any(feature = "api", feature = "core"))]
pub mod bookmark;

#[cfg(any(feature = "api", feature = "core"))]
pub mod bookmark_types;

#[cfg(feature = "core")]
pub mod metadata;
