//! Append-only event log. Mirrors `internal/db/events.go`.

use super::Db;
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
// event id is 16 random bytes (CSPRNG) hex-encoded — same on-wire shape
// as a UUID v4 string but without the `uuid` crate dependency. Existing
// consumers that grep for the `xxxxxxxx-xxxx-...` pattern won't match
// (this is a server-internal id only), so the format change is contained.
fn new_event_id() -> String {
    use rand::RngCore;
    let mut b = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut b);
    b[6] = (b[6] & 0x0f) | 0x40; // tag as v4
    b[8] = (b[8] & 0x3f) | 0x80;
    crate::utils::hex::bytes_to_hex(&b)
}

const PREFIX: &[u8] = b"event:";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: String,
    pub r#type: String,
    pub message: String,
    pub client_id: String,
    pub created_at: DateTime<Utc>,
}

pub struct EventLog<'a> {
    db: &'a Db,
}

impl<'a> EventLog<'a> {
    pub fn new(db: &'a Db) -> Self {
        Self { db }
    }

    pub fn add(&self, kind: &str, message: &str, client_id: &str) -> Result<()> {
        let event = Event {
            id: new_event_id(),
            r#type: kind.to_string(),
            message: message.to_string(),
            client_id: client_id.to_string(),
            created_at: Utc::now(),
        };
        // Use a monotonically increasing timestamp prefix so scans are chronological.
        let ts = event.created_at.timestamp_nanos_opt().unwrap_or(0);
        let key = format!("{}:{}:{}", prefix_str(), ts, event.id);
        let bytes = serde_json::to_vec(&event)?;
        self.db.insert(key.as_bytes(), bytes)?;
        Ok(())
    }

    /// Newest-first page of the event log. `needle` (from the DSL `text:`
    /// field) is a case-insensitive substring over `type` + `message`,
    /// applied before offset/limit. Unknown stored kinds are kept here —
    /// the GraphQL layer drops them (no data migration).
    pub fn list(&self, offset: usize, limit: usize, needle: Option<&str>) -> Result<Vec<Event>> {
        let needle = needle.map(str::to_lowercase);
        let mut events: Vec<Event> = Vec::new();
        for kv in self.db.scan_prefix(PREFIX) {
            let Ok(kv) = kv else {
                continue;
            };
            let Ok(e) = serde_json::from_slice::<Event>(&kv.1) else {
                continue;
            };
            if let Some(n) = &needle
                && !format!("{} {}", e.r#type, e.message)
                    .to_lowercase()
                    .contains(n)
            {
                continue;
            }
            events.push(e);
        }
        // Newest first to match the Go side's reverse iteration.
        events.sort_by_key(|e| std::cmp::Reverse(e.created_at));
        Ok(events.into_iter().skip(offset).take(limit).collect())
    }
}

fn prefix_str() -> &'static str {
    "event"
}

#[cfg(test)]
#[path = "../../../tests/unit/media/kv/events.rs"]
mod tests;
