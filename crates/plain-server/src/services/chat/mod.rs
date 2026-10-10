#[cfg(feature = "api")]
pub mod desktop;

#[cfg(feature = "host-api")]
pub mod attachment_imports;

#[cfg(feature = "host-api")]
pub mod channel_delivery;

#[cfg(feature = "host-api")]
pub mod channel_outgoing;

#[cfg(feature = "host-api")]
pub mod channel_runtime;

#[cfg(feature = "host-api")]
pub mod chat_actions;

#[cfg(feature = "host-api")]
pub mod chat_delivery;

#[cfg(feature = "host-api")]
pub mod chat_service;

#[cfg(feature = "host-api")]
pub mod chat_store_routes;

#[cfg(feature = "host-api")]
pub mod download_queue;

#[cfg(feature = "host-api")]
pub mod link_preview;

#[cfg(feature = "host-api")]
pub mod prewarm;
