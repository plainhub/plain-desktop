//! XChaCha20-Poly1305 wrappers delegating to `plain-rs` (shared with
//! plain-desktop / plain-app), plus the password-hash helper.
//!
//! Wire format stays byte-for-byte compatible with the Go
//! `internal/strutils.ChaCha20Encrypt/Decrypt` helpers:
//!   * 24-byte random nonce
//!   * 32-byte key
//!   * no AAD
//!   * output: `nonce || ciphertext || tag(16)`
//!
//! `plain_rs::xchacha_encrypt_raw` / `xchacha_decrypt_raw` use exactly
//! this layout (`nonce || ciphertext` where the ciphertext includes the
//! tag), so the delegation is a pure re-plumb.

use anyhow::{Result, anyhow};

pub const KEY_LEN: usize = 32;

/// Encrypts plaintext with a random nonce and returns nonce||ct||tag.
pub fn encrypt(key: &[u8], plaintext: &[u8]) -> Result<Vec<u8>> {
    plain_rs::xchacha_encrypt_raw(key, plaintext)
        .ok_or_else(|| anyhow!("key must be {KEY_LEN} bytes"))
}

/// Decrypts nonce||ct||tag. Returns None on tag failure (matches Go which
/// returns `nil`).
pub fn decrypt(key: &[u8], blob: &[u8]) -> Option<Vec<u8>> {
    plain_rs::xchacha_decrypt_raw(key, blob)
}

/// Returns SHA-512(hex) of input - matches the web frontend's hash format
/// for the admin password.
pub fn sha512_hex(input: &str) -> String {
    plain_rs::utils::hash::sha512_hex(input)
}

#[cfg(test)]
#[path = "../tests/unit/crypto.rs"]
mod tests;
