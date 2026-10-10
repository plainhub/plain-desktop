//! Password hashing helper delegating to `plain-rs` (the XChaCha20
//! wrappers moved there with the HTTP layer).

/// Returns SHA-512(hex) of input - matches the web frontend's hash format
/// for the admin password.
pub fn sha512_hex(input: &str) -> String {
    plain_server::utils::hash::sha512_hex(input)
}

#[cfg(test)]
#[path = "../tests/unit/crypto.rs"]
mod tests;
