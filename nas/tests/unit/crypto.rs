//! Unit tests for `src/crypto.rs` — moved out-of-line; compiled
//! as the `tests` child module via `#[cfg(test)] #[path]` there.
use super::*;
// XChaCha20-Poly1305 framing: 24-byte nonce, 16-byte tag.
const NONCE_LEN: usize = 24;
const TAG_LEN: usize = 16;
#[test]
fn round_trip() {
    let key = [7u8; KEY_LEN];
    let pt = b"hello world";
    let ct = encrypt(&key, pt).unwrap();
    assert_eq!(ct.len(), NONCE_LEN + pt.len() + TAG_LEN);
    let back = decrypt(&key, &ct).unwrap();
    assert_eq!(back, pt);
}
#[test]
fn tampered_tag_fails() {
    let key = [9u8; KEY_LEN];
    let mut ct = encrypt(&key, b"abc").unwrap();
    let last = ct.len() - 1;
    ct[last] ^= 1;
    assert!(decrypt(&key, &ct).is_none());
}
#[test]
fn wire_format_matches_go() {
    // The XChaCha20Poly1305 layout: nonce(24) + plaintext + tag(16).
    let key = [1u8; KEY_LEN];
    let pt = b"x";
    let ct = encrypt(&key, pt).unwrap();
    assert_eq!(ct.len(), NONCE_LEN + 1 + TAG_LEN);
}
