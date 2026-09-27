//! Unit tests for `src/config.rs` — moved out-of-line; compiled
//! as the `tests` child module via `#[cfg(test)] #[path]` there.
use super::*;
#[test]
fn parses_simple() {
    let c = Config::parse("[server]\nhttp_port = 8080\nhttps_port = 8443\n");
    assert_eq!(c.get_string("server.http_port"), "8080");
    assert_eq!(c.get_int("server.https_port"), 8443);
}
#[test]
fn comments_and_quotes() {
    let c = Config::parse("a = \"x\"  # comment\nb = 'y'\n");
    assert_eq!(c.get_string("a"), "x");
    assert_eq!(c.get_string("b"), "y");
}
