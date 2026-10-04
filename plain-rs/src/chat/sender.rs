use crate::base64_decode;
use crate::chat::channel::chat_helper::{ChannelDeliveryResult, SendResult, send};
use crate::chat::content::to_peer_content;
use crate::chat::events::{ChatEvent, WS_MESSAGE_UPDATED, load_key_cache};
use crate::chat::manager::chat_to_json;
use crate::chat::service::ChatService;
use crate::chat::transport::{PeerTransport, deliver_to_peer, peer_graphql_urls};
use crate::db::DChat;
use serde_json::json;

impl<T: PeerTransport + 'static> ChatService<T> {
    // ── delivery plumbing ────────────────────────────────────────────────

    /// Mirrors `ChatSender.sendToPeer` — spawn async peer delivery and
    /// update the chat status from the result.
    fn spawn_peer_delivery(&self, chat: &DChat) {
        let peer_id = chat.to_id.clone();
        let Some(peer) = self.db.get_peer_by_id(&peer_id) else {
            apply_delivery(&self.db, &self.event_tx, &chat.id, None);
            return;
        };
        if !peer.is_paired() {
            apply_delivery(
                &self.db,
                &self.event_tx,
                &chat.id,
                Some(vec![ChannelDeliveryResult {
                    peer_id: peer.id.clone(),
                    peer_name: peer.name.clone(),
                    error: Some("peer unpaired".into()),
                }]),
            );
            return;
        }
        let key = {
            let cache = self.peer_key_cache.read().unwrap();
            cache.get(&peer_id).cloned()
        }
        .or_else(|| {
            let raw = base64_decode(&peer.key);
            if raw.len() == 32 { Some(raw) } else { None }
        });
        let Some(key) = key else {
            apply_delivery(&self.db, &self.event_tx, &chat.id, None);
            return;
        };

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
            let results = match delivery_result {
                Ok(()) => vec![],
                Err(error) => vec![ChannelDeliveryResult {
                    peer_id: peer_id_for_status,
                    peer_name: peer_name_for_status,
                    error: Some(error),
                }],
            };
            apply_delivery(&db, &event_tx, &chat_id, Some(results));
        });
    }

    /// Mirrors `ChatSender.sendToChannel` — spawn async channel delivery
    /// and update the chat status from the per-member results.
    fn spawn_channel_delivery(&self, chat: &DChat) {
        let channel_id = chat.channel_id.clone();
        let Some(channel) = self.db.get_channel_by_id(&channel_id) else {
            apply_delivery(&self.db, &self.event_tx, &chat.id, None);
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

            let results = match result {
                SendResult::Status(results) => Some(results),
                SendResult::NoLeader | SendResult::LeaderPeerMissing(()) => {
                    hooks.rebrowse_peers();
                    None
                }
            };
            apply_delivery(&db, &event_tx, &chat_id, results);
        });
    }

    /// Mirrors `ChatSender.send` — route to peer or channel delivery based
    /// on the chat item's target. Local notes (`to_id == "local"`) are
    /// skipped.
    pub(super) fn spawn_delivery(&self, chat: &DChat) {
        if chat.to_id == "local" {
            return;
        }
        if !chat.to_id.is_empty() && chat.channel_id.is_empty() {
            self.spawn_peer_delivery(chat);
        } else if !chat.channel_id.is_empty() {
            self.spawn_channel_delivery(chat);
        }
    }
}

fn apply_delivery(
    db: &crate::db::Db,
    event_tx: &tokio::sync::broadcast::Sender<ChatEvent>,
    id: &str,
    results: Option<Vec<ChannelDeliveryResult>>,
) {
    match super::message_lifecycle::delivery(db, id, results, false) {
        Ok(Some(updated)) => {
            let _ = event_tx.send(ChatEvent {
                event_type: WS_MESSAGE_UPDATED,
                payload: json!([chat_to_json(&updated)]).to_string(),
            });
        }
        Ok(None) => {}
        Err(error) => log::error!("chat delivery status failed: {error}"),
    }
}
