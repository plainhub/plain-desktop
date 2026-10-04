use crate::{
    chat::transport::{START_AWARE, chat_item_request, signed_request},
    db::{
        DPeer, Db,
        chat_store::{channels, peers},
    },
    prefs::Prefs,
};
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::json;
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub(super) enum Operation {
    Chat { content: String, channel_id: String },
    StartAware,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Prepared {
    pub peer: DPeer,
    pub body: String,
    pub key: String,
    pub channel_id: String,
}
pub(super) fn prepare(db: &Db, prefs: &Prefs, id: &str, operation: Operation) -> Result<Prepared> {
    let peer = peers::get(db, id)?.ok_or_else(|| anyhow::anyhow!("Unknown peer"))?;
    let (body, channel_id, key) = match operation {
        Operation::Chat {
            content,
            channel_id,
        } => {
            let key = if channel_id.is_empty() {
                peer.key.clone()
            } else {
                channels::get(db, &channel_id)?
                    .ok_or_else(|| anyhow::anyhow!("Unknown channel"))?
                    .key
            };
            (
                chat_item_request(&signing_keypair(prefs)?, &content)
                    .map_err(anyhow::Error::msg)?,
                channel_id,
                key,
            )
        }
        Operation::StartAware => (
            signed_request(&signing_keypair(prefs)?, START_AWARE, json!({}))
                .map_err(anyhow::Error::msg)?,
            String::new(),
            peer.key.clone(),
        ),
    };
    if crate::base64_decode(&key).len() != 32 {
        bail!("Invalid peer transport key");
    }
    Ok(Prepared {
        peer,
        body,
        key,
        channel_id,
    })
}
pub(super) fn encrypt(key: &str, body: &str) -> Result<String> {
    crate::xchacha_encrypt_raw(&crate::base64_decode(key), body.as_bytes())
        .map(|bytes| crate::base64_encode(&bytes))
        .ok_or_else(|| anyhow::anyhow!("Invalid peer encryption key"))
}
pub(super) fn decrypt(key: &str, body: &str) -> Result<Option<String>> {
    let key = crate::base64_decode(key);
    if key.len() != 32 {
        bail!("Invalid peer decryption key");
    }
    crate::xchacha_decrypt_raw(&key, &crate::base64_decode(body))
        .map(String::from_utf8)
        .transpose()
        .map_err(Into::into)
}
pub(super) fn signing_keypair(prefs: &Prefs) -> Result<Vec<u8>> {
    let raw = prefs
        .get::<String>("signature_key_pair")?
        .ok_or_else(|| anyhow::anyhow!("Missing signature keypair"))?;
    let pair: serde_json::Value = serde_json::from_str(&raw)?;
    let mut keypair = crate::base64_decode(
        pair["privateKey"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing private key"))?,
    );
    let public = crate::base64_decode(
        pair["publicKey"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing public key"))?,
    );
    if keypair.len() != 32 || public.len() != 32 {
        bail!("Invalid signature keypair");
    }
    keypair.extend(public);
    Ok(keypair)
}
pub(super) fn authenticate(db: &Db, id: &str, channel_id: &str, body: &str) -> serde_json::Value {
    match crate::chat::peer_auth::authenticate(db, id, channel_id, &crate::base64_decode(body)) {
        Ok(value) => {
            json!({"status":200,"key":crate::base64_encode(&value.key),"content":value.graphql_json,"signature":value.signature_b64,"timestamp":value.timestamp})
        }
        Err(error) => json!({"status":error.http_status(),"reason":error.reason()}),
    }
}
pub(super) fn envelope(key: &str, public_key: &str, body: &str) -> serde_json::Value {
    match crate::chat::peer_auth::decrypt(
        &crate::base64_decode(key),
        public_key,
        &crate::base64_decode(body),
    ) {
        Ok(value) => {
            json!({"status":200,"content":value.content,"signature":value.signature_b64,"timestamp":value.timestamp})
        }
        Err(error) => json!({"status":error.http_status(),"reason":error.reason()}),
    }
}
#[cfg(test)]
#[path = "../../tests/unit/content_api/peer_wire.rs"]
mod tests;
