//! Persistent global URL token — the `url_token` preference, matching
//! plain-app/desktop (`url_token` in their preference stores).
//!
//! The token is 32 random bytes, base64-encoded. It is used as the
//! XChaCha20-Poly1305 key for encrypting/decrypting file IDs in
//! `/fs?id=...` URLs.

use crate::prefs::Prefs;
use anyhow::Result;

const URL_TOKEN_KEY: &str = "url_token";

/// Validate that a token string is valid base64 and decodes to exactly
/// 32 bytes.
fn is_valid_token(token: &str) -> bool {
    crate::utils::base64::base64_decode_checked(token).is_ok_and(|decoded| decoded.len() == 32)
}

pub struct UrlToken<'a> {
    prefs: &'a Prefs,
}

impl<'a> UrlToken<'a> {
    pub fn new(prefs: &'a Prefs) -> Self {
        Self { prefs }
    }

    /// Ensure a URL token exists. Returns the base64-encoded token; an
    /// existing entry that is not a valid base64-encoded 32-byte value is
    /// regenerated.
    pub fn ensure(&self) -> Result<String> {
        if let Some(token) = self
            .prefs
            .get::<String>(URL_TOKEN_KEY)?
            .filter(|t| !t.is_empty() && is_valid_token(t))
        {
            return Ok(token);
        }
        let token = crate::crypto::gen_token();
        self.prefs.set(URL_TOKEN_KEY, &token)?;
        Ok(token)
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/media/kv/url_token.rs"]
mod tests;
