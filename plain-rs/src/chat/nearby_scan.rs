use super::nearby_wire::{DiscoverReply, discover_reply};
use anyhow::{Result, bail, ensure};
use serde::Serialize;
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Step {
    Read { generation: String },
    Wait,
    Emit { reply: DiscoverReply },
}
struct Entry {
    reply: Option<DiscoverReply>,
    generation: String,
    last_emit: Option<Instant>,
    touched: Instant,
}
#[derive(Default)]
pub struct Scans(Mutex<HashMap<String, HashMap<String, Entry>>>);
impl Scans {
    pub fn begin(&self) -> Result<String> {
        let mut scans = self.0.lock().unwrap();
        ensure!(scans.len() < 16, "BLE scan capacity exceeded");
        let id = uuid::Uuid::new_v4().to_string();
        scans.insert(id.clone(), HashMap::new());
        Ok(id)
    }
    pub fn end(&self, id: &str) {
        self.0.lock().unwrap().remove(id);
    }
    pub fn seen(&self, id: &str, short_id: &str) -> Result<Step> {
        self.seen_at(id, short_id, Instant::now())
    }
    fn seen_at(&self, id: &str, short_id: &str, now: Instant) -> Result<Step> {
        if short_id.len() != 16
            || !short_id
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        {
            return Ok(Step::Wait);
        }
        let mut scans = self.0.lock().unwrap();
        let entries = scans
            .get_mut(id)
            .ok_or_else(|| anyhow::anyhow!("Unknown BLE scan"))?;
        if let Some(entry) = entries.get_mut(short_id) {
            if let Some(reply) = &entry.reply {
                if entry.last_emit.is_some_and(|last| {
                    now.saturating_duration_since(last) <= Duration::from_secs(5)
                }) {
                    return Ok(Step::Wait);
                }
                entry.last_emit = Some(now);
                entry.touched = now;
                return Ok(Step::Emit {
                    reply: reply.clone(),
                });
            }
            if now.saturating_duration_since(entry.touched) < Duration::from_secs(30) {
                return Ok(Step::Wait);
            }
        }
        if entries.len() >= 512 && !entries.contains_key(short_id) {
            let oldest = entries
                .iter()
                .min_by_key(|(_, v)| v.touched)
                .map(|(k, _)| k.clone())
                .unwrap();
            entries.remove(&oldest);
        }
        let generation = uuid::Uuid::new_v4().to_string();
        entries.insert(
            short_id.into(),
            Entry {
                reply: None,
                generation: generation.clone(),
                last_emit: None,
                touched: now,
            },
        );
        Ok(Step::Read { generation })
    }
    pub fn reply(
        &self,
        id: &str,
        short_id: &str,
        generation: &str,
        payload: Option<&str>,
    ) -> Result<Option<DiscoverReply>> {
        let mut scans = self.0.lock().unwrap();
        let Some(entries) = scans.get_mut(id) else {
            return Ok(None);
        };
        let Some(entry) = entries.get_mut(short_id) else {
            return Ok(None);
        };
        if entry.generation != generation || entry.reply.is_some() {
            return Ok(None);
        };
        let reply = payload.map(|p| discover_reply(p, short_id)).transpose();
        match reply {
            Ok(Some(reply)) => {
                let now = Instant::now();
                entry.reply = Some(reply.clone());
                entry.last_emit = Some(now);
                entry.touched = now;
                Ok(Some(reply))
            }
            Ok(None) => {
                entries.remove(short_id);
                Ok(None)
            }
            Err(error) => {
                entries.remove(short_id);
                bail!(error)
            }
        }
    }
}
#[cfg(test)]
#[path = "../../tests/unit/chat/nearby_scan.rs"]
mod tests;
