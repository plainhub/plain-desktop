#[cfg(any(feature = "api", feature = "host-api"))]
pub mod media_alias;

#[cfg(any(feature = "api", feature = "host-api"))]
pub mod receiver;

#[cfg(any(feature = "system", feature = "host-api"))]
pub mod sender;

#[cfg(feature = "host-api")]
pub mod cast_runtime;

#[cfg(feature = "host-api")]
pub mod dlna_sender_playback;

#[cfg(feature = "host-api")]
pub mod dlna_sender_runtime;

#[cfg(feature = "host-api")]
pub mod dlna_sender_scan;
