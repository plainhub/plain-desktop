use crate::db::{DPeer, Db, chat_store::peers};
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    future::Future,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Capabilities {
    pub ble_ready: bool,
    pub aware_supported: bool,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Advertisement {
    pub short_id: String,
    pub aware_supported: bool,
    pub aware_running: bool,
}
pub trait Driver: Send + Sync {
    fn capabilities(&self) -> impl Future<Output = Result<Capabilities>> + Send;
    fn scan(&self, short_id: &str) -> impl Future<Output = Result<Option<Advertisement>>> + Send;
    fn start_aware(&self, peer_id: &str) -> impl Future<Output = Result<bool>> + Send;
    fn observe(
        &self,
        peer_id: &str,
        value: &Advertisement,
    ) -> impl Future<Output = Result<()>> + Send;
}
struct Entry {
    generation: u64,
    started: Instant,
    active: bool,
}
#[derive(Default)]
struct State {
    next: u64,
    entries: HashMap<String, Entry>,
}
pub struct Prewarmer {
    state: Mutex<State>,
    slots: tokio::sync::Semaphore,
}
impl Default for Prewarmer {
    fn default() -> Self {
        Self {
            state: Mutex::new(State::default()),
            slots: tokio::sync::Semaphore::new(2),
        }
    }
}
struct Claim {
    owner: Arc<Prewarmer>,
    id: String,
    generation: u64,
}
impl Claim {
    fn current(&self) -> bool {
        self.owner
            .state
            .lock()
            .unwrap()
            .entries
            .get(&self.id)
            .is_some_and(|entry| entry.active && entry.generation == self.generation)
    }
}
impl Drop for Claim {
    fn drop(&mut self) {
        if let Some(entry) = self.owner.state.lock().unwrap().entries.get_mut(&self.id)
            && entry.generation == self.generation
        {
            entry.active = false;
        }
    }
}
impl Prewarmer {
    fn begin(self: &Arc<Self>, id: &str, now: Instant) -> Result<Option<Claim>> {
        let mut state = self.state.lock().unwrap();
        state.entries.retain(|_, entry| {
            entry.active || now.saturating_duration_since(entry.started) < Duration::from_secs(600)
        });
        if state.entries.get(id).is_some_and(|entry| {
            entry.active || now.saturating_duration_since(entry.started) < Duration::from_secs(30)
        }) {
            return Ok(None);
        }
        if !state.entries.contains_key(id) && state.entries.len() >= 128 {
            bail!("Peer prewarm capacity exceeded");
        }
        state.next += 1;
        let generation = state.next;
        state.entries.insert(
            id.into(),
            Entry {
                generation,
                started: now,
                active: true,
            },
        );
        Ok(Some(Claim {
            owner: self.clone(),
            id: id.into(),
            generation,
        }))
    }
    pub fn forget(&self, id: &str) {
        self.state.lock().unwrap().entries.remove(id);
    }
}
fn paired(db: &Db, id: &str) -> Result<Option<DPeer>> {
    Ok(peers::get(db, id)?.filter(DPeer::is_paired))
}
fn unchanged(db: &Db, claim: &Claim, before: &DPeer) -> Result<bool> {
    Ok(claim.current()
        && paired(db, &before.id)?
            .is_some_and(|peer| peer.key == before.key && peer.public_key == before.public_key))
}
pub async fn run<D: Driver>(
    db: &Db,
    prewarmer: &Arc<Prewarmer>,
    id: &str,
    driver: &D,
) -> Result<Option<Advertisement>> {
    let Some(peer) = paired(db, id)? else {
        return Ok(None);
    };
    let Ok(_slot) = prewarmer.slots.try_acquire() else {
        return Ok(None);
    };
    let Some(claim) = prewarmer.begin(id, Instant::now())? else {
        return Ok(None);
    };
    let caps = driver.capabilities().await?;
    if !caps.ble_ready {
        return Ok(None);
    }
    let short_id = crate::utils::hex::bytes_to_hex(&Sha256::digest(id.as_bytes())[..8]);
    let scan = tokio::time::timeout(Duration::from_secs(15), driver.scan(&short_id)).await;
    let Some(mut value) = scan.map_err(|_| anyhow::anyhow!("Peer prewarm scan timed out"))?? else {
        return Ok(None);
    };
    if value.short_id != short_id {
        bail!("Peer prewarm advertisement mismatch")
    }
    if !unchanged(db, &claim, &peer)? {
        return Ok(None);
    }
    driver.observe(id, &value).await?;
    if caps.aware_supported && value.aware_supported && !value.aware_running {
        if !unchanged(db, &claim, &peer)? {
            return Ok(None);
        }
        let started = tokio::time::timeout(Duration::from_secs(20), driver.start_aware(id))
            .await
            .map_err(|_| anyhow::anyhow!("Peer prewarm start timed out"))??;
        if !unchanged(db, &claim, &peer)? {
            return Ok(None);
        }
        if started {
            value.aware_running = true;
            driver.observe(id, &value).await?;
        }
    }
    Ok(Some(value))
}

#[cfg(test)]
#[path = "../../tests/unit/chat/prewarm.rs"]
mod tests;
