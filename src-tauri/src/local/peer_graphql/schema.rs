//! async-graphql schema for the peer endpoint.
//!
//! Defines a minimal, type-safe surface (a stub Query + the two mutations
//! the peer protocol actually uses) and builds it once at server start.
//! Per-request state is carried in `PeerCtx` (see `context.rs`); the
//! actual mutation bodies live on the shared chat service (the same
//! chat service layer the local GraphQL mutations use) — the resolvers here
//! are thin and only forward the authenticated arguments.
//!
//! The Query root is required by the GraphQL spec; it intentionally
//! exposes no fields so peers can only invoke the two mutations.

use async_graphql::{Context, EmptySubscription, Object, Schema};

use super::context::PeerCtx;
use crate::local::enums::ChannelSystemMessageType;
use crate::local::graphql::schema::types::ChatItem;

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
        ChatItem::with_data(
            c.app
                .chat
                .service
                .receive_peer_chat(&c.peer.id, &c.channel_id, &content),
            &c.app.token,
        )
    }

    /// Receive a channel system message from an authenticated peer.
    /// Wire format: `mutation ChannelSystemMessage($type: ChannelSystemMessageType!, $payload: String!) { channelSystemMessage(type: $type, payload: $payload) }`
    async fn channel_system_message(
        &self,
        ctx: &Context<'_>,
        #[graphql(name = "type", desc = "System message type discriminator")]
        r#type: ChannelSystemMessageType,
        payload: String,
    ) -> bool {
        let c = ctx.data_unchecked::<PeerCtx>();
        c.app
            .chat
            .service
            .receive_peer_channel_system_message(&c.peer.id, r#type.into(), &payload)
    }

    /// Ask the peer to start its Wi-Fi Aware service (mirrors plain-app's
    /// `startAware` mutation). The Tauri desktop build does not support
    /// Wi-Fi Aware, so this always logs a warning and returns `false`.
    /// Keeping the field in the schema avoids GraphQL validation errors
    /// when a paired Android peer prewarms the transport.
    async fn start_aware(&self, ctx: &Context<'_>) -> bool {
        let c = ctx.data_unchecked::<PeerCtx>();
        log::warn!(
            "[peer_graphql] startAware requested by {} — Wi-Fi Aware not supported on desktop",
            c.peer.id
        );
        false
    }
}

pub type PeerSchema = Schema<PeerQuery, PeerMutation, EmptySubscription>;

pub fn build_schema() -> PeerSchema {
    Schema::build(PeerQuery, PeerMutation, EmptySubscription).finish()
}
