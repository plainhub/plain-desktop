use crate::base64_decode;
use crate::chat::channel::handler as channel_handler;
use crate::chat::enums::ChannelSystemMessageType;
use crate::chat::events::{WS_CHANNELS_UPDATED, WS_MESSAGE_CREATED, channels_updated_payload};
use crate::chat::manager::chat_to_json;
use crate::chat::service::ChatService;
use crate::chat::transport::PeerTransport;
use crate::db::DChat;
use serde_json::json;

impl<T: PeerTransport + 'static> ChatService<T> {
    /// Persist an incoming chat item from a peer and broadcast
    /// `WS_MESSAGE_CREATED`. Returns the persisted row (or a synthetic,
    /// un-persisted row when the channel gate drops the message) for the
    /// GraphQL response.
    pub fn receive_peer_chat(
        &self,
        from_id: &str,
        channel_id: &str,
        content: &str,
        signature: &str,
        timestamp: i64,
    ) -> Result<Option<DChat>, String> {
        let Some(received) = super::message_lifecycle::receive(
            &self.db, from_id, channel_id, content, signature, timestamp,
        )
        .map_err(|e| e.to_string())?
        else {
            return Ok(None);
        };
        let chat = received.chat;
        self.cacher.load(&self.db);
        self.emit(WS_MESSAGE_CREATED, json!([chat_to_json(&chat)]).to_string());

        self.spawn_link_preview_refresh(&chat.id, &chat.content);
        Ok(Some(chat))
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
}
