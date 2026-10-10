//! Unit tests for `src/app_update.rs` — moved out-of-line; compiled
//! as the `tests` child module via `#[cfg(test)] #[path]` there.
use super::*;

#[test]
fn normalize_strips_prefix_and_meta() {
    assert_eq!(normalize_version("v1.2.3"), "1.2.3");
    assert_eq!(normalize_version("PlainNAS 1.2.3"), "1.2.3");
    assert_eq!(normalize_version("1.2.3-beta.1"), "1.2.3");
    assert_eq!(normalize_version("1.2.3+abc"), "1.2.3");
    assert_eq!(normalize_version("  v1.2.3  "), "1.2.3");
}

#[test]
fn parse_semver_basic() {
    assert_eq!(parse_semver("v1.2.3"), Some([1, 2, 3]));
    assert_eq!(parse_semver("1.2.3"), Some([1, 2, 3]));
    assert_eq!(parse_semver("1.2"), None);
    assert_eq!(parse_semver(""), None);
}

#[test]
fn has_newer_basic() {
    assert!(has_newer_version("1.2.3", "1.2.4"));
    assert!(!has_newer_version("1.2.3", "1.2.3"));
    assert!(!has_newer_version("1.2.4", "1.2.3"));
    assert!(has_newer_version("1.2.9", "1.3.0"));
    assert!(has_newer_version("1.9.9", "2.0.0"));
    assert!(!has_newer_version("", "1.0.0"));
    assert!(!has_newer_version("1.0.0", ""));
}

#[test]
fn has_newer_fallback_non_semver() {
    // Fallback: if both non-empty and different, treat as newer.
    assert!(has_newer_version("dev", "any-later-tag"));
    assert!(!has_newer_version("dev", "dev"));
}
