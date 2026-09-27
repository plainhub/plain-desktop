//! Unit tests for `src/crypto.rs` — moved out-of-line; compiled
//! as the `tests` child module via `#[cfg(test)] #[path]` there.
//! The XChaCha20 wrappers moved to plain-rs with the HTTP layer; what
//! remains here is the password hash helper.
use super::*;

#[test]
fn sha512_hex_matches_frontend_format() {
    let h = sha512_hex("password");
    assert_eq!(h.len(), 128);
    assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
    assert_eq!(sha512_hex("password"), h);
    assert_ne!(sha512_hex("other"), h);
}
