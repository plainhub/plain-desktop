//! The peer-facing GraphQL schema for `POST /peer_graphql`.
//!
//! A minimal, type-safe surface — a stub Query plus the mutations the
//! peer protocol actually uses (`createChatItem`,
//! `channelSystemMessage`, `startAware`). Per-request state is carried
//! in [`PeerCtx`]; the mutation bodies live in the shared
//! `crate::chat` service — resolvers here are thin and only forward
//! the authenticated arguments.

use async_graphql::{Context, EmptySubscription, Object, Schema};
use std::sync::Arc;

use super::types::{ChatItem, chat_item_from_dchat};
use crate::api::chat::ChatState;

/// GraphQL input mirror of the wire `ChannelSystemMessageType` — the
/// plain-rs domain enum deliberately carries no async-graphql derives.
#[derive(async_graphql::Enum, Copy, Clone, Eq, PartialEq, Debug)]
#[allow(non_camel_case_types, clippy::upper_case_acronyms)] // plain-app wire names
pub enum PeerChannelSystemMessageType {
    INVITE,
    INVITE_ACCEPT,
    INVITE_DECLINE,
    UPDATE,
    KICK,
    LEAVE,
}

impl From<PeerChannelSystemMessageType> for crate::chat::enums::ChannelSystemMessageType {
    fn from(t: PeerChannelSystemMessageType) -> Self {
        use crate::chat::enums::ChannelSystemMessageType as T;
        match t {
            PeerChannelSystemMessageType::INVITE => T::Invite,
            PeerChannelSystemMessageType::INVITE_ACCEPT => T::InviteAccept,
            PeerChannelSystemMessageType::INVITE_DECLINE => T::InviteDecline,
            PeerChannelSystemMessageType::UPDATE => T::Update,
            PeerChannelSystemMessageType::KICK => T::Kick,
            PeerChannelSystemMessageType::LEAVE => T::Leave,
        }
    }
}

/// Per-request context injected into the peer schema.
pub struct PeerCtx {
    pub state: Arc<ChatState>,
    /// The authenticated sender's peer row.
    pub peer: crate::chat::db::DPeer,
    /// `c-cid` header — the channel a channel-bound request targets.
    pub channel_id: String,
}

#[derive(Default)]
pub struct PeerQuery;

#[Object]
impl PeerQuery {
    /// Schema-mandated placeholder; the peer protocol is mutation-only.
    async fn _peer_schema_version(&self, _ctx: &Context<'_>) -> i32 {
        1
    }
}

#[derive(Default)]
pub struct PeerMutation;

#[Object]
impl PeerMutation {
    /// Receive a chat item from an authenticated peer.
    /// Wire format: `mutation CreateChatItem($content: String!) { createChatItem(content: $content) { ... } }`
    async fn create_chat_item(&self, ctx: &Context<'_>, content: String) -> ChatItem {
        let c = ctx.data_unchecked::<PeerCtx>();
        chat_item_from_dchat(&c.state.service.receive_peer_chat(
            &c.peer.id,
            &c.channel_id,
            &content,
        ))
    }

    /// Receive a channel system message from an authenticated peer.
    /// Wire format: `mutation ChannelSystemMessage($type: ChannelSystemMessageType!, $payload: String!) { channelSystemMessage(type: $type, payload: $payload) }`
    async fn channel_system_message(
        &self,
        ctx: &Context<'_>,
        #[graphql(name = "type", desc = "System message type discriminator")]
        r#type: PeerChannelSystemMessageType,
        payload: String,
    ) -> bool {
        let c = ctx.data_unchecked::<PeerCtx>();
        c.state
            .service
            .receive_peer_channel_system_message(&c.peer.id, r#type.into(), &payload)
    }

    /// Wi-Fi Aware prewarm (plain-app contract). The NAS has no Aware
    /// support — log and refuse, keeping the field so paired Android
    /// peers don't hit validation errors.
    async fn start_aware(&self, ctx: &Context<'_>) -> bool {
        let c = ctx.data_unchecked::<PeerCtx>();
        log::warn!(
            "[peer_graphql] startAware requested by {} — Wi-Fi Aware not supported on NAS",
            c.peer.id
        );
        false
    }
}

pub type PeerSchema = Schema<PeerQuery, PeerMutation, EmptySubscription>;

pub fn build_schema() -> PeerSchema {
    Schema::build(PeerQuery, PeerMutation, EmptySubscription).finish()
}
