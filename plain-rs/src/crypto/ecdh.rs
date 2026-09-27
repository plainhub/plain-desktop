use p256::{PublicKey as P256PublicKey, ecdh::EphemeralSecret, pkcs8::DecodePublicKey};
use rand::rngs::OsRng;
use sha2::{Digest, Sha256};

// ── ECDH P-256 ───────────────────────────────────────────────────────────────
//
// Matches plain-app `PairingCrypto.generateECDHKeyPair()` (secp256r1),
// `publicKeyEncoded` (X9.63 uncompressed point: 0x04 || X || Y, 65 bytes),
// and `computeECDHSharedKey()` (SHA-256 of the raw shared secret).

/// ECDH session — holds the ephemeral secret until shared key is computed.
pub struct EcdhSession {
    secret: EphemeralSecret,
    /// X9.63 uncompressed public key bytes (65 bytes: 0x04 || X || Y) for
    /// transmitting to peer. Matches plain-app's `publicKeyEncoded`.
    pub public_key_bytes: Vec<u8>,
}

impl EcdhSession {
    /// Generate a new ephemeral P-256 ECDH key pair.
    pub fn generate() -> Self {
        let secret = EphemeralSecret::random(&mut OsRng);
        let public_key = P256PublicKey::from(&secret);
        // SEC1 uncompressed point: 0x04 || X(32) || Y(32) = 65 bytes.
        // Matches plain-app's `encodePublicKeyX963`.
        let public_key_bytes = public_key.to_sec1_bytes().to_vec();
        EcdhSession {
            secret,
            public_key_bytes,
        }
    }

    /// Compute SHA-256(ECDH raw shared secret) from the peer's X9.63
    /// uncompressed public key bytes (65 bytes: 0x04 || X || Y).
    /// Returns `None` if the peer public key is malformed.
    pub fn compute_shared_key(self, peer_pub_bytes: &[u8]) -> Option<[u8; 32]> {
        // plain-app sends X9.63 uncompressed point (65 bytes). Try SEC1 first,
        // fall back to X.509 SPKI DER for backwards compatibility.
        let peer_pub = P256PublicKey::from_sec1_bytes(peer_pub_bytes)
            .or_else(|_| P256PublicKey::from_public_key_der(peer_pub_bytes))
            .ok()?;
        let shared = self.secret.diffie_hellman(&peer_pub);
        let digest = Sha256::digest(shared.raw_secret_bytes());
        let mut out = [0u8; 32];
        out.copy_from_slice(&digest);
        Some(out)
    }
}
