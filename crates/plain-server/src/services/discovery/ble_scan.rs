//! BLE scan orchestration — Rust owns the scan session.
//!
//! The App remains the GATT radio driver (Android/iOS have no Rust BLE stack),
//! but everything above the radio now lives here:
//!
//! ```text
//! App scan loop:  for each advertisement -> bleScans.seen(short_id)
//! Rust:            Step::Read  -> enqueue (scan, short_id, generation)
//! Rust worker:     pop -> host.call("bleScanRead", {shortId, scan})
//!                       -> bleScans.reply(...)  (identity-checked)
//!                       -> nearby_devices::Context::seen(...)
//!                       -> emit WS_NEARBY_DEVICE_FOUND
//! ```
//!
//! The App's scan loop therefore never decides *when* to do a GATT read and
//! never touches the discovered-device list. It only feeds advertised shortIds
//! and answers `bleScanRead` when asked.
//!
//! Reads run on a single worker so the BLE radio is driven by one GATT session
//! at a time. `bleScanRead` can block up to the GATT timeout (~120s), which is
//! far longer than the local `chat/nearby` HTTP client's 30s budget — hence
//! reads are decoupled from the `seen` request that schedules them.

use crate::assembly::server::ServerState;
use crate::services::discovery::nearby_devices::Context;
use plain_rs::db::now_iso;
use plain_rs::discovery::nearby_devices::Device;
use plain_rs::discovery::nearby_scan::Step;
use serde_json::json;
use std::sync::Mutex;
use tokio::sync::Notify;

/// One pending GATT read: which scan session and which device.
#[derive(Clone)]
struct Pending {
    scan: String,
    short_id: String,
    generation: String,
}

/// Serial GATT-read work queue. A single worker drains it so the radio is never
/// driven by two concurrent GATT sessions.
#[derive(Default)]
pub(crate) struct Runtime {
    queue: Mutex<Vec<Pending>>,
    wake: Notify,
}

impl Runtime {
    /// Records that the scan state machine wants a GATT read for this device
    /// and wakes the worker. Safe to call from the synchronous `seen` request.
    fn schedule(&self, pending: Pending) {
        self.queue.lock().unwrap().push(pending);
        self.wake.notify_one();
    }

    fn take(&self) -> Option<Pending> {
        let mut queue = self.queue.lock().unwrap();
        if queue.is_empty() {
            return None;
        }
        Some(queue.remove(0))
    }
}

/// Handles the `ScanSeen` request from the App scan loop.
///
/// Returns the same step the App already understands, so the existing
/// `chat/nearby` contract is unchanged. The only behavioural change is that a
/// `Read` step now also schedules the GATT read on the Rust worker; the App no
/// longer performs the read itself.
pub(crate) async fn seen(state: &ServerState, id: &str, short_id: &str) -> anyhow::Result<Step> {
    let step = state.ble_scans.seen(id, short_id)?;
    if let Step::Read { generation } = &step {
        state.ble_scan.schedule(Pending {
            scan: id.to_string(),
            short_id: short_id.to_string(),
            generation: generation.clone(),
        });
    }
    Ok(step)
}

/// Performs one scheduled read: ask the App to do the GATT DISCOVER, hand the
/// payload back to the scan state machine for identity checking, then publish
/// the device so the App list updates over `NEARBY_DEVICES_UPDATED` /
/// `WS_NEARBY_DEVICE_FOUND`.
async fn run(state: ServerState, pending: Pending) {
    let result = state
        .host
        .call(
            "bleScanRead",
            json!({"shortId": pending.short_id, "scan": pending.scan}),
        )
        .await;
    let payload = match result {
        Ok(value) => value
            .get("payload")
            .and_then(serde_json::Value::as_str)
            .map(|s| s.to_string()),
        Err(error) => {
            log::debug!("bleScanRead {} failed: {error}", pending.short_id);
            None
        }
    };
    let reply = match state.ble_scans.reply(
        &pending.scan,
        &pending.short_id,
        &pending.generation,
        payload.as_deref(),
    ) {
        Ok(Some(reply)) => reply,
        Ok(None) => return,
        Err(error) => {
            log::debug!("ble scan reply {} rejected: {error}", pending.short_id);
            return;
        }
    };
    let device = Device {
        id: reply.id,
        name: reply.name,
        ips: reply.ips,
        port: reply.port,
        device_type: reply.device_type.as_str().to_string(),
        version: reply.version,
        platform: reply.platform,
        last_seen: now_iso(),
        discovery_methods: vec!["BLE".to_string()],
    };
    if let Err(error) = Context::from(&state).seen(device, true, false) {
        log::debug!("ble publish {} failed: {error}", pending.short_id);
    }
}

/// The single serial read worker. Started once at server bring-up.
pub(crate) fn start(state: ServerState) {
    tokio::spawn(async move {
        loop {
            let Some(pending) = state.ble_scan.take() else {
                state.ble_scan.wake.notified().await;
                continue;
            };
            run(state.clone(), pending).await;
        }
    });
}
