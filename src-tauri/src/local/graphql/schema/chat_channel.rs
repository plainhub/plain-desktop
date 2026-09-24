use async_graphql::{Context, Error as GqlError, Object, Result as GqlResult};
use std::sync::Arc;

use super::super::context::{
    AppCtx, WS_CHANNELS_UPDATED, WsEvent, channels_updated_payload, load_key_cache,
    refresh_peer_key_cache,
};
use super::types::ChatChannel;
use crate::local::channel::messages::{ChannelMember, decode_members, encode_members, has_member};
use crate::local::db::{DChannel, now_iso};
use crate::local::enums::ChannelStatus;
use plain_rs::{base64_decode, base64_encode, random_bytes};

#[derive(Default)]
pub struct ChatChannelMutation;

fn gql_err(msg: impl Into<String>) -> GqlError {
    GqlError::new(msg.into())
}

fn emit_channels_updated(c: &AppCtx) {
    let _ = c.event_tx.send(WsEvent {
        event_type: WS_CHANNELS_UPDATED,
        payload: channels_updated_payload(&c.db),
    });
}

#[Object]
impl ChatChannelMutation {
    async fn create_chat_channel(&self, ctx: &Context<'_>, name: String) -> ChatChannel {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        let mut ch = DChannel::new(name.trim(), &c.identity.client_id);
        // Mirror plain-app `ChannelManager.createChannel`: the owner is a
        // member from the start (JOINED) and the per-channel ChaCha20 key is
        // generated immediately. Without the owner in `members`,
        // `build_member_peers` omits it from the invite's `memberPeers`, so
        // the invitee rejects the invite ("no owner memberPeerInfo").
        ch.members = encode_members(&[ChannelMember::new(&c.identity.client_id)]);
        ch.key = base64_encode(&random_bytes(32));
        c.db.insert_channel(&ch);
        let channel = ChatChannel::from(ch);
        emit_channels_updated(c);
        channel
    }

    async fn update_chat_channel(
        &self,
        ctx: &Context<'_>,
        id: String,
        name: String,
    ) -> GqlResult<ChatChannel> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>().clone();
        let mut ch =
            c.db.get_channel_by_id(&id)
                .ok_or_else(|| gql_err("Channel not found"))?;
        ch.name = name.trim().to_string();
        ch.version += 1;
        ch.updated_at = now_iso();
        c.db.update_channel(&ch);
        if ch.owner_id == c.identity.client_id {
            let kp_bytes = base64_decode(&c.identity.ed25519_keypair);
            let channel_key = base64_decode(&ch.key);
            crate::local::channel::sender::broadcast_update(
                &ch,
                &c.identity.client_id,
                &c.identity.device_name,
                &kp_bytes,
                &c.db,
                &c.peer_key_cache,
                &channel_key,
            )
            .await;
        }
        emit_channels_updated(&c);
        Ok(ChatChannel::from(ch))
    }

    async fn delete_chat_channel(&self, ctx: &Context<'_>, id: String) -> bool {
        let c = ctx.data_unchecked::<Arc<AppCtx>>().clone();
        if let Some(mut ch) = c.db.get_channel_by_id(&id) {
            if ch.owner_id == c.identity.client_id {
                let kp_bytes = base64_decode(&c.identity.ed25519_keypair);
                let channel_key = base64_decode(&ch.key);
                crate::local::channel::sender::broadcast_kick(
                    &ch,
                    &c.identity.client_id,
                    &kp_bytes,
                    &c.db,
                    &c.peer_key_cache,
                    &channel_key,
                )
                .await;
            }
            ch.status = ChannelStatus::Left;
            ch.updated_at = now_iso();
            c.db.update_channel(&ch);
        }
        c.db.delete_chats_by_channel(&id);
        c.db.delete_channel(&id);
        refresh_peer_key_cache(&c.db, &c.peer_key_cache);
        emit_channels_updated(&c);
        true
    }

    async fn leave_chat_channel(&self, ctx: &Context<'_>, id: String) -> bool {
        let c = ctx.data_unchecked::<Arc<AppCtx>>().clone();
        if let Some(mut ch) = c.db.get_channel_by_id(&id) {
            if ch.owner_id != c.identity.client_id {
                if let Some(owner_peer) = c.db.get_peer_by_id(&ch.owner_id) {
                    let kp_bytes = base64_decode(&c.identity.ed25519_keypair);
                    let channel_key = base64_decode(&ch.key);
                    let _ = crate::local::channel::sender::send_leave(
                        &ch.id,
                        &owner_peer,
                        &c.identity.client_id,
                        &kp_bytes,
                        &channel_key,
                        &c.peer_key_cache,
                    )
                    .await;
                }
                let new_members: Vec<ChannelMember> = decode_members(&ch.members)
                    .into_iter()
                    .filter(|m| m.peer_id != c.identity.client_id)
                    .collect();
                ch.members = encode_members(&new_members);
                ch.status = ChannelStatus::Left;
                ch.updated_at = now_iso();
                c.db.update_channel(&ch);
                refresh_peer_key_cache(&c.db, &c.peer_key_cache);
            }
            emit_channels_updated(&c);
        }
        true
    }

    async fn add_chat_channel_member(
        &self,
        ctx: &Context<'_>,
        id: String,
        peer_id: String,
    ) -> GqlResult<ChatChannel> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>().clone();
        let mut ch =
            c.db.get_channel_by_id(&id)
                .ok_or_else(|| gql_err("Channel not found"))?;
        if ch.owner_id != c.identity.client_id {
            return Err(gql_err("Only owner can add members"));
        }
        let mut new_members = decode_members(&ch.members);
        if has_member(&new_members, &peer_id) {
            return Err(gql_err("Already a member"));
        }
        new_members.push(ChannelMember::pending(&peer_id));
        ch.members = encode_members(&new_members);
        ch.version += 1;
        ch.updated_at = now_iso();
        c.db.update_channel(&ch);

        if let Some(peer) = c.db.get_peer_by_id(&peer_id) {
            let kp_bytes = base64_decode(&c.identity.ed25519_keypair);
            let channel_key = base64_decode(&ch.key);
            let _ = crate::local::channel::sender::send_invite(
                &ch,
                &peer,
                &c.identity.client_id,
                &c.identity.device_name,
                &kp_bytes,
                &c.db,
                &c.peer_key_cache,
                &channel_key,
            )
            .await;
        }
        emit_channels_updated(&c);
        Ok(ChatChannel::from(ch))
    }

    async fn remove_chat_channel_member(
        &self,
        ctx: &Context<'_>,
        id: String,
        peer_id: String,
    ) -> GqlResult<ChatChannel> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>().clone();
        let mut ch =
            c.db.get_channel_by_id(&id)
                .ok_or_else(|| gql_err("Channel not found"))?;
        if ch.owner_id != c.identity.client_id {
            return Err(gql_err("Only owner can remove members"));
        }
        let members = decode_members(&ch.members);
        if !has_member(&members, &peer_id) {
            return Err(gql_err("Not a member"));
        }
        let new_members: Vec<ChannelMember> =
            members.into_iter().filter(|m| m.peer_id != peer_id).collect();
        ch.members = encode_members(&new_members);
        ch.version += 1;
        ch.updated_at = now_iso();
        c.db.update_channel(&ch);

        let kp_bytes = base64_decode(&c.identity.ed25519_keypair);
        let channel_key = base64_decode(&ch.key);
        if let Some(peer) = c.db.get_peer_by_id(&peer_id) {
            let _ = crate::local::channel::sender::send_kick(
                &ch.id,
                ch.version,
                &peer,
                &c.identity.client_id,
                &kp_bytes,
                &channel_key,
                &c.peer_key_cache,
            )
            .await;
        }
        crate::local::channel::sender::broadcast_update(
            &ch,
            &c.identity.client_id,
            &c.identity.device_name,
            &kp_bytes,
            &c.db,
            &c.peer_key_cache,
            &channel_key,
        )
        .await;
        emit_channels_updated(&c);
        Ok(ChatChannel::from(ch))
    }

    async fn accept_chat_channel_invite(&self, ctx: &Context<'_>, id: String) -> GqlResult<bool> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>().clone();
        let ch =
            c.db.get_channel_by_id(&id)
                .ok_or_else(|| gql_err("Channel not found"))?;
        let owner_peer =
            c.db.get_peer_by_id(&ch.owner_id)
                .ok_or_else(|| gql_err("Owner peer not found"))?;
        let kp_bytes = base64_decode(&c.identity.ed25519_keypair);
        let channel_key = base64_decode(&ch.key);
        load_key_cache(&c.db, &c.peer_key_cache, &c.channel_key_cache);
        let _ = crate::local::channel::sender::send_invite_accept(
            &ch.id,
            &owner_peer,
            &c.identity.client_id,
            &kp_bytes,
            &c.identity.device_name,
            "desktop",
            &channel_key,
            &c.peer_key_cache,
        )
        .await;
        Ok(true)
    }

    async fn decline_chat_channel_invite(&self, ctx: &Context<'_>, id: String) -> bool {
        let c = ctx.data_unchecked::<Arc<AppCtx>>().clone();
        let Some(ch) = c.db.get_channel_by_id(&id) else {
            return true;
        };
        if let Some(owner_peer) = c.db.get_peer_by_id(&ch.owner_id) {
            let kp_bytes = base64_decode(&c.identity.ed25519_keypair);
            let channel_key = base64_decode(&ch.key);
            let _ = crate::local::channel::sender::send_invite_decline(
                &ch.id,
                &owner_peer,
                &c.identity.client_id,
                &kp_bytes,
                &channel_key,
                &c.peer_key_cache,
            )
            .await;
        }
        c.db.delete_chats_by_channel(&ch.id);
        c.db.delete_channel(&ch.id);
        refresh_peer_key_cache(&c.db, &c.peer_key_cache);
        emit_channels_updated(&c);
        true
    }

    /// Web-only convenience mutation that branches to
    /// `acceptChatChannelInvite` or `declineChatChannelInvite` based
    /// on the `accept` flag. The plain-app Android schema doesn't
    /// expose this — the web client added it so the
    /// `ChannelInviteModal` can use a single GraphQL document for
    /// both buttons. Returns the `accept` flag verbatim so the
    /// modal's `onDone` handler can read the chosen action back from
    /// the mutation result.
    async fn respond_channel_invite(&self, ctx: &Context<'_>, id: String, accept: bool) -> bool {
        if accept {
            // Reuse the accept path — `accept_chat_channel_invite`
            // already returns `Ok(true)` on success.
            let _ = Self::accept_chat_channel_invite(self, ctx, id).await;
        } else {
            let _ = Self::decline_chat_channel_invite(self, ctx, id).await;
        }
        accept
    }

    async fn channel_system_message(
        &self,
        _ctx: &Context<'_>,
        #[graphql(name = "type")] _msg_type: String,
        _payload: String,
    ) -> bool {
        // Debug-only stub. The Kotlin client only invokes
        // `channelSystemMessage` through the peer endpoint
        // (`/peer_graphql`), which has its own resolver delegating to
        // `chat_handler::receive_peer_channel_system_message`.
        // Exposing it on the main schema would double-process the
        // same wire payload, so we keep this here purely so the
        // GraphQL schema still validates; the implementation is a
        // no-op.
        log::warn!(
            "[chat_channel] main-schema channelSystemMessage called — use /peer_graphql instead"
        );
        false
    }
}
