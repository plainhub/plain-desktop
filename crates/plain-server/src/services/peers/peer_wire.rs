use anyhow::{Result, bail};
use plain_rs::db::DPeer;
use plain_rs::db::Db;
use plain_rs::db::chat_store::channels;
use plain_rs::db::chat_store::peers;
use plain_rs::prefs::Prefs;
use plain_rs::transport::contract::START_AWARE;
use plain_rs::transport::contract::chat_item_request;
use plain_rs::transport::contract::signed_request;
use serde::{Deserialize, Serialize};
use serde_json::json;
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum Operation {
    Chat { content: String, channel_id: String },
    StartAware,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Prepared {
    #[serde(serialize_with = "crate::services::peers::peer_address::serialize_view")]
    pub peer: DPeer,
    pub body: String,
    pub key: String,
    pub channel_id: String,
}
pub(crate) fn prepare(db: &Db, prefs: &Prefs, id: &str, operation: Operation) -> Result<Prepared> {
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
    if plain_rs::utils::base64::base64_decode(&key).len() != 32 {
        bail!("Invalid peer transport key");
    }
    Ok(Prepared {
        peer,
        body,
        key,
        channel_id,
    })
}
pub(crate) fn encrypt(key: &str, body: &str) -> Result<String> {
    plain_rs::crypto::xchacha_encrypt_raw(
        &plain_rs::utils::base64::base64_decode(key),
        body.as_bytes(),
    )
    .map(|bytes| plain_rs::utils::base64::base64_encode(&bytes))
    .ok_or_else(|| anyhow::anyhow!("Invalid peer encryption key"))
}
pub(crate) fn decrypt(key: &str, body: &str) -> Result<Option<String>> {
    let key = plain_rs::utils::base64::base64_decode(key);
    if key.len() != 32 {
        bail!("Invalid peer decryption key");
    }
    plain_rs::crypto::xchacha_decrypt_raw(&key, &plain_rs::utils::base64::base64_decode(body))
        .map(String::from_utf8)
        .transpose()
        .map_err(Into::into)
}
pub(crate) fn signing_keypair(prefs: &Prefs) -> Result<Vec<u8>> {
    plain_rs::prefs::system::signing_keypair(prefs)
}
pub(crate) fn authenticate(db: &Db, id: &str, channel_id: &str, body: &str) -> serde_json::Value {
    match plain_rs::transport::auth::authenticate(
        db,
        id,
        channel_id,
        &plain_rs::utils::base64::base64_decode(body),
    ) {
        Ok(value) => {
            json!({"status":200,"key":plain_rs::utils::base64::base64_encode(&value.key),"content":value.graphql_json,"signature":value.signature_b64,"timestamp":value.timestamp})
        }
        Err(error) => json!({"status":error.http_status(),"reason":error.reason()}),
    }
}
pub(crate) fn envelope(key: &str, public_key: &str, body: &str) -> serde_json::Value {
    match plain_rs::transport::auth::decrypt(
        &plain_rs::utils::base64::base64_decode(key),
        public_key,
        &plain_rs::utils::base64::base64_decode(body),
    ) {
        Ok(value) => {
            json!({"status":200,"content":value.content,"signature":value.signature_b64,"timestamp":value.timestamp})
        }
        Err(error) => json!({"status":error.http_status(),"reason":error.reason()}),
    }
}
#[cfg(test)]
#[path = "../../../tests/unit/services/peers/peer_wire.rs"]
mod tests;
