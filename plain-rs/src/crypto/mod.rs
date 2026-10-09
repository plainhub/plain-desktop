mod ecdh;
mod ed25519;
pub mod hash;
mod symmetric;
pub use hash::sha512_hex;

#[cfg(test)]
#[path = "../../tests/unit/crypto/cross_platform_vectors.rs"]
mod cross_platform_vectors;

pub use crate::utils::base64::{base64_decode, base64_encode};
pub use ecdh::EcdhSession;
pub use ed25519::{ed25519_generate, ed25519_public_from_keypair, ed25519_sign, ed25519_verify};
pub use symmetric::{
    chacha20_decrypt, chacha20_encrypt, xchacha_decrypt, xchacha_decrypt_raw, xchacha_encrypt,
    xchacha_encrypt_raw,
};

pub fn gen_random(buf: &mut [u8]) {
    use rand::RngCore;
    rand::rngs::OsRng.fill_bytes(buf);
}

pub fn random_bytes(len: usize) -> Vec<u8> {
    let mut buf = vec![0u8; len];
    gen_random(&mut buf);
    buf
}

pub fn gen_token() -> String {
    let mut bytes = [0u8; 32];
    gen_random(&mut bytes);
    base64_encode(&bytes)
}

#[cfg(test)]
#[path = "../../tests/unit/crypto/mod.rs"]
mod tests;
