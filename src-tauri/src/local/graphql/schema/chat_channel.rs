//! `ChatChannelMutation` — thin GraphQL surface for channel mutations.
//!
//! The business logic lives on the shared chat service
//! (`plain_rs::chat::channel::ops`, see [`crate::local::chat::ChatState`]);
//! these resolvers parse the wire arguments and map the error strings the
//! ops layer surfaces.

use async_graphql::{Context, Error as GqlError, Object, Result as GqlResult};
use std::sync::Arc;

use super::super::context::AppCtx;
use super::types::ChatChannel;

#[derive(Default)]
pub struct ChatChannelMutation;

fn gql_err(msg: String) -> GqlError {
    GqlError::new(msg)
}

#[Object]
impl ChatChannelMutation {
    async fn create_chat_channel(&self, ctx: &Context<'_>, name: String) -> ChatChannel {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        ChatChannel::from(c.chat.service.create_channel(&name))
    }

    async fn update_chat_channel(
        &self,
        ctx: &Context<'_>,
        id: String,
        name: String,
    ) -> GqlResult<ChatChannel> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        c.chat
            .service
            .update_channel_name(&id, &name)
            .await
            .map(ChatChannel::from)
            .map_err(gql_err)
    }

    async fn delete_chat_channel(&self, ctx: &Context<'_>, id: String) -> bool {
        ctx.data_unchecked::<Arc<AppCtx>>()
            .chat
            .service
            .delete_channel(&id)
            .await
    }

    async fn leave_chat_channel(&self, ctx: &Context<'_>, id: String) -> bool {
        ctx.data_unchecked::<Arc<AppCtx>>()
            .chat
            .service
            .leave_channel(&id)
            .await
    }

    async fn add_chat_channel_member(
        &self,
        ctx: &Context<'_>,
        id: String,
        peer_id: String,
    ) -> GqlResult<ChatChannel> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        c.chat
            .service
            .add_channel_member(&id, &peer_id)
            .await
            .map(ChatChannel::from)
            .map_err(gql_err)
    }

    async fn remove_chat_channel_member(
        &self,
        ctx: &Context<'_>,
        id: String,
        peer_id: String,
    ) -> GqlResult<ChatChannel> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        c.chat
            .service
            .remove_channel_member(&id, &peer_id)
            .await
            .map(ChatChannel::from)
            .map_err(gql_err)
    }

    async fn accept_chat_channel_invite(&self, ctx: &Context<'_>, id: String) -> GqlResult<bool> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        c.chat
            .service
            .accept_channel_invite(&id)
            .await
            .map_err(gql_err)
    }

    async fn decline_chat_channel_invite(&self, ctx: &Context<'_>, id: String) -> bool {
        ctx.data_unchecked::<Arc<AppCtx>>()
            .chat
            .service
            .decline_channel_invite(&id)
            .await
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
        ctx.data_unchecked::<Arc<AppCtx>>()
            .chat
            .service
            .respond_channel_invite(&id, accept)
            .await
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
        // the shared service. Exposing it on the main schema would
        // double-process the same wire payload, so we keep this here
        // purely so the GraphQL schema still validates; the
        // implementation is a no-op.
        log::warn!(
            "[chat_channel] main-schema channelSystemMessage called — use /peer_graphql instead"
        );
        false
    }
}
