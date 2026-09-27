use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand::rngs::OsRng;

use crate::utils::base64::{base64_decode, base64_encode};

// ── Ed25519 signing ───────────────────────────────────────────────────────────
//
// Android uses Google Tink `Ed25519Verify` with raw 32-byte public keys.
// ed25519-dalek `VerifyingKey` also stores 32 raw bytes.

/// Generate a new Ed25519 signing key.  Returns (signing_key_64_bytes, verifying_key_32_bytes).
pub fn ed25519_generate() -> ([u8; 64], [u8; 32]) {
    let signing_key = SigningKey::generate(&mut OsRng);
    let verifying_key = signing_key.verifying_key();
    let sk_bytes = signing_key.to_keypair_bytes();
    let vk_bytes = verifying_key.to_bytes();
    (sk_bytes, vk_bytes)
}

/// Derive the raw 32-byte verifying key from 64-byte Ed25519 keypair bytes.
pub fn ed25519_public_from_keypair(keypair_bytes: &[u8]) -> Option<[u8; 32]> {
    if keypair_bytes.len() != 64 {
        return None;
    }
    let mut arr = [0u8; 64];
    arr.copy_from_slice(keypair_bytes);
    SigningKey::from_keypair_bytes(&arr)
        .ok()
        .map(|k| k.verifying_key().to_bytes())
}

/// Sign `message` with the 64-byte Ed25519 keypair bytes.  Returns Base64 signature or empty string on error.
pub fn ed25519_sign(keypair_bytes: &[u8], message: &[u8]) -> String {
    if keypair_bytes.len() != 64 {
        return String::new();
    }
    let mut arr = [0u8; 64];
    arr.copy_from_slice(keypair_bytes);
    let signing_key = match SigningKey::from_keypair_bytes(&arr) {
        Ok(k) => k,
        Err(_) => return String::new(),
    };
    let sig: Signature = signing_key.sign(message);
    base64_encode(sig.to_bytes().as_ref())
}

/// Verify `signature_b64` against `message` using the raw 32-byte `public_key_b64`.
pub fn ed25519_verify(public_key_b64: &str, message: &[u8], signature_b64: &str) -> bool {
    let pk_bytes = base64_decode(public_key_b64);
    let sig_bytes = base64_decode(signature_b64);
    if pk_bytes.len() != 32 || sig_bytes.len() != 64 {
        return false;
    }
    let mut pk_arr = [0u8; 32];
    pk_arr.copy_from_slice(&pk_bytes);
    let mut sig_arr = [0u8; 64];
    sig_arr.copy_from_slice(&sig_bytes);
    let vk = match VerifyingKey::from_bytes(&pk_arr) {
        Ok(k) => k,
        Err(_) => return false,
    };
    let sig = Signature::from_bytes(&sig_arr);
    vk.verify(message, &sig).is_ok()
}
