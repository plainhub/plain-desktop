//! App version metadata surfaced by the NAS `deviceInfo` / `appUpdate`
//! queries. plain-rs carries no build-time git/build info of its own —
//! the shell binary (whose build.rs knows the commit and build time)
//! injects its values via [`set`] at startup, before any resolver reads
//! them.

use std::sync::OnceLock;

static VERSION: OnceLock<String> = OnceLock::new();
static FULL_VERSION: OnceLock<String> = OnceLock::new();

pub fn set(version: &str, commit: &str, build_time: &str) {
    let _ = VERSION.set(version.to_string());
    let _ = FULL_VERSION.set(format!("{version} (commit {commit}, built {build_time})"));
}

pub fn version() -> &'static str {
    VERSION
        .get()
        .map(String::as_str)
        .unwrap_or(env!("CARGO_PKG_VERSION"))
}

pub fn full_version() -> String {
    FULL_VERSION
        .get()
        .cloned()
        .unwrap_or_else(|| env!("CARGO_PKG_VERSION").to_string())
}
