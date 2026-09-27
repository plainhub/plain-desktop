use chacha20poly1305::{
    ChaCha20Poly1305, Nonce as ChaNonce, XChaCha20Poly1305, XNonce,
    aead::{Aead, KeyInit},
};

use crate::utils::base64::base64_decode;

// ── XChaCha20-Poly1305 (24-byte nonce) ────────────────────────────────────────
//
// Two key forms:
//   • `xchacha_encrypt` / `xchacha_decrypt`     — key is a base64-encoded &str
//     (used by the local-server ↔ JS channel with the URL token)
//   • `xchacha_encrypt_raw` / `xchacha_decrypt_raw` — key is raw &[u8]
//     (used for peer-to-peer traffic; matches Android CryptoHelper.chaCha20Encrypt
//     which internally uses Google Tink XChaCha20Poly1305)
//
// The base64 variants are thin wrappers around the raw variants.

/// Encrypt `plaintext` with a raw 32-byte key using XChaCha20-Poly1305.
/// Returns `nonce (24 bytes) || ciphertext` or `None` on failure.
pub fn xchacha_encrypt_raw(key: &[u8], plaintext: &[u8]) -> Option<Vec<u8>> {
    let cipher = XChaCha20Poly1305::new_from_slice(key).ok()?;
    let mut nonce_bytes = [0u8; 24];
    super::gen_random(&mut nonce_bytes);
    let nonce = XNonce::from_slice(&nonce_bytes);
    let ciphertext = cipher.encrypt(nonce, plaintext).ok()?;
    let mut result = Vec::with_capacity(24 + ciphertext.len());
    result.extend_from_slice(&nonce_bytes);
    result.extend_from_slice(&ciphertext);
    Some(result)
}

/// Decrypt `data` (`nonce (24 bytes) || ciphertext`) with a raw 32-byte key using XChaCha20-Poly1305.
/// Returns plaintext or `None` on auth failure.
pub fn xchacha_decrypt_raw(key: &[u8], data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < 24 {
        return None;
    }
    let cipher = XChaCha20Poly1305::new_from_slice(key).ok()?;
    let nonce = XNonce::from_slice(&data[..24]);
    cipher.decrypt(nonce, &data[24..]).ok()
}

/// Encrypt `plaintext` using XChaCha20-Poly1305 with the key derived from a base64 token.
/// Returns `nonce (24 bytes) || ciphertext` or `None` if the token is invalid.
pub fn xchacha_encrypt(token_b64: &str, plaintext: &[u8]) -> Option<Vec<u8>> {
    xchacha_encrypt_raw(&base64_decode(token_b64), plaintext)
}

/// Decrypt `data` (`nonce (24 bytes) || ciphertext`) using XChaCha20-Poly1305.
/// Returns plaintext or `None` on auth failure / bad token.
pub fn xchacha_decrypt(token_b64: &str, data: &[u8]) -> Option<Vec<u8>> {
    xchacha_decrypt_raw(&base64_decode(token_b64), data)
}

// ── ChaCha20-Poly1305 (12-byte nonce) — mDNS / nearby discovery ──────────────
//
// Used by PeerStatusManager and NearbyDiscoverManager for local peer discovery
// (distinct from the Android CryptoHelper which uses XChaCha20 / Google Tink).

/// Encrypt `plaintext` with a 32-byte key using ChaCha20-Poly1305.
/// Returns `nonce (12 bytes) || ciphertext` or `None` on failure.
pub fn chacha20_encrypt(key: &[u8], plaintext: &[u8]) -> Option<Vec<u8>> {
    let cipher = ChaCha20Poly1305::new_from_slice(key).ok()?;
    let mut nonce_bytes = [0u8; 12];
    super::gen_random(&mut nonce_bytes);
    let nonce = ChaNonce::from_slice(&nonce_bytes);
    let ciphertext = cipher.encrypt(nonce, plaintext).ok()?;
    let mut result = Vec::with_capacity(12 + ciphertext.len());
    result.extend_from_slice(&nonce_bytes);
    result.extend_from_slice(&ciphertext);
    Some(result)
}

/// Decrypt `data` (`nonce (12 bytes) || ciphertext`) with a 32-byte key using ChaCha20-Poly1305.
/// Returns plaintext or `None` on auth failure.
pub fn chacha20_decrypt(key: &[u8], data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < 12 {
        return None;
    }
    let cipher = ChaCha20Poly1305::new_from_slice(key).ok()?;
    let nonce = ChaNonce::from_slice(&data[..12]);
    cipher.decrypt(nonce, &data[12..]).ok()
}
