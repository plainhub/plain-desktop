//! Bridge the process-global media eventbus topics onto the local
//! server's WS event channel (`WsEvent`), so the unified chat socket
//! (which forwards `WsEvent`s) also delivers the plain-nas push numbers:
//!
//!   41 = `EVENT_MEDIA_SCAN_PROGRESS`      (broadcast)
//!   42 = `EVENT_FILE_TASK_PROGRESS`       (targeted at the event cid)
//!   43 = `EVENT_DLNA_RENDERER_FOUND`      (targeted)
//!   44 = `EVENT_DLNA_DISCOVERY_DONE`      (targeted)
//!   45 = `EVENT_DISK_FORMAT_DONE`         (broadcast)
//!
//! The bridge subscribes ONCE per topic for the whole process and fans
//! every event into the broadcast channel; per-connection filtering by
//! `WsEvent::target_cid` happens in the socket forward loop. Chat and
//! pairing events are NOT bridged here — they go through
//! [`crate::chat_service::ChatState::spawn_event_bridges`].

use tokio::sync::broadcast;

use crate::api::context::WsEvent;
use crate::media::eventbus::{self, EventBus};

/// WebSocket event numbers for the plain-nas push channels (extend the
/// shared plain-app numbering — the phone server owns 1-40, plain-nas
/// pushes start at 41).
pub const WS_MEDIA_SCAN_PROGRESS: i32 = 41;
pub const WS_FILE_TASK_PROGRESS: i32 = 42;
pub const WS_DLNA_RENDERER_FOUND: i32 = 43;
pub const WS_DLNA_DISCOVERY_DONE: i32 = 44;
pub const WS_DISK_FORMAT_DONE: i32 = 45;

/// Subscribe the media eventbus topics once and forward every event to
/// `event_tx` as a `WsEvent`. Must run inside the async runtime (it
/// spawns the forward tasks). Idempotent per call — each call adds one
/// forwarder; hosts call it exactly once at startup.
pub fn spawn_media_event_bridge(event_tx: broadcast::Sender<WsEvent>) {
    {
        let tx = event_tx.clone();
        EventBus::global().subscribe(
            eventbus::EVENT_MEDIA_SCAN_PROGRESS,
            move |payload: serde_json::Value| {
                let _ = tx.send(WsEvent::broadcast(
                    WS_MEDIA_SCAN_PROGRESS,
                    payload.to_string(),
                ));
            },
        );
    }
    {
        let tx = event_tx.clone();
        EventBus::global().subscribe_with_cid(
            eventbus::EVENT_FILE_TASK_PROGRESS,
            move |event_cid: String, payload: serde_json::Value| {
                let _ = tx.send(WsEvent::targeted(
                    WS_FILE_TASK_PROGRESS,
                    payload.to_string(),
                    &event_cid,
                ));
            },
        );
    }
    {
        let tx = event_tx.clone();
        EventBus::global().subscribe_with_cid(
            eventbus::EVENT_DLNA_RENDERER_FOUND,
            move |event_cid: String, payload: serde_json::Value| {
                let _ = tx.send(WsEvent::targeted(
                    WS_DLNA_RENDERER_FOUND,
                    payload.to_string(),
                    &event_cid,
                ));
            },
        );
    }
    {
        let tx = event_tx.clone();
        EventBus::global().subscribe_with_cid(
            eventbus::EVENT_DLNA_DISCOVERY_DONE,
            move |event_cid: String, payload: serde_json::Value| {
                let _ = tx.send(WsEvent::targeted(
                    WS_DLNA_DISCOVERY_DONE,
                    payload.to_string(),
                    &event_cid,
                ));
            },
        );
    }
    {
        let tx = event_tx.clone();
        // Broadcast like the scan progress: every connected client
        // (including the one that triggered the format) learns the
        // disk is ready without polling `mounts`.
        EventBus::global().subscribe(
            eventbus::EVENT_DISK_FORMAT_DONE,
            move |payload: serde_json::Value| {
                let _ = tx.send(WsEvent::broadcast(WS_DISK_FORMAT_DONE, payload.to_string()));
            },
        );
    }
}
