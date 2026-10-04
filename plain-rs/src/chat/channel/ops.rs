//! Channel CRUD operations on [`ChatService`] — the business logic behind
//! the `createChatChannel` / `updateChatChannel` / … mutations. Port of
//! plain-desktop's `ChatChannelMutation` resolvers, minus the GraphQL
//! layer; error strings are the wire messages the resolvers surface.

use crate::base64_decode;

use crate::chat::events::{
    ChatEvent, WS_CHANNELS_UPDATED, channels_updated_payload, load_key_cache,
    refresh_peer_key_cache,
};
use crate::chat::service::ChatService;
use crate::chat::transport::PeerTransport;
use crate::db::DChannel;

use super::sender;

impl<T: PeerTransport + 'static> ChatService<T> {
    fn emit_channels_updated(&self) {
        let _ = self.event_tx.send(ChatEvent {
            event_type: WS_CHANNELS_UPDATED,
            payload: channels_updated_payload(&self.db),
        });
    }

    /// Mirror plain-app `ChannelManager.createChannel`: the owner is a
    /// member from the start (JOINED) and the per-channel ChaCha20 key is
    /// generated immediately. Without the owner in `members`,
    /// `build_member_peers` omits it from the invite's `memberPeers`, so
    /// the invitee rejects the invite ("no owner memberPeerInfo").
    pub fn create_channel(&self, name: &str) -> Result<DChannel, String> {
        let channel = super::state::create(&self.db, &self.identity.client_id, name)
            .map_err(|e| e.to_string())?;
        self.emit_channels_updated();
        Ok(channel)
    }

    pub async fn update_channel_name(&self, id: &str, name: &str) -> Result<DChannel, String> {
        let ch = super::state::apply(
            &self.db,
            &self.identity.client_id,
            id,
            super::state::Action::Rename { name: name.into() },
        )
        .map_err(|e| e.to_string())?;
        if ch.owner_id == self.identity.client_id {
            let kp_bytes = base64_decode(&self.identity.ed25519_keypair);
            let channel_key = base64_decode(&ch.key);
            sender::broadcast_update(
                &self.transport,
                &ch,
                &self.identity.client_id,
                &self.identity.device_name(),
                self.wire_device_type,
                &kp_bytes,
                &self.db,
                &self.peer_key_cache,
                &channel_key,
            )
            .await;
        }
        self.emit_channels_updated();
        Ok(ch)
    }

    pub async fn delete_channel(&self, id: &str) -> bool {
        if let Some(ch) = self.db.get_channel_by_id(id) {
            if ch.owner_id == self.identity.client_id {
                let kp_bytes = base64_decode(&self.identity.ed25519_keypair);
                let channel_key = base64_decode(&ch.key);
                sender::broadcast_kick(
                    &self.transport,
                    &ch,
                    &self.identity.client_id,
                    &kp_bytes,
                    &self.db,
                    &self.peer_key_cache,
                    &channel_key,
                )
                .await;
            }
        }
        if let Err(error) = crate::chat::app_file_store::chat_deletion::delete(
            &self.db,
            &self.data_dir,
            crate::chat::app_file_store::chat_deletion::Selection::ChannelRecord(id),
        ) {
            log::error!("chat deletion failed: {error}");
            return false;
        }
        refresh_peer_key_cache(&self.db, &self.peer_key_cache);
        self.cacher.load(&self.db);
        self.emit_channels_updated();
        true
    }

    pub async fn leave_channel(&self, id: &str) -> bool {
        let ch = match super::state::apply(
            &self.db,
            &self.identity.client_id,
            id,
            super::state::Action::Leave,
        ) {
            Ok(ch) => ch,
            Err(error) => {
                log::error!("channel leave failed: {error}");
                return false;
            }
        };
        if let Some(owner_peer) = self.db.get_peer_by_id(&ch.owner_id) {
            let kp_bytes = base64_decode(&self.identity.ed25519_keypair);
            let channel_key = base64_decode(&ch.key);
            let _ = sender::send_leave(
                &self.transport,
                &ch.id,
                &owner_peer,
                &self.identity.client_id,
                &kp_bytes,
                &channel_key,
                &self.peer_key_cache,
            )
            .await;
        }
        refresh_peer_key_cache(&self.db, &self.peer_key_cache);
        self.emit_channels_updated();
        true
    }

    pub async fn add_channel_member(&self, id: &str, peer_id: &str) -> Result<DChannel, String> {
        let ch = super::state::apply(
            &self.db,
            &self.identity.client_id,
            id,
            super::state::Action::Invite {
                peer: peer_id.into(),
            },
        )
        .map_err(|e| e.to_string())?;

        if let Some(peer) = self.db.get_peer_by_id(peer_id) {
            let kp_bytes = base64_decode(&self.identity.ed25519_keypair);
            let channel_key = base64_decode(&ch.key);
            let _ = sender::send_invite(
                &self.transport,
                &ch,
                &peer,
                &self.identity.client_id,
                &self.identity.device_name(),
                self.wire_device_type,
                &kp_bytes,
                &self.db,
                &self.peer_key_cache,
                &channel_key,
            )
            .await;
        }
        self.emit_channels_updated();
        Ok(ch)
    }

    pub async fn remove_channel_member(&self, id: &str, peer_id: &str) -> Result<DChannel, String> {
        let ch = super::state::apply(
            &self.db,
            &self.identity.client_id,
            id,
            super::state::Action::Kick {
                peer: peer_id.into(),
            },
        )
        .map_err(|e| e.to_string())?;

        let kp_bytes = base64_decode(&self.identity.ed25519_keypair);
        let channel_key = base64_decode(&ch.key);
        if let Some(peer) = self.db.get_peer_by_id(peer_id) {
            let _ = sender::send_kick(
                &self.transport,
                &ch.id,
                ch.version,
                &peer,
                &self.identity.client_id,
                &kp_bytes,
                &channel_key,
                &self.peer_key_cache,
            )
            .await;
        }
        sender::broadcast_update(
            &self.transport,
            &ch,
            &self.identity.client_id,
            &self.identity.device_name(),
            self.wire_device_type,
            &kp_bytes,
            &self.db,
            &self.peer_key_cache,
            &channel_key,
        )
        .await;
        self.emit_channels_updated();
        Ok(ch)
    }

    pub async fn accept_channel_invite(&self, id: &str) -> Result<bool, String> {
        let ch = super::state::apply(
            &self.db,
            &self.identity.client_id,
            id,
            super::state::Action::Accept,
        )
        .map_err(|e| e.to_string())?;
        let Some(owner_peer) = self.db.get_peer_by_id(&ch.owner_id) else {
            return Err("Owner peer not found".to_string());
        };
        let kp_bytes = base64_decode(&self.identity.ed25519_keypair);
        let channel_key = base64_decode(&ch.key);
        load_key_cache(&self.db, &self.peer_key_cache, &self.channel_key_cache);
        let _ = sender::send_invite_accept(
            &self.transport,
            &ch.id,
            &owner_peer,
            &self.identity.client_id,
            &kp_bytes,
            &self.identity.device_name(),
            self.wire_device_type,
            &channel_key,
            &self.peer_key_cache,
        )
        .await;
        Ok(true)
    }

    pub async fn decline_channel_invite(&self, id: &str) -> bool {
        let Some(ch) = self.db.get_channel_by_id(id) else {
            return true;
        };
        if let Some(owner_peer) = self.db.get_peer_by_id(&ch.owner_id) {
            let kp_bytes = base64_decode(&self.identity.ed25519_keypair);
            let channel_key = base64_decode(&ch.key);
            let _ = sender::send_invite_decline(
                &self.transport,
                &ch.id,
                &owner_peer,
                &self.identity.client_id,
                &kp_bytes,
                &channel_key,
                &self.peer_key_cache,
            )
            .await;
        }
        if let Err(error) = crate::chat::app_file_store::chat_deletion::delete(
            &self.db,
            &self.data_dir,
            crate::chat::app_file_store::chat_deletion::Selection::ChannelRecord(&ch.id),
        ) {
            log::error!("chat deletion failed: {error}");
            return false;
        }
        refresh_peer_key_cache(&self.db, &self.peer_key_cache);
        self.cacher.load(&self.db);
        self.emit_channels_updated();
        true
    }

    /// Web-only convenience that branches to accept or decline based on
    /// the `accept` flag (plain-app's Android schema doesn't expose this —
    /// the web client added it so `ChannelInviteModal` can use a single
    /// GraphQL document for both buttons). Returns the flag verbatim.
    pub async fn respond_channel_invite(&self, id: &str, accept: bool) -> bool {
        if accept {
            let _ = self.accept_channel_invite(id).await;
        } else {
            let _ = self.decline_channel_invite(id).await;
        }
        accept
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/chat/channel/ops.rs"]
mod tests;
