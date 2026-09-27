//! Hash helpers shared across the Plain* projects.
//!
//! `sha512_hex` matches the web frontends' admin-password hash format:
//! lowercase hex of the SHA-512 digest of the UTF-8 password.

use sha2::{Digest, Sha512};

use super::hex::bytes_to_hex;

/// Returns SHA-512(input) as lowercase hex.
pub fn sha512_hex(input: &str) -> String {
    let mut h = Sha512::new();
    h.update(input.as_bytes());
    bytes_to_hex(&h.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha512_of_empty() {
        // sha512("") = cf83e135…da3e
        assert_eq!(
            sha512_hex(""),
            "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e"
        );
    }

    #[test]
    fn sha512_known_value() {
        // sha512("abc")
        assert_eq!(
            sha512_hex("abc"),
            "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f"
        );
    }
}
