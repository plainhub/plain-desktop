use super::*;

#[cfg(feature = "api")]
#[path = "feed/desktop.rs"]
mod desktop;

#[cfg(feature = "core")]
#[path = "feed/public.rs"]
mod public;
