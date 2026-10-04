use crate::crypto::EcdhSession;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};
pub const RESPONSE_TIMEOUT: Duration = Duration::from_secs(90);
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub device_id: String,
    pub device_name: String,
    pub device_ip: String,
    pub device_port: u16,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ticket {
    #[serde(flatten)]
    pub target: Target,
    pub generation: String,
    pub delay_ms: u64,
}
pub struct Pending {
    pub ticket: Ticket,
    pub ecdh: EcdhSession,
    started: Instant,
}
#[derive(Default)]
pub struct Sessions(
    Mutex<HashMap<String, Pending>>,
    Mutex<HashMap<String, Incoming>>,
);
struct Incoming {
    signature: String,
    target: Target,
    timestamp: i64,
}

impl Sessions {
    pub fn receive(&self, request: &super::protocol::PairingRequest) -> bool {
        let mut incoming = self.1.lock().unwrap();
        incoming.retain(|_, pending| super::timestamp_ok(pending.timestamp));
        if incoming
            .get(&request.from_id)
            .is_some_and(|p| p.timestamp > request.timestamp)
        {
            return false;
        }
        let new = incoming
            .get(&request.from_id)
            .is_none_or(|p| p.signature != request.signature);
        let target = Target {
            device_id: request.from_id.clone(),
            device_name: request.from_name.clone(),
            device_ip: request.from_ip.clone(),
            device_port: request.port,
        };
        if !new && target.device_ip.is_empty() {
            return false;
        }
        incoming.insert(
            request.from_id.clone(),
            Incoming {
                signature: request.signature.clone(),
                target,
                timestamp: request.timestamp,
            },
        );
        new
    }
    pub fn take_incoming(&self, id: &str, signature: &str) -> Option<Target> {
        let mut incoming = self.1.lock().unwrap();
        if incoming.get(id)?.signature != signature {
            return None;
        }
        incoming.remove(id).map(|p| p.target)
    }
    pub fn receive_cancel(&self, id: &str) -> Option<Target> {
        let outgoing = self.cancel(id, None).map(|p| p.target);
        self.1
            .lock()
            .unwrap()
            .remove(id)
            .map(|p| p.target)
            .or(outgoing)
    }
    pub fn start(&self, target: Target, ecdh: EcdhSession) -> Ticket {
        let ticket = Ticket {
            target,
            generation: uuid::Uuid::new_v4().to_string(),
            delay_ms: RESPONSE_TIMEOUT.as_millis() as u64,
        };
        self.0.lock().unwrap().insert(
            ticket.target.device_id.clone(),
            Pending {
                ticket: ticket.clone(),
                ecdh,
                started: Instant::now(),
            },
        );
        ticket
    }
    pub fn contains(&self, id: &str) -> bool {
        self.0.lock().unwrap().contains_key(id)
    }
    pub fn tickets(&self) -> Vec<Ticket> {
        self.0
            .lock()
            .unwrap()
            .values()
            .filter(|pending| !pending.expired())
            .map(|pending| pending.ticket.clone())
            .collect()
    }
    pub fn expire_all(&self) -> Vec<Ticket> {
        let mut pending = self.0.lock().unwrap();
        let ids = pending
            .iter()
            .filter(|(_, value)| value.expired())
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        ids.into_iter()
            .filter_map(|id| pending.remove(&id).map(|value| value.ticket))
            .collect()
    }
    pub fn current(&self, id: &str, generation: &str) -> bool {
        self.0
            .lock()
            .unwrap()
            .get(id)
            .is_some_and(|p| p.ticket.generation == generation)
    }
    pub fn take(&self, id: &str) -> Option<Pending> {
        self.0.lock().unwrap().remove(id)
    }
    pub fn cancel(&self, id: &str, generation: Option<&str>) -> Option<Ticket> {
        let mut pending = self.0.lock().unwrap();
        let item = pending.get(id)?;
        if generation.is_some_and(|g| g != item.ticket.generation) {
            return None;
        }
        pending.remove(id).map(|p| p.ticket)
    }
    pub fn expire(&self, id: &str, generation: &str) -> Option<Ticket> {
        let mut pending = self.0.lock().unwrap();
        let item = pending.get(id)?;
        if item.ticket.generation != generation || !item.expired() {
            return None;
        }
        pending.remove(id).map(|p| p.ticket)
    }
}
impl Pending {
    pub fn expired(&self) -> bool {
        self.started.elapsed() >= RESPONSE_TIMEOUT
    }
}
#[cfg(test)]
#[path = "../../../tests/unit/chat/pairing/sessions.rs"]
mod tests;
