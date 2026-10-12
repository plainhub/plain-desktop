use super::*;

#[cfg(feature = "api")]
#[path = "note/desktop.rs"]
mod desktop;

#[cfg(feature = "core")]
#[path = "note/public.rs"]
mod public;
