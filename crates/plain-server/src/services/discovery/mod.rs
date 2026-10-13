#[cfg(feature = "system")]
pub mod desktop;

#[cfg(any(feature = "api", feature = "core"))]
pub mod local;

#[cfg(feature = "core")]
pub mod ble_http;

#[cfg(feature = "core")]
pub mod ble_pairing;

#[cfg(feature = "core")]
pub mod ble_scan;

#[cfg(feature = "core")]
pub mod discovery_advertisement;

#[cfg(feature = "core")]
pub mod mdns_runtime;

#[cfg(feature = "core")]
pub mod nearby_devices;

#[cfg(feature = "core")]
pub mod nearby_http;

#[cfg(feature = "core")]
pub mod pairing;

#[cfg(feature = "core")]
pub mod pairing_runtime;

#[cfg(feature = "core")]
pub mod pairing_timeout;
