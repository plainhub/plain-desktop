//! Per-connection broadcast hub. Each WebSocket is registered here and the
//! hub forwards `media:scan:progress` and `file:task:progress` events into
//! the connection's ChaCha20-encrypted frame queue.
//!
//! Wire format (shared with the desktop client's `EventType` map):
//!
//! ```text
//! server → client  :  big-endian i32 (msg_type) || chacha20_poly1305(json_payload)
//! client → server  :  chacha20_poly1305(handshake blob)            (one frame)
//! ```
//!
//! `msg_type` constants (extend the shared desktop numbering — the phone
//! server owns 1-40, plain-nas pushes start at 41):
//!   41 = `EVENT_MEDIA_SCAN_PROGRESS`
//!   42 = `EVENT_FILE_TASK_PROGRESS`
//!   43 = `EVENT_DLNA_RENDERER_FOUND`
//!   44 = `EVENT_DLNA_DISCOVERY_DONE`
//!   45 = `EVENT_DISK_FORMAT_DONE`
//!
//! Chat/pairing events reuse the phone-protocol numbers verbatim (the
//! same web client handles both servers): 1 message_created, 2
//! message_deleted, 3 message_updated, 18 channels_updated, 20
//! peer_status_updated, 22-26 pairing, 28 channel_invite_received.
//! They arrive on the `chat:event` bus channel as
//! `{"msgType": n, "payload": …}` and are re-framed byte-identically.

use crate::consts;
use crate::crypto;
use crate::eventbus;
use axum::extract::ws::Message;
use futures::{SinkExt, StreamExt};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;

#[derive(Clone)]
pub struct WsHub {
    inner: Arc<Mutex<HubInner>>,
}

struct HubInner {
    conns: HashMap<String, ConnHandle>,
}

#[derive(Clone)]
struct ConnHandle {
    tx: futures::channel::mpsc::UnboundedSender<Message>,
    sub_id_scan: u64,
    sub_id_task: u64,
    sub_id_dlna_found: u64,
    sub_id_dlna_done: u64,
    sub_id_format: u64,
    sub_id_chat: u64,
    _key: [u8; crypto::KEY_LEN],
}

impl WsHub {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HubInner {
                conns: HashMap::new(),
            })),
        }
    }

    /// Encode a `(msg_type, json_payload)` pair as a `Message::Binary`.
    /// Frame layout is the shared plain-rs `ws_frame` protocol:
    /// `i32_be(msg_type) || xchacha(key, json)`. Returns `None` if
    /// encryption fails.
    fn encode(key: &[u8; crypto::KEY_LEN], msg_type: i32, payload: &JsonValue) -> Option<Message> {
        let json = serde_json::to_vec(payload).ok()?;
        let frame = plain_rs::ws_frame::encode(msg_type, &json, key)?;
        Some(Message::Binary(frame))
    }

    /// Encode a `chat:event` bus payload as a `Message::Binary` using the
    /// phone-protocol `msgType` it carries. A string `payload` is framed
    /// byte-identically (chat WS bodies are pre-serialized JSON strings,
    /// e.g. `message_deleted`'s `ids=…`), anything else is serialized.
    fn encode_chat(key: &[u8; crypto::KEY_LEN], payload: &JsonValue) -> Option<Message> {
        let msg_type = payload.get("msgType")?.as_i64()? as i32;
        let inner = payload.get("payload")?;
        let bytes: Vec<u8> = match inner {
            JsonValue::String(s) => s.clone().into_bytes(),
            other => serde_json::to_vec(other).ok()?,
        };
        let frame = plain_rs::ws_frame::encode(msg_type, &bytes, key)?;
        Some(Message::Binary(frame))
    }

    pub async fn register(
        &self,
        cid: String,
        key: [u8; crypto::KEY_LEN],
        sink: futures::stream::SplitSink<axum::extract::ws::WebSocket, Message>,
    ) {
        crate::log::info!(
            "[ws_hub] register cid={} - subscribing to all 4 channels",
            cid
        );
        let (tx, mut rx) = futures::channel::mpsc::unbounded::<Message>();

        // Subscribe to all four event channels. The forward closures clone
        // the sender and key, build the encrypted message and push it onto
        // the connection's mpsc. We store the subscription ids so the
        // per-connection `unregister` call can clean them up.
        let sub_id_scan = {
            let tx = tx.clone();
            let key = key;
            // `EVENT_MEDIA_SCAN_PROGRESS` is broadcast to every
            // connected WS client regardless of cid (mirrors the Go
            // side: the scanner uses `Publish(topic, payload)` with a
            // 1-arg handler signature, and `ws.go` registers a
            // matching 1-arg `scanHandler`). Use the no-cid
            // `subscribe` channel for parity.
            eventbus::EventBus::global().subscribe(
                consts::EVENT_MEDIA_SCAN_PROGRESS,
                move |payload: JsonValue| {
                    if let Some(m) = Self::encode(&key, 41, &payload) {
                        let _ = tx.unbounded_send(m);
                    }
                },
            )
        };
        let sub_id_task = {
            let cid = cid.clone();
            let tx = tx.clone();
            let key = key;
            eventbus::EventBus::global().subscribe_with_cid(
                consts::EVENT_FILE_TASK_PROGRESS,
                move |event_cid: String, payload: JsonValue| {
                    if event_cid != cid {
                        return;
                    }
                    if let Some(m) = Self::encode(&key, 42, &payload) {
                        let _ = tx.unbounded_send(m);
                    }
                },
            )
        };
        let sub_id_dlna_found = {
            let cid = cid.clone();
            let tx = tx.clone();
            let key = key;
            eventbus::EventBus::global().subscribe_with_cid(
                consts::EVENT_DLNA_RENDERER_FOUND,
                move |event_cid: String, payload: JsonValue| {
                    if event_cid != cid {
                        return;
                    }
                    if let Some(m) = Self::encode(&key, 43, &payload) {
                        let _ = tx.unbounded_send(m);
                    }
                },
            )
        };
        let sub_id_dlna_done = {
            let cid = cid.clone();
            let tx = tx.clone();
            let key = key;
            eventbus::EventBus::global().subscribe_with_cid(
                consts::EVENT_DLNA_DISCOVERY_DONE,
                move |event_cid: String, payload: JsonValue| {
                    if event_cid != cid {
                        return;
                    }
                    if let Some(m) = Self::encode(&key, 44, &payload) {
                        let _ = tx.unbounded_send(m);
                    }
                },
            )
        };

        let sub_id_format = {
            let tx = tx.clone();
            // Broadcast like the scan progress: every connected client
            // (including the one that triggered the format) learns the
            // disk is ready without polling `mounts`.
            eventbus::EventBus::global().subscribe(
                consts::EVENT_DISK_FORMAT_DONE,
                move |payload: JsonValue| {
                    if let Some(m) = Self::encode(&key, 45, &payload) {
                        let _ = tx.unbounded_send(m);
                    }
                },
            )
        };

        let sub_id_chat = {
            let tx = tx.clone();
            let key = key;
            // Broadcast: chat state is server-wide (single-user NAS).
            eventbus::EventBus::global().subscribe(consts::EVENT_CHAT, move |payload: JsonValue| {
                if let Some(m) = Self::encode_chat(&key, &payload) {
                    let _ = tx.unbounded_send(m);
                }
            })
        };

        {
            let mut g = self.inner.lock().expect("ws_hub lock poisoned");
            g.conns.insert(
                cid.clone(),
                ConnHandle {
                    tx: tx.clone(),
                    sub_id_scan,
                    sub_id_task,
                    sub_id_dlna_found,
                    sub_id_dlna_done,
                    sub_id_format,
                    sub_id_chat,
                    _key: key,
                },
            );
        }

        // Writer task: forward queued messages to the socket.
        tokio::spawn(async move {
            let mut sink = sink;
            while let Some(msg) = rx.next().await {
                if sink.send(msg).await.is_err() {
                    break;
                }
            }
        });
    }

    pub fn unregister(&self, cid: &str) {
        crate::log::info!("[ws_hub] unregister cid={}", cid);
        let mut g = self.inner.lock().expect("ws_hub lock poisoned");
        if let Some(h) = g.conns.remove(cid) {
            // Best-effort: ask the writer task to terminate by closing the
            // socket. Failures here are ignored — the connection may have
            // already been torn down by the peer.
            let _ = h.tx.unbounded_send(Message::Close(None));
            let bus = eventbus::EventBus::global();
            bus.unsubscribe(h.sub_id_scan);
            bus.unsubscribe(h.sub_id_task);
            bus.unsubscribe(h.sub_id_dlna_found);
            bus.unsubscribe(h.sub_id_dlna_done);
            bus.unsubscribe(h.sub_id_format);
            bus.unsubscribe(h.sub_id_chat);
        }
    }
}

use std::sync::LazyLock as Lazy;
static HUB: Lazy<WsHub> = Lazy::new(WsHub::new);
pub fn global() -> &'static WsHub {
    &HUB
}

#[cfg(test)]
#[path = "../tests/unit/ws_hub.rs"]
mod tests;
