//! Session store - mirrors `internal/db/session_*.go`.
//! Sessions are keyed by client id; each holds a ChaCha20 key (base64) used
//! to encrypt request bodies. The Go code also tracks client info and last
//! active timestamps; we keep the same shape.

use super::Db;
use anyhow::{Result, anyhow};
use chrono::{DateTime, Utc};
use rand::RngCore;
use serde::{Deserialize, Serialize};

pub const SESSION_PREFIX: &[u8] = b"session:";

/// Symmetric key length (bytes) for session tokens — XChaCha20-Poly1305 keys.
const KEY_LEN: usize = 32;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionInfo {
    pub client_id: String,
    pub token: String, // base64 of ChaCha20 key
    pub client_name: String,
    pub browser_name: String,
    pub browser_version: String,
    pub os_name: String,
    pub os_version: String,
    pub is_mobile: bool,
    pub last_active: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub struct SessionStore<'a> {
    db: &'a Db,
}

impl<'a> SessionStore<'a> {
    pub fn new(db: &'a Db) -> Self {
        Self { db }
    }

    pub fn get(&self, client_id: &str) -> Option<SessionInfo> {
        let key = key_for(client_id);
        self.db
            .get(&key)
            .ok()
            .flatten()
            .and_then(|v| serde_json::from_slice(&v).ok())
    }

    pub fn upsert(&self, mut info: SessionInfo) -> Result<SessionInfo> {
        if info.token.is_empty() {
            // Generate a new 32-byte key and base64-encode it, matching the Go side.
            let mut key = [0u8; KEY_LEN];
            rand::thread_rng().fill_bytes(&mut key);
            info.token = crate::utils::base64::base64_encode(&key);
        }
        let now = Utc::now();
        if info.created_at.timestamp() == 0 {
            info.created_at = now;
        }
        info.updated_at = now;
        info.last_active = now;
        let key = key_for(&info.client_id);
        let bytes = serde_json::to_vec(&info)?;
        self.db.insert(&key, bytes)?;
        Ok(info)
    }

    pub fn touch_last_active(&self, info: &SessionInfo) -> Result<()> {
        let mut info = info.clone();
        info.last_active = Utc::now();
        let bytes = serde_json::to_vec(&info)?;
        self.db.insert(&key_for(&info.client_id), bytes)?;
        Ok(())
    }

    pub fn delete(&self, client_id: &str) -> Result<()> {
        self.db.remove(key_for(client_id))?;
        Ok(())
    }

    pub fn list(&self) -> Vec<SessionInfo> {
        let mut out = Vec::new();
        for kv in self.db.scan_prefix(SESSION_PREFIX).flatten() {
            if let Ok(s) = serde_json::from_slice::<SessionInfo>(&kv.1) {
                out.push(s);
            }
        }
        out
    }
}

fn key_for(client_id: &str) -> Vec<u8> {
    let mut k = SESSION_PREFIX.to_vec();
    k.extend_from_slice(client_id.as_bytes());
    k
}

/// Helper used by the API layer to derive a 32-byte key from a session token.
pub fn token_key(token: &str) -> Result<[u8; KEY_LEN]> {
    let bytes = crate::utils::base64::base64_decode_checked(token).map_err(|_| anyhow!("invalid base64 token"))?;
    if bytes.len() != KEY_LEN {
        return Err(anyhow!("token must be {} bytes", KEY_LEN));
    }
    let mut k = [0u8; KEY_LEN];
    k.copy_from_slice(&bytes);
    Ok(k)
}
