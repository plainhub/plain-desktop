//! Chat service — the single place where chat items are created,
//! delivered, retried, and received from peers. Port of plain-desktop's
//! `chat_handler` (itself the port of plain-app `ChatManager` /
//! `ChatSender`).
//!
//! GraphQL resolvers (local and peer) deliberately contain no business
//! logic: they parse the wire arguments and delegate here. App-specific
//! side effects plug in through [`ChatHooks`], the [`LinkPreviewFn`]
//! async closure, and the [`PeerTransport`] type parameter.

use std::path::PathBuf;
use std::sync::Arc;

use futures_core::future::BoxFuture;
use serde_json::{Value, json};
use tokio::sync::broadcast;

use crate::base64_decode;

use crate::chat::cacher::ChatCacher;
use crate::chat::channel::chat_helper::{
    SendResult, build_no_leader_status_data, build_status_data_json, compute_status, send,
};
use crate::chat::channel::handler as channel_handler;
use crate::chat::content::make_file_id;
use crate::chat::db::{ChatDb, DChat};
use crate::chat::enums::{ChannelStatus, ChannelSystemMessageType, ChatStatus, DeviceType};
use crate::chat::events::{
    ChannelKeyCache, ChatEvent, PeerKeyCache, WS_CHANNELS_UPDATED, WS_MESSAGE_CREATED,
    WS_MESSAGE_DELETED, WS_MESSAGE_UPDATED, WS_PEER_STATUS_UPDATED, channels_updated_payload,
    load_key_cache, new_channel_key_cache, new_peer_key_cache, refresh_peer_key_cache,
};
use crate::chat::transport::{PeerTransport, deliver_to_peer, peer_graphql_urls};

/// Local device identity for pairing and message signing.
pub struct ChatIdentity {
    pub client_id: String,
    /// Display name — interior-mutable so a runtime rename (the
    /// `updateDeviceName` mutation) propagates to every reader of the
    /// shared `Arc<ChatIdentity>`.
    device_name: std::sync::RwLock<String>,
    /// Base64 64-byte Ed25519 keypair — same value as plain-app's
    /// `TempData.ed25519Keypair`.
    pub ed25519_keypair: String,
}

impl ChatIdentity {
    pub fn new(
        client_id: impl Into<String>,
        device_name: impl Into<String>,
        ed25519_keypair: impl Into<String>,
    ) -> Self {
        Self {
            client_id: client_id.into(),
            device_name: std::sync::RwLock::new(device_name.into()),
            ed25519_keypair: ed25519_keypair.into(),
        }
    }

    pub fn device_name(&self) -> String {
        self.device_name.read().unwrap().clone()
    }

    pub fn set_device_name(&self, name: &str) {
        *self.device_name.write().unwrap() = name.to_string();
    }
}

/// App-specific side effects of the chat flow.
pub trait ChatHooks: Send + Sync {
    /// Called after a failed peer delivery — usually means the peer's
    /// IP/port changed. plain-desktop kicks an mDNS re-browse here.
    fn rebrowse_peers(&self) {}
}

/// No-op hooks.
pub struct NoChatHooks;
impl ChatHooks for NoChatHooks {}

/// Async link-preview seam: rewrite a persisted message's `content` with
/// link previews filled in (`None` = leave unchanged). plain-desktop
/// scrapes OpenGraph data; the NAS side starts as a no-op.
pub type LinkPreviewFn =
    Arc<dyn Fn(ChatDb, PathBuf, String) -> BoxFuture<'static, Option<String>> + Send + Sync>;

/// A `LinkPreviewFn` that never rewrites content.
pub fn no_link_previews() -> LinkPreviewFn {
    Arc::new(|_db, _data_dir, _content| Box::pin(async { None }))
}

/// All chat-domain state and the entry points of the business logic.
/// Generic over the app's HTTP transport to peers.
pub struct ChatService<T: PeerTransport> {
    pub db: ChatDb,
    pub cacher: ChatCacher,
    /// Base64 local URL token — the key behind `/fs` file ids.
    pub token: String,
    /// Shared with PairingManager so a device-name update propagates.
    pub identity: std::sync::Arc<ChatIdentity>,
    /// Device type advertised in channel wire traffic
    /// (COMPUTER on desktop, NAS on plain-nas).
    pub wire_device_type: DeviceType,
    /// App data directory (app-file store root).
    pub data_dir: PathBuf,
    pub transport: Arc<T>,
    pub event_tx: broadcast::Sender<ChatEvent>,
    pub peer_key_cache: PeerKeyCache,
    pub channel_key_cache: ChannelKeyCache,
    pub hooks: Arc<dyn ChatHooks>,
    pub link_previews: LinkPreviewFn,
}

impl<T: PeerTransport + 'static> ChatService<T> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        db: ChatDb,
        token: String,
        identity: std::sync::Arc<ChatIdentity>,
        wire_device_type: DeviceType,
        data_dir: PathBuf,
        transport: Arc<T>,
        hooks: Arc<dyn ChatHooks>,
        link_previews: LinkPreviewFn,
    ) -> Self {
        let (event_tx, _) = broadcast::channel(256);
        let peer_key_cache = new_peer_key_cache();
        let channel_key_cache = new_channel_key_cache();
        let cacher = ChatCacher::new();
        cacher.load(&db);
        load_key_cache(&db, &peer_key_cache, &channel_key_cache);
        Self {
            db,
            cacher,
            token,
            identity,
            wire_device_type,
            data_dir,
            transport,
            event_tx,
            peer_key_cache,
            channel_key_cache,
            hooks,
            link_previews,
        }
    }

    fn emit(&self, event_type: i32, payload: String) {
        let _ = self.event_tx.send(ChatEvent {
            event_type,
            payload,
        });
    }

    /// Build the wire JSON for a single chat item. Used for both GraphQL
    /// response data and `WS_MESSAGE_CREATED` / `WS_MESSAGE_UPDATED`
    /// payloads. File ids are no longer embedded — clients derive them
    /// from `content` with their own urlToken.
    pub fn chat_to_json(&self, c: &DChat) -> Value {
        chat_to_json(c)
    }

    /// Send a chat item. Mirrors `ChatSender.send`:
    ///   * `peer:<id>`    — peer-to-peer (encrypts with the peer shared key)
    ///   * `channel:<id>` — channel (star topology, leader election)
    ///   * anything else  — local note
    ///
    /// Delivery is spawned fire-and-forget; the final status is published
    /// via `WS_MESSAGE_UPDATED`. Returns the initially-inserted row (status
    /// `PENDING` for remote targets) so the caller can render immediately.
    pub fn send_chat_item(&self, to_id: String, content: String) -> Vec<DChat> {
        let is_channel = to_id.starts_with("channel:");
        let is_peer = to_id.starts_with("peer:");
        let peer_id = if is_peer {
            to_id.strip_prefix("peer:").unwrap_or(&to_id).to_string()
        } else {
            String::new()
        };
        let channel_id = if is_channel {
            to_id.strip_prefix("channel:").unwrap_or("").to_string()
        } else {
            String::new()
        };
        let to = if is_peer {
            peer_id.clone()
        } else if is_channel {
            String::new()
        } else {
            to_id.clone()
        };

        let is_remote = (is_peer && !peer_id.is_empty() && peer_id != "local") || is_channel;
        let mut chat = DChat::new("me", &to, &channel_id, &content);
        if is_remote {
            chat.status = ChatStatus::Pending;
        }
        self.db.insert_chat(&chat);

        if is_remote {
            self.spawn_delivery(&chat);
        }

        self.emit(
            WS_MESSAGE_CREATED,
            json!([chat_to_json(&chat)]).to_string(),
        );

        self.spawn_link_preview_refresh(&chat.id, &chat.content);

        vec![chat]
    }

    /// Delete a single chat item and broadcast `WS_MESSAGE_DELETED`.
    pub fn delete_chat_item(&self, id: String) -> bool {
        if self.db.get_chat_by_id(&id).is_none() {
            return false;
        }
        self.db.delete_chat(&id);
        self.emit(WS_MESSAGE_DELETED, json!([id]).to_string());
        true
    }

    /// Bulk-delete chats by query (see `resolve_chat_ids`). Emits a single
    /// `WS_MESSAGE_DELETED` event whose payload is the `ids=...` string the
    /// web's `message_deleted` handler expects.
    pub fn delete_chat_items(&self, query: String) -> i32 {
        let ids = resolve_chat_ids(&self.db, &query);
        if ids.is_empty() {
            return 0;
        }
        self.db.delete_chats_by_ids(&ids);
        self.emit(WS_MESSAGE_DELETED, format!("ids={}", ids.join(",")));
        ids.len() as i32
    }

    /// Retry a failed chat item: set status to `PENDING`, emit
    /// `WS_MESSAGE_UPDATED`, then re-deliver via the same `ChatSender.send`
    /// path. The final status is computed from the actual delivery results.
    pub fn retry_chat_item(&self, id: String) -> Option<DChat> {
        let chat = self.db.get_chat_by_id(&id)?;

        // Broadcast with the updated (pending) row so the UI switches to
        // "sending" immediately — the stale object still carries FAILED.
        let updated = self.db.update_chat_status(&id, ChatStatus::Pending)?;
        self.emit(
            WS_MESSAGE_UPDATED,
            json!([chat_to_json(&updated)]).to_string(),
        );

        self.spawn_delivery(&chat);

        self.db.get_chat_by_id(&id)
    }

    /// Persist an incoming chat item from a peer and broadcast
    /// `WS_MESSAGE_CREATED`. Returns the persisted row (or a synthetic,
    /// un-persisted row when the channel gate drops the message) for the
    /// GraphQL response.
    pub fn receive_peer_chat(&self, from_id: &str, channel_id: &str, content: &str) -> DChat {
        // Channel membership gate (mirrors Kotlin's
        // `PeerGraphQL.createChatItem` IllegalStateException("Channel not joined")).
        if !channel_id.is_empty() {
            match self.db.get_channel_by_id(channel_id) {
                Some(ch)
                    if ch.status == ChannelStatus::Joined || ch.status == ChannelStatus::Kicked => {
                }
                Some(_) => {
                    log::warn!(
                        "[peer_graphql] dropping chat for channel {channel_id} in status {}",
                        self.db
                            .get_channel_by_id(channel_id)
                            .map(|c| c.status)
                            .unwrap_or_default()
                    );
                    return DChat::new(from_id, "", channel_id, content);
                }
                None => {
                    log::warn!("[peer_graphql] dropping chat for unknown channel {channel_id}");
                    return DChat::new(from_id, "", channel_id, content);
                }
            }
        }
        let to_id = if channel_id.is_empty() { "me" } else { "" };
        let chat = DChat::new(from_id, to_id, channel_id, content);
        self.db.insert_chat(&chat);
        self.emit(
            WS_MESSAGE_CREATED,
            json!([chat_to_json(&chat)]).to_string(),
        );

        self.spawn_link_preview_refresh(&chat.id, &chat.content);
        chat
    }

    /// Dispatch an incoming `channelSystemMessage` to the local channel handler
    /// and broadcast `WS_CHANNELS_UPDATED` so local UI can refresh. Returns the
    /// boolean the peer expects from the GraphQL contract.
    pub fn receive_peer_channel_system_message(
        &self,
        from_id: &str,
        msg_type: ChannelSystemMessageType,
        payload: &str,
    ) -> bool {
        let kp_bytes = base64_decode(&self.identity.ed25519_keypair);
        let ok = channel_handler::handle(self, from_id, msg_type, payload, &kp_bytes);
        self.emit(WS_CHANNELS_UPDATED, channels_updated_payload(&self.db));
        ok
    }

    /// Mirrors plain-app `PeerManager.deletePeer(peerId)`:
    ///   1. Delete all 1:1 chats with the peer.
    ///   2. If the peer is still a member of any local channel, demote it
    ///      to `status="CHANNEL"` with an empty shared key — the row
    ///      must remain so channel routing can still resolve it.
    ///   3. Otherwise delete the peer row outright.
    ///   4. Refresh the peer key cache.
    ///
    /// Returns `false` if the peer id is unknown, `true` otherwise.
    pub fn delete_peer(&self, id: &str) -> bool {
        use crate::chat::enums::PeerStatus;
        if self.db.get_peer_by_id(id).is_none() {
            return false;
        }
        self.db.delete_chats_by_peer(id);
        if self.db.any_channel_has_member(id) {
            self.db
                .update_peer_status_and_key(id, PeerStatus::Channel, "");
        } else {
            self.db.delete_peer(id);
        }
        refresh_peer_key_cache(&self.db, &self.peer_key_cache);
        self.emit(
            WS_PEER_STATUS_UPDATED,
            json!({ "id": id, "online": false }).to_string(),
        );
        true
    }

    /// Mirrors plain-app `PeerManager.markUnpaired(peerId)`: flips the
    /// peer's status to "UNPAIRED" and bumps `updated_at`, leaving the
    /// shared key intact so a future re-pair can reuse the stored
    /// credentials. Returns `false` if the peer id is unknown.
    pub fn unpair_peer(&self, id: &str) -> bool {
        use crate::chat::enums::PeerStatus;
        if self.db.get_peer_by_id(id).is_none() {
            return false;
        }
        self.db.update_peer_status(id, PeerStatus::Unpaired);
        refresh_peer_key_cache(&self.db, &self.peer_key_cache);
        self.emit(
            WS_PEER_STATUS_UPDATED,
            json!({ "id": id, "online": false }).to_string(),
        );
        true
    }

    // ── delivery plumbing ────────────────────────────────────────────────

    /// Mirrors `ChatSender.sendToPeer` — spawn async peer delivery and
    /// update the chat status from the result.
    fn spawn_peer_delivery(&self, chat: &DChat) {
        let peer_id = chat.to_id.clone();
        let Some(peer) = self.db.get_peer_by_id(&peer_id) else {
            return;
        };
        let key = {
            let cache = self.peer_key_cache.read().unwrap();
            cache.get(&peer_id).cloned()
        }
        .or_else(|| {
            let raw = base64_decode(&peer.key);
            if raw.len() == 32 { Some(raw) } else { None }
        });
        let Some(key) = key else { return };

        let chat_id = chat.id.clone();
        let peer_urls = peer_graphql_urls(&peer);
        let client_id = self.identity.client_id.clone();
        let kp_bytes = base64_decode(&self.identity.ed25519_keypair);
        let content_str = to_peer_content(&chat.content, &self.token);
        let event_tx = self.event_tx.clone();
        let db = self.db.clone();
        let peer_id_for_status = peer.id.clone();
        let peer_name_for_status = peer.name.clone();
        let transport = self.transport.clone();
        let hooks = self.hooks.clone();
        tokio::spawn(async move {
            let delivery_result = deliver_to_peer(
                &transport,
                &peer_urls,
                &key,
                &client_id,
                &kp_bytes,
                &content_str,
                None,
            )
            .await;
            if delivery_result.is_err() {
                // A failed send usually means the peer's IP/port changed —
                // kick a re-browse so the peer row refreshes for next time.
                hooks.rebrowse_peers();
            }
            let (new_status, status_data) = match delivery_result {
                Ok(()) => (ChatStatus::Sent, String::new()),
                Err(error) => (
                    ChatStatus::Failed,
                    peer_delivery_status_data(&peer_id_for_status, &peer_name_for_status, &error),
                ),
            };
            if let Some(updated) =
                db.update_chat_status_and_data(&chat_id, new_status, &status_data)
            {
                let _ = event_tx.send(ChatEvent {
                    event_type: WS_MESSAGE_UPDATED,
                    payload: json!([chat_to_json(&updated)]).to_string(),
                });
            }
        });
    }

    /// Mirrors `ChatSender.sendToChannel` — spawn async channel delivery
    /// and update the chat status from the per-member results.
    fn spawn_channel_delivery(&self, chat: &DChat) {
        let channel_id = chat.channel_id.clone();
        let Some(channel) = self.db.get_channel_by_id(&channel_id) else {
            return;
        };

        let client_id = self.identity.client_id.clone();
        let kp_bytes = base64_decode(&self.identity.ed25519_keypair);
        let chat_id = chat.id.clone();
        let content_str = to_peer_content(&chat.content, &self.token);
        let db = self.db.clone();
        let event_tx = self.event_tx.clone();
        let peer_key_cache = self.peer_key_cache.clone();
        let channel_key_cache = self.channel_key_cache.clone();
        let transport = self.transport.clone();
        let hooks = self.hooks.clone();

        tokio::spawn(async move {
            {
                let cache = channel_key_cache.read().unwrap();
                if !cache.contains_key(&channel.id) {
                    drop(cache);
                    load_key_cache(&db, &peer_key_cache, &channel_key_cache);
                }
            }

            let result = send(
                &transport,
                &channel,
                &client_id,
                &content_str,
                &db,
                &channel_key_cache,
                &kp_bytes,
            )
            .await;

            let (status, status_data) = match result {
                SendResult::Status(results) => {
                    let s = compute_status(&results);
                    let d = build_status_data_json(&results);
                    (s, d)
                }
                SendResult::NoLeader | SendResult::LeaderPeerMissing(()) => {
                    // No reachable leader/member means stale peer addresses —
                    // kick a re-browse so peers' IP/port refresh.
                    hooks.rebrowse_peers();
                    (ChatStatus::Failed, build_no_leader_status_data())
                }
            };

            if let Some(updated) = db.update_chat_status_and_data(&chat_id, status, &status_data) {
                let _ = event_tx.send(ChatEvent {
                    event_type: WS_MESSAGE_UPDATED,
                    payload: json!([chat_to_json(&updated)]).to_string(),
                });
            }
        });
    }

    /// Mirrors `ChatSender.send` — route to peer or channel delivery based
    /// on the chat item's target. Local notes (`to_id == "local"`) are
    /// skipped.
    fn spawn_delivery(&self, chat: &DChat) {
        if chat.to_id == "local" {
            return;
        }
        if !chat.to_id.is_empty() && chat.channel_id.is_empty() {
            self.spawn_peer_delivery(chat);
        } else if !chat.channel_id.is_empty() {
            self.spawn_channel_delivery(chat);
        }
    }

    /// Async link-preview refresh: rewrite the stored `content` with a
    /// `linkPreviews` array (via the app hook) and broadcast the result
    /// as `WS_MESSAGE_UPDATED`. Fire-and-forget like delivery.
    fn spawn_link_preview_refresh(&self, chat_id: &str, content: &str) {
        let db = self.db.clone();
        let data_dir = self.data_dir.clone();
        let event_tx = self.event_tx.clone();
        let chat_id = chat_id.to_string();
        let content = content.to_string();
        let link_previews = self.link_previews.clone();
        tokio::spawn(async move {
            let Some(new_content) = link_previews(db.clone(), data_dir, content).await else {
                return;
            };
            if db.update_chat_content(&chat_id, &new_content)
                && let Some(updated) = db.get_chat_by_id(&chat_id)
            {
                let _ = event_tx.send(ChatEvent {
                    event_type: WS_MESSAGE_UPDATED,
                    payload: json!([chat_to_json(&updated)]).to_string(),
                });
            }
        });
    }
}

/// Build the wire JSON for a single chat item (camelCase fields). Used for
/// WS payloads and GraphQL mapping. File ids are no longer embedded —
/// clients derive them from `content` with their own urlToken.
pub fn chat_to_json(c: &DChat) -> Value {
    json!({
        "id": c.id, "fromId": c.from_id, "toId": c.to_id,
        "channelId": c.channel_id, "content": c.content,
        "createdAt": c.created_at, "updatedAt": c.updated_at,
        "status": c.status, "statusData": c.status_data,
    })
}

/// Convert `fid:` URIs to `fsid:` URIs for peer delivery. Mirrors
/// `DMessageContent.toPeerMessageContent()`: each item's `uri` is
/// encrypted with the local URL token and prefixed with `fsid:` so the
/// receiver can fetch it via the sender's `/fs` endpoint. The stored
/// content keeps the original `fid:` URIs.
pub fn to_peer_content(content: &str, token: &str) -> String {
    let Ok(mut v) = serde_json::from_str::<Value>(content) else {
        return content.to_string();
    };
    let Some(items) = v
        .get_mut("value")
        .and_then(|v| v.get_mut("items"))
        .and_then(|v| v.as_array_mut())
    else {
        return content.to_string();
    };
    for item in items.iter_mut() {
        if let Some(uri) = item.get("uri").and_then(|u| u.as_str())
            && uri.starts_with("fid:")
        {
            let encrypted = make_file_id(uri, token);
            if !encrypted.is_empty()
                && let Some(obj) = item.as_object_mut()
            {
                obj.insert(
                    "uri".to_string(),
                    Value::String(format!("fsid:{encrypted}")),
                );
            }
        }
    }
    v.to_string()
}

fn peer_delivery_status_data(peer_id: &str, peer_name: &str, error: &str) -> String {
    json!({
        "results": [{
            "peerId": peer_id,
            "peerName": peer_name,
            "error": error,
        }]
    })
    .to_string()
}

/// Resolve a `deleteChatItems(query)` query into the list of chat ids that
/// should be removed. Mirrors plain-app's `ChatDbHelper.getIdsAsync(query)`:
///   * `ids:<comma-separated-ids>`   — return the listed ids verbatim.
///   * `channel:<channelId>`         — every chat id in the channel.
///   * `peer:<peerId>`               — every 1:1 chat id with the peer.
///   * `peer:local`                  — every local-note chat id.
///
/// Returns an empty Vec for an unrecognized / empty query.
pub fn resolve_chat_ids(db: &ChatDb, query: &str) -> Vec<String> {
    let query = query.trim();
    if query.is_empty() {
        return vec![];
    }
    let Some((name, value)) = query.split_once(':') else {
        return vec![];
    };
    match name {
        "ids" => value
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect(),
        "channel" => db
            .get_chats_by_channel(value)
            .into_iter()
            .map(|c| c.id)
            .collect(),
        "peer" => {
            let peer_id = if value == "local" { "local" } else { value };
            db.get_chats_by_peer(peer_id)
                .into_iter()
                .map(|c| c.id)
                .collect()
        }
        _ => vec![],
    }
}

#[cfg(test)]
#[path = "../../tests/unit/chat/service.rs"]
mod tests;
