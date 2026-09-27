mod ecdh;
mod ed25519;
mod symmetric;

#[cfg(test)]
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
mod tests {
    use super::*;

    #[test]
    fn gen_random_produces_distinct_buffers() {
        let mut a = [0u8; 32];
        let mut b = [0u8; 32];
        gen_random(&mut a);
        gen_random(&mut b);
        assert_ne!(a, b);
    }

    #[test]
    fn random_bytes_are_distinct() {
        assert_ne!(random_bytes(32), random_bytes(32));
        assert!(random_bytes(0).is_empty());
    }

    #[test]
    fn gen_token_is_unique_and_base64_padded() {
        let t = gen_token();
        assert_eq!(t.len(), 44);
        assert!(t.ends_with('='));
        assert!(
            t.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '/' || c == '=')
        );
        assert_ne!(t, gen_token());
    }

    #[test]
    fn xchacha_nonce_is_fresh_per_call() {
        let key = [7u8; 32];
        let c1 = xchacha_encrypt_raw(&key, b"hello").unwrap();
        let c2 = xchacha_encrypt_raw(&key, b"hello").unwrap();
        assert_ne!(&c1[..24], &c2[..24]);
        assert_ne!(c1, c2);
    }

    #[test]
    fn chacha20_nonce_is_fresh_per_call() {
        let key = [7u8; 32];
        let c1 = chacha20_encrypt(&key, b"hello").unwrap();
        let c2 = chacha20_encrypt(&key, b"hello").unwrap();
        assert_ne!(&c1[..12], &c2[..12]);
    }
}
