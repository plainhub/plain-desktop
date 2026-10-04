use crate::chat::{
    events::{ChatEvent, WS_MESSAGE_UPDATED},
    manager::chat_to_json,
    service::ChatService,
    transport::PeerTransport,
};
use crate::db::DChat;
use serde_json::json;

impl<T: PeerTransport + 'static> ChatService<T> {
    pub(super) fn spawn_delivery(&self, chat: &DChat) {
        if chat.to_id == "local" || (chat.to_id.is_empty() && chat.channel_id.is_empty()) {
            return;
        }
        let delivery = self.delivery.clone();
        let transport = self.transport.clone();
        let identity = self.identity.clone();
        let token = self.token.clone();
        let id = chat.id.clone();
        let events = self.event_tx.clone();
        let hooks = self.hooks.clone();
        tokio::spawn(async move {
            match delivery
                .send(
                    &transport,
                    &identity.client_id,
                    &crate::base64_decode(&identity.ed25519_keypair),
                    &token,
                    &id,
                    None,
                )
                .await
            {
                Ok(receipt) => {
                    if receipt.rediscover {
                        hooks.rebrowse_peers();
                    }
                    if let Some(updated) = receipt.chat {
                        let _ = events.send(ChatEvent {
                            event_type: WS_MESSAGE_UPDATED,
                            payload: json!([chat_to_json(&updated)]).to_string(),
                        });
                    }
                }
                Err(error) => log::error!("chat delivery failed: {error}"),
            }
        });
    }
}
