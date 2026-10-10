use anyhow::Result;
use plain_rs::prefs::Prefs;

pub struct SignatureKey<'a> {
    prefs: &'a Prefs,
}
impl<'a> SignatureKey<'a> {
    pub fn new(prefs: &'a Prefs) -> Self {
        Self { prefs }
    }
    pub fn ensure(&self) -> Result<String> {
        let keypair = self.ensure_keypair()?;
        Ok(plain_rs::base64_encode(
            &plain_rs::ed25519_public_from_keypair(&keypair).unwrap(),
        ))
    }
    pub fn ensure_keypair(&self) -> Result<[u8; 64]> {
        plain_rs::prefs::system::ensure_signing_keypair(self.prefs)?
            .try_into()
            .map_err(|_| anyhow::anyhow!("Invalid signing identity"))
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/storage/kv/signature_key.rs"]
mod tests;
