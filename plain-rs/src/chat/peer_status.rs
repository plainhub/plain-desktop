use crate::{
    chat::peer_auth,
    db::{DPeer, Db},
};
use anyhow::{Result, bail};
use serde_json::{Value, json};
use std::{collections::HashMap, sync::Mutex};

struct State {
    revision: u64,
    connections: HashMap<String, DPeer>,
}
pub struct Connections {
    id: String,
    state: Mutex<State>,
}
impl Default for Connections {
    fn default() -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            state: Mutex::new(State {
                revision: 0,
                connections: HashMap::new(),
            }),
        }
    }
}
impl Connections {
    pub fn open(&self, db: &Db, id: &str, body: &[u8]) -> Result<(String, bool)> {
        let authenticated =
            peer_auth::authenticate(db, id, "", body).map_err(|e| anyhow::anyhow!(e.reason()))?;
        if authenticated.graphql_json != id {
            bail!("Status identity does not match peer");
        }
        let mut state = self.state.lock().unwrap();
        if state.connections.len() >= 128 {
            bail!("Too many status connections");
        }
        if !Self::current(db, &authenticated.peer) {
            bail!("Peer identity changed");
        }
        let changed = !state.connections.values().any(|p| {
            p.id == id
                && p.key == authenticated.peer.key
                && p.public_key == authenticated.peer.public_key
        });
        let lease = uuid::Uuid::new_v4().to_string();
        state.connections.insert(lease.clone(), authenticated.peer);
        state.revision = state.revision.wrapping_add(1);
        Ok((lease, changed))
    }
    pub fn valid(&self, db: &Db, lease: &str) -> bool {
        let state = self.state.lock().unwrap();
        state
            .connections
            .get(lease)
            .is_some_and(|expected| Self::current(db, expected))
    }
    fn current(db: &Db, expected: &DPeer) -> bool {
        crate::db::chat_store::peers::get(db, &expected.id)
            .ok()
            .flatten()
            .is_some_and(|peer| {
                peer.is_paired()
                    && peer.key == expected.key
                    && peer.public_key == expected.public_key
            })
    }
    pub fn close(&self, db: &Db, lease: &str) -> Option<(String, bool)> {
        let mut state = self.state.lock().unwrap();
        let peer = state.connections.remove(lease)?;
        state.revision = state.revision.wrapping_add(1);
        let online = state
            .connections
            .values()
            .any(|p| p.id == peer.id && Self::current(db, p));
        Some((peer.id, online))
    }
    pub fn snapshot(&self, db: &Db) -> Value {
        let state = self.state.lock().unwrap();
        let online: std::collections::BTreeSet<_> = state
            .connections
            .values()
            .filter(|p| Self::current(db, p))
            .map(|p| p.id.clone())
            .collect();
        json!({"runtimeId":self.id,"revision":state.revision,"online":online})
    }
}

pub fn handshake(key: &[u8], keypair: &[u8], client_id: &str) -> Result<Vec<u8>> {
    if client_id.is_empty() {
        bail!("Missing client identity");
    }
    let timestamp = chrono::Utc::now().timestamp_millis();
    let signature = crate::ed25519_sign(keypair, format!("{timestamp}{client_id}").as_bytes());
    if signature.is_empty() {
        bail!("Invalid signing identity");
    }
    crate::xchacha_encrypt_raw(
        key,
        format!("{signature}|{timestamp}|{client_id}").as_bytes(),
    )
    .ok_or_else(|| anyhow::anyhow!("Invalid peer key"))
}
