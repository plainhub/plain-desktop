//! Version helpers. The Go side uses build-time ldflags to embed the
//! version. We do the same via `env!` lookups populated by `build.rs`.

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const COMMIT: &str = env!("PLAIN_NAS_GIT_COMMIT", "unknown");
pub const BUILD_TIME: &str = env!("PLAIN_NAS_BUILD_TIME", "unknown");
