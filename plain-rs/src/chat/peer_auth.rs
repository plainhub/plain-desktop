use crate::{
    base64_decode,
    db::{CHANNEL_COLS, DPeer, Db, PEER_COLS, row_to_channel, row_to_peer},
    ed25519_verify, xchacha_decrypt_raw,
};
use rusqlite::OptionalExtension;
use std::time::{SystemTime, UNIX_EPOCH};
const TIMESTAMP_WINDOW_MS: u64 = 5 * 60 * 1000;

pub struct AuthenticatedPeer {
    pub peer: DPeer,
    pub key: Vec<u8>,
    pub signature_b64: String,
    pub timestamp: i64,
    pub graphql_json: String,
}
pub struct Envelope {
    pub signature_b64: String,
    pub timestamp: i64,
    pub content: String,
}
#[derive(Debug)]
pub enum AuthError {
    UnknownPeer,
    NotPaired,
    DecryptFailed,
    NotUtf8,
    TimestampExpired,
    BadSignature,
    MissingFields,
    NoChannelKey,
    Storage,
}
impl AuthError {
    pub fn reason(&self) -> &'static str {
        match self {
            Self::UnknownPeer => "unknown peer",
            Self::NotPaired => "not paired",
            Self::DecryptFailed => "decrypt failed",
            Self::NotUtf8 => "not utf-8",
            Self::TimestampExpired => "timestamp expired",
            Self::BadSignature => "bad signature",
            Self::MissingFields => "missing fields",
            Self::NoChannelKey => "no channel key",
            Self::Storage => "peer storage failed",
        }
    }
    pub fn http_status(&self) -> u16 {
        match self {
            Self::NotPaired => 403,
            Self::TimestampExpired | Self::MissingFields => 400,
            Self::Storage => 500,
            _ => 401,
        }
    }
}
pub fn decrypt(key: &[u8], public_key: &str, body: &[u8]) -> Result<Envelope, AuthError> {
    let plaintext = xchacha_decrypt_raw(key, body).ok_or(AuthError::DecryptFailed)?;
    let plaintext = std::str::from_utf8(&plaintext).map_err(|_| AuthError::NotUtf8)?;
    let mut parts = plaintext.splitn(3, '|');
    let signature_b64 = parts.next().unwrap_or_default();
    let raw_timestamp = parts.next().ok_or(AuthError::MissingFields)?;
    let content = parts
        .next()
        .filter(|s| !s.is_empty())
        .ok_or(AuthError::MissingFields)?;
    if signature_b64.is_empty() {
        return Err(AuthError::MissingFields);
    }
    let timestamp = raw_timestamp
        .parse::<i64>()
        .map_err(|_| AuthError::MissingFields)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    if now.abs_diff(timestamp) > TIMESTAMP_WINDOW_MS {
        return Err(AuthError::TimestampExpired);
    }
    if !ed25519_verify(
        public_key,
        format!("{timestamp}{content}").as_bytes(),
        signature_b64,
    ) {
        return Err(AuthError::BadSignature);
    }
    Ok(Envelope {
        signature_b64: signature_b64.into(),
        timestamp,
        content: content.into(),
    })
}
pub fn authenticate(
    db: &Db,
    client_id: &str,
    channel_id: &str,
    body: &[u8],
) -> Result<AuthenticatedPeer, AuthError> {
    let (peer, key) = db.with_conn(|c| -> Result<_, AuthError> {
        let peer = c
            .query_row(
                &format!("SELECT {PEER_COLS} FROM peers WHERE id=?1"),
                [client_id],
                row_to_peer,
            )
            .optional()
            .map_err(|_| AuthError::Storage)?
            .ok_or(AuthError::UnknownPeer)?;
        let key = if channel_id.is_empty() {
            if !peer.is_paired() {
                return Err(AuthError::NotPaired);
            }
            base64_decode(&peer.key)
        } else {
            let channel = c
                .query_row(
                    &format!("SELECT {CHANNEL_COLS} FROM chat_channels WHERE id=?1"),
                    [channel_id],
                    row_to_channel,
                )
                .optional()
                .map_err(|_| AuthError::Storage)?
                .ok_or(AuthError::NoChannelKey)?;
            base64_decode(&channel.key)
        };
        Ok((peer, key))
    })?;
    let envelope = decrypt(&key, &peer.public_key, body)?;
    Ok(AuthenticatedPeer {
        peer,
        key,
        signature_b64: envelope.signature_b64,
        timestamp: envelope.timestamp,
        graphql_json: envelope.content,
    })
}
#[cfg(test)]
#[path = "../../tests/unit/chat/peer_auth.rs"]
mod tests;
