//! Persistent server Ed25519 signing keypair — the `signature_key_pair`
//! preference, stored exactly like plain-desktop (base64 of the 64-byte
//! keypair). The keypair is generated on first use so the public key
//! served by `POST /init` is stable across restarts — clients use it for
//! TOFU verification of signed login responses (plain-desktop
//! `login-handshake.ts`, plain-app pairing).

use crate::prefs::Prefs;
use anyhow::Result;

const SIGNATURE_KEYPAIR_KEY: &str = "signature_key_pair";

pub struct SignatureKey<'a> {
    prefs: &'a Prefs,
}

impl<'a> SignatureKey<'a> {
    pub fn new(prefs: &'a Prefs) -> Self {
        Self { prefs }
    }

    /// Ensure a signing keypair exists. Returns the base64-encoded
    /// 64-byte keypair, regenerating malformed entries.
    fn ensure_keypair_b64(&self) -> Result<String> {
        if let Some(kp) = self
            .prefs
            .get::<String>(SIGNATURE_KEYPAIR_KEY)?
            .filter(|kp| is_valid_keypair(kp))
        {
            return Ok(kp);
        }
        let (keypair, _) = crate::crypto::ed25519_generate();
        let b64 = crate::utils::base64::base64_encode(&keypair);
        self.prefs.set(SIGNATURE_KEYPAIR_KEY, &b64)?;
        Ok(b64)
    }

    /// Ensure a signing keypair exists. Returns the base64-encoded
    /// 32-byte public key (`signaturePublicKey` in the /init response).
    pub fn ensure(&self) -> Result<String> {
        let kp = self.decode(&self.ensure_keypair_b64()?)?;
        let public =
            crate::crypto::ed25519_public_from_keypair(&kp).expect("validated 64-byte keypair");
        Ok(crate::utils::base64::base64_encode(&public))
    }

    /// Ensure a signing keypair exists. Returns the 64-byte private
    /// keypair for signing login responses
    /// (`crate::crypto::ed25519_sign`).
    pub fn ensure_keypair(&self) -> Result<[u8; 64]> {
        self.decode(&self.ensure_keypair_b64()?)
    }

    fn decode(&self, b64: &str) -> Result<[u8; 64]> {
        let bytes = crate::utils::base64::base64_decode_checked(b64)?;
        let mut kp = [0u8; 64];
        anyhow::ensure!(bytes.len() == 64, "keypair must be 64 bytes");
        kp.copy_from_slice(&bytes);
        Ok(kp)
    }
}

/// A stored keypair is valid base64 decodable to 64 bytes AND carries a
/// well-formed public key (rejects corrupted entries so they are
/// regenerated instead of serving a broken identity).
fn is_valid_keypair(b64: &str) -> bool {
    crate::utils::base64::base64_decode_checked(b64)
        .ok()
        .and_then(|b| crate::crypto::ed25519_public_from_keypair(&b))
        .is_some()
}

#[cfg(test)]
#[path = "../../../tests/unit/media/kv/signature_key.rs"]
mod tests;
