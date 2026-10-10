use tokio::sync::broadcast;

use crate::api::context::WsEvent;
use crate::media::eventbus::{self, EventBus};

pub const WS_MEDIA_SCAN_PROGRESS: &'static str = "MEDIA_SCAN_PROGRESS";
pub const WS_FILE_TASK_PROGRESS: &'static str = "FILE_TASK_PROGRESS";
pub const WS_DLNA_RENDERER_FOUND: &'static str = "DLNA_RENDERER_FOUND";
pub const WS_DLNA_DISCOVERY_DONE: &'static str = "DLNA_DISCOVERY_DONE";
pub const WS_DISK_FORMAT_DONE: &'static str = "DISK_FORMAT_DONE";

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
