//! Admin password storage — the password is kept as a SHA-512(hex) string
//! produced by the web client. Stored as the `password_hash` preference
//! (plain-app keeps its password as a `password` preference too).

use crate::prefs::Prefs;
use anyhow::Result;

const PASSWORD_KEY: &str = "password_hash";

pub struct PasswordStore<'a> {
    prefs: &'a Prefs,
}

impl<'a> PasswordStore<'a> {
    pub fn new(prefs: &'a Prefs) -> Self {
        Self { prefs }
    }
    pub fn has(&self) -> bool {
        self.get().is_some_and(|v| !v.is_empty())
    }
    pub fn get(&self) -> Option<String> {
        self.prefs.get::<String>(PASSWORD_KEY).ok().flatten()
    }
    pub fn set(&self, hash_hex: &str) -> Result<()> {
        // The web client always sends exactly 128 hex chars (sha-512).
        if hash_hex.len() != 128 {
            anyhow::bail!("password hash must be 128 hex chars (sha-512)");
        }
        if !hash_hex.chars().all(|c| c.is_ascii_hexdigit()) {
            anyhow::bail!("password hash must be hex");
        }
        self.prefs.set(PASSWORD_KEY, hash_hex)?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/media/kv/password.rs"]
mod tests;
