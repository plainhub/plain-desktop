//! Unit tests for `src/search_index.rs` — moved out-of-line; compiled
//! as the `tests` child module via `#[cfg(test)] #[path]` there.
use super::*;

#[test]
fn parse_size_value_plain() {
    assert_eq!(parse_size_value("1024").unwrap(), 1024);
}

#[test]
fn parse_size_value_kb() {
    assert_eq!(parse_size_value("10KB").unwrap(), 10240);
}

#[test]
fn parse_size_value_mb() {
    assert_eq!(parse_size_value("5MB").unwrap(), 5_242_880);
}

#[test]
fn parse_size_value_gb() {
    assert_eq!(parse_size_value("1GB").unwrap(), 1_073_741_824);
}
