#[cfg(feature = "system")]
pub mod desktop;

#[cfg(feature = "api")]
pub mod local;

#[cfg(feature = "host-api")]
pub mod ble_http;

#[cfg(feature = "host-api")]
pub mod ble_pairing;

#[cfg(feature = "host-api")]
pub mod discovery_advertisement;

#[cfg(feature = "host-api")]
pub mod mdns_runtime;

#[cfg(feature = "host-api")]
pub mod nearby_devices;

#[cfg(feature = "host-api")]
pub mod nearby_http;

#[cfg(feature = "host-api")]
pub mod pairing;

#[cfg(feature = "host-api")]
pub mod pairing_runtime;

#[cfg(feature = "host-api")]
pub mod pairing_timeout;
