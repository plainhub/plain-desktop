#[cfg(any(feature = "api", feature = "host-api"))]
pub mod note;

#[cfg(feature = "host-api")]
pub mod public;
