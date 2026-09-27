//! Channel CRUD operations on [`ChatService`] — the business logic behind
//! the `createChatChannel` / `updateChatChannel` / … mutations. Port of
//! plain-desktop's `ChatChannelMutation` resolvers, minus the GraphQL
//! layer; error strings are the wire messages the resolvers surface.

use crate::base64_decode;
use crate::random_bytes;

use crate::chat::db::{DChannel, now_iso};
use crate::chat::enums::ChannelStatus;
use crate::chat::events::{
    ChatEvent, WS_CHANNELS_UPDATED, channels_updated_payload, load_key_cache,
    refresh_peer_key_cache,
};
use crate::chat::service::ChatService;
use crate::chat::transport::PeerTransport;

use super::messages::{ChannelMember, decode_members, encode_members, has_member};
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
    pub fn create_channel(&self, name: &str) -> DChannel {
        let mut ch = DChannel::new(name.trim(), &self.identity.client_id);
        ch.members = encode_members(&[ChannelMember::new(&self.identity.client_id)]);
        ch.key = crate::base64_encode(&random_bytes(32));
        self.db.insert_channel(&ch);
        self.emit_channels_updated();
        ch
    }

    pub async fn update_channel_name(&self, id: &str, name: &str) -> Result<DChannel, String> {
        let Some(mut ch) = self.db.get_channel_by_id(id) else {
            return Err("Channel not found".to_string());
        };
        ch.name = name.trim().to_string();
        ch.version += 1;
        ch.updated_at = now_iso();
        self.db.update_channel(&ch);
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
        if let Some(mut ch) = self.db.get_channel_by_id(id) {
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
            ch.status = ChannelStatus::Left;
            ch.updated_at = now_iso();
            self.db.update_channel(&ch);
        }
        self.db.delete_chats_by_channel(id);
        self.db.delete_channel(id);
        refresh_peer_key_cache(&self.db, &self.peer_key_cache);
        self.emit_channels_updated();
        true
    }

    pub async fn leave_channel(&self, id: &str) -> bool {
        if let Some(mut ch) = self.db.get_channel_by_id(id) {
            if ch.owner_id != self.identity.client_id {
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
                let new_members: Vec<ChannelMember> = decode_members(&ch.members)
                    .into_iter()
                    .filter(|m| m.peer_id != self.identity.client_id)
                    .collect();
                ch.members = encode_members(&new_members);
                ch.status = ChannelStatus::Left;
                ch.updated_at = now_iso();
                self.db.update_channel(&ch);
                refresh_peer_key_cache(&self.db, &self.peer_key_cache);
            }
            self.emit_channels_updated();
        }
        true
    }

    pub async fn add_channel_member(&self, id: &str, peer_id: &str) -> Result<DChannel, String> {
        let Some(mut ch) = self.db.get_channel_by_id(id) else {
            return Err("Channel not found".to_string());
        };
        if ch.owner_id != self.identity.client_id {
            return Err("Only owner can add members".to_string());
        }
        let mut new_members = decode_members(&ch.members);
        if has_member(&new_members, peer_id) {
            return Err("Already a member".to_string());
        }
        new_members.push(ChannelMember::pending(peer_id));
        ch.members = encode_members(&new_members);
        ch.version += 1;
        ch.updated_at = now_iso();
        self.db.update_channel(&ch);

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
        let Some(mut ch) = self.db.get_channel_by_id(id) else {
            return Err("Channel not found".to_string());
        };
        if ch.owner_id != self.identity.client_id {
            return Err("Only owner can remove members".to_string());
        }
        let members = decode_members(&ch.members);
        if !has_member(&members, peer_id) {
            return Err("Not a member".to_string());
        }
        let new_members: Vec<ChannelMember> = members
            .into_iter()
            .filter(|m| m.peer_id != peer_id)
            .collect();
        ch.members = encode_members(&new_members);
        ch.version += 1;
        ch.updated_at = now_iso();
        self.db.update_channel(&ch);

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
        let Some(ch) = self.db.get_channel_by_id(id) else {
            return Err("Channel not found".to_string());
        };
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
        self.db.delete_chats_by_channel(&ch.id);
        self.db.delete_channel(&ch.id);
        refresh_peer_key_cache(&self.db, &self.peer_key_cache);
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
