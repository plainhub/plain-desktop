//! Per-connection GATT server session state — Rust owns the bookkeeping.
//!
//! A BLE peer writes fragmented frames to one of two characteristics. This
//! module owns the per-(address, characteristic) assembly sessions the GATT
//! server needs: which assembler is mid-message, when it goes stale, and the
//! request id a completed Nearby request came from. The platform server layer
//! (Android/iOS) only forwards raw writes here and receives either "need more
//! frames" or a completed message to dispatch.
//!
//! Assembly itself is [`crate::protocols::ble::Assembler`]; this adds the
//! multi-connection table, the per-peer capacity limit, and idle expiry that
//! the old Kotlin `BleServerProtocol` duplicated in a second language.

use crate::protocols::ble::{self, Assembler};
use anyhow::{Result, ensure};
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

/// Idle time after which a half-assembled message is dropped.
pub const ASSEMBLY_IDLE: Duration = Duration::from_secs(15);
/// Maximum simultaneously assembling messages from one peer address.
pub const PEER_CAPACITY: usize = 2;
/// Maximum tracked connections.
pub const SESSION_CAPACITY: usize = 512;

/// What the GATT server should do after forwarding a write.
#[derive(Debug, PartialEq)]
pub enum Outcome {
    /// Message incomplete; keep the session and wait for more frames.
    Pending,
    /// A complete request message arrived. Dispatch it to the handler.
    Complete { request_id: u32, body: Vec<u8> },
}

struct Session {
    assembler: Assembler,
    touched: Instant,
}

/// Session table keyed by `(address, lowercased characteristic uuid)`, plus the
/// last completed Nearby request id per address for response correlation.
#[derive(Default)]
pub struct Sessions {
    table: Mutex<HashMap<(String, String), Session>>,
    nearby: Mutex<HashMap<String, u32>>,
}

impl Sessions {
    /// Feed one raw GATT write. Returns whether more frames are needed, or the
    /// assembled request once the final frame lands.
    ///
    /// `nearby` marks the Nearby characteristic so a completed request records
    /// its id for later response correlation.
    pub fn write(
        &self,
        address: &str,
        characteristic: &str,
        value: &[u8],
        nearby: bool,
    ) -> Result<Outcome> {
        let key = (address.to_string(), characteristic.to_ascii_lowercase());
        let mut table = self.table.lock().unwrap();
        Sessions::reap(&mut table);
        ensure!(
            table.keys().filter(|(addr, _)| addr == address).count() < PEER_CAPACITY
                || table.contains_key(&key),
            "BLE assembly capacity exceeded"
        );
        let now = Instant::now();
        let session = table
            .entry(key.clone())
            .or_insert_with(|| Session {
                assembler: Assembler::default(),
                touched: now,
            });
        let result = session.assembler.push(value);
        session.touched = now;
        match result {
            Ok(Some(body)) => {
                let request_id = session.assembler.info() as u32;
                let response = session.assembler.info() >> 32 != 0;
                table.remove(&key);
                ensure!(!response, "Unexpected BLE request direction");
                Ok(Outcome::Complete { request_id, body })
            }
            Ok(None) => Ok(Outcome::Pending),
            Err(error) => {
                table.remove(&key);
                Err(error)
            }
        }
    }

    /// Records the request id of a completed Nearby request so the platform can
    /// correlate a later response notification back to the peer.
    pub fn record_nearby_request(&self, address: &str, request_id: u32) {
        self.nearby.lock().unwrap().insert(address.to_string(), request_id);
    }

    /// The most recent Nearby request id seen for an address, if any.
    pub fn nearby_request_id(&self, address: &str) -> Option<u32> {
        self.nearby.lock().unwrap().get(address).copied()
    }

    /// Forget every session belonging to a disconnected peer.
    pub fn clear_peer(&self, address: &str) {
        self.table.lock().unwrap().retain(|(addr, _), _| addr != address);
        self.nearby.lock().unwrap().remove(address);
    }

    /// Drop every session.
    pub fn clear(&self) {
        self.table.lock().unwrap().clear();
        self.nearby.lock().unwrap().clear();
    }

    fn reap(table: &mut HashMap<(String, String), Session>) {
        table.retain(|_, session| session.touched.elapsed() < ASSEMBLY_IDLE);
        while table.len() > SESSION_CAPACITY {
            let Some(oldest) = table
                .iter()
                .min_by_key(|(_, session)| session.touched)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            table.remove(&oldest);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIMIT: usize = 20;

    /// Splits `message` into the frames a peer would write at `LIMIT` bytes.
    fn frames(message: &[u8], id: u32) -> Vec<Vec<u8>> {
        let size = LIMIT - ble::HEADER;
        let mut out = Vec::new();
        let mut sequence = 0u32;
        loop {
            let start = (sequence as usize) * size;
            if start >= message.len() {
                break;
            }
            out.push(ble::frame(message, id, false, sequence, LIMIT).unwrap().unwrap());
            sequence += 1;
        }
        out
    }

    #[test]
    fn reassembles_a_multi_frame_message() {
        let sessions = Sessions::default();
        let message = ble::message(&[], b"hello").unwrap();
        let parts = frames(&message, 7);
        assert!(parts.len() > 1, "fixture must span several frames");
        for part in &parts[..parts.len() - 1] {
            assert_eq!(sessions.write("mac", "c", part, false).unwrap(), Outcome::Pending);
        }
        match sessions.write("mac", "c", parts.last().unwrap(), false).unwrap() {
            Outcome::Complete { request_id, body } => {
                assert_eq!(request_id, 7);
                // The body is the reassembled BLE *message* (length prefix +
                // payload), which is what the characteristic handler decodes.
                assert_eq!(body, message);
            }
            other => panic!("expected complete, got {other:?}"),
        }
    }

    #[test]
    fn rejects_a_skipped_fragment() {
        let sessions = Sessions::default();
        let message = ble::message(&[], b"hello world hello world").unwrap();
        let parts = frames(&message, 7);
        sessions.write("mac", "c", &parts[0], false).unwrap();
        // Jump straight to the last frame, skipping the middle.
        assert!(sessions.write("mac", "c", parts.last().unwrap(), false).is_err());
    }

    #[test]
    fn nearby_request_id_roundtrips_and_clears() {
        let sessions = Sessions::default();
        sessions.record_nearby_request("mac", 42);
        assert_eq!(sessions.nearby_request_id("mac"), Some(42));
        sessions.clear_peer("mac");
        assert_eq!(sessions.nearby_request_id("mac"), None);
    }

    #[test]
    fn peer_capacity_is_enforced_across_characteristics() {
        let sessions = Sessions::default();
        let message = ble::message(&[], &[0u8; 64]).unwrap();
        let parts = frames(&message, 7);
        // First characteristic starts a session.
        sessions.write("mac", "a", &parts[0], false).unwrap();
        // Second characteristic for the same peer is still within capacity.
        sessions.write("mac", "b", &parts[0], false).unwrap();
        // A third concurrent characteristic exceeds the per-peer limit.
        assert!(sessions.write("mac", "c", &parts[0], false).is_err());
    }
}