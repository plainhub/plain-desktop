//! Hash helpers shared across the Plain* projects.
//!
//! `sha512_hex` matches the web frontends' admin-password hash format:
//! lowercase hex of the SHA-512 digest of the UTF-8 password.

use sha2::{Digest, Sha512};

use crate::utils::hex::bytes_to_hex;

/// Returns SHA-512(input) as lowercase hex.
pub fn sha512_hex(input: &str) -> String {
    let mut h = Sha512::new();
    h.update(input.as_bytes());
    bytes_to_hex(&h.finalize())
}

#[cfg(test)]
#[path = "../../tests/unit/crypto/hash.rs"]
mod tests;
