use crate::content_types::{ActionResult, Instant};
use crate::db::{DChat, Db};
use async_graphql::{Context, Enum, Object, SimpleObject};
use serde_json::{Value, json};
use std::sync::Arc;

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum ChatStatus {
    Sent,
    Partial,
    Failed,
    Pending,
}

impl ChatStatus {
    fn parse(value: &str) -> Self {
        match value {
            "PARTIAL" => ChatStatus::Partial,
            "FAILED" => ChatStatus::Failed,
            "PENDING" => ChatStatus::Pending,
            _ => ChatStatus::Sent,
        }
    }
}

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum ChatChannelStatus {
    Joined,
    Left,
    Kicked,
}

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug)]
pub enum ChannelMemberStatus {
    Joined,
    Pending,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct ChatChannelMember {
    #[graphql(name = "peerId")]
    pub peer_id: async_graphql::ID,
    pub status: ChannelMemberStatus,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct ChatChannel {
    pub id: async_graphql::ID,
    #[graphql(name = "ownerId")]
    pub owner_id: async_graphql::ID,
    pub name: String,
    pub members: Vec<ChatChannelMember>,
    /// Monotonically increasing mutation counter; receivers ignore channel
    /// updates whose version is not greater than their local copy.
    pub version: crate::content_types::Long,
    pub status: ChatChannelStatus,
    #[graphql(name = "createdAt")]
    pub created_at: Instant,
    #[graphql(name = "updatedAt")]
    pub updated_at: Instant,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct ChatItem {
    /// `me` when this device created the item, otherwise the client id of
    /// the originating peer.
    #[graphql(name = "fromId")]
    pub from_id: async_graphql::ID,
    /// Peer id for direct messages; empty for channel messages —
    /// `channelId` is authoritative there.
    #[graphql(name = "toId")]
    pub to_id: async_graphql::ID,
    /// Null for direct messages: the store keeps "" and the contract says
    /// null, because "" is not a channel a client can query.
    #[graphql(name = "channelId")]
    pub channel_id: Option<async_graphql::ID>,
    pub id: async_graphql::ID,
    /// Message envelope JSON: `{type: TEXT|IMAGES|FILES|SHARE, value: {…}}` —
    /// parse `value` according to `type`.
    pub content: String,
    #[graphql(name = "createdAt")]
    pub created_at: Instant,
    #[graphql(name = "updatedAt")]
    pub updated_at: Instant,
    pub status: ChatStatus,
    /// Per-recipient delivery details as JSON (peer delivery results);
    /// empty when the message has no delivery failures. Drives
    /// SENT/PARTIAL/FAILED alongside `status`.
    #[graphql(name = "statusData")]
    pub status_data: String,
}

#[derive(Default)]
pub struct ChatQuery;

#[Object]
impl ChatQuery {
    /// Latest-first page of one conversation, returned oldest-to-newest so
    /// clients can render directly. `target` is the chat target id: a bare
    /// peer id (an optional `peer:` prefix is accepted), or a
    /// `channel:<id>` prefixed channel id.
    async fn chat_items(
        &self,
        ctx: &Context<'_>,
        target: String,
        offset: i32,
        limit: i32,
        query: String,
    ) -> async_graphql::Result<Vec<ChatItem>> {
        let text = text_of(&query);
        Ok(db(ctx)?
            .get_chats_page(&target, &text, offset, limit)
            .iter()
            .map(item)
            .collect())
    }

    /// The most recent item of every conversation (direct and channel),
    /// newest conversation first.
    async fn latest_chat_items(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<ChatItem>> {
        Ok(db(ctx)?.get_all_latest_chats().iter().map(item).collect())
    }

    async fn chat_channels(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<ChatChannel>> {
        let facts = crate::chat::events::public_channels(db(ctx)?)?;
        Ok(rows(&facts, channel))
    }
}

#[derive(Default)]
pub struct ChatMutation;

#[Object]
impl ChatMutation {
    /// Send a chat message. `target` is the chat target id — a bare peer id
    /// (an optional `peer:` prefix is accepted), or a `channel:<id>`
    /// prefixed channel id; same value space as the `chatItems` query.
    /// `content` is the message envelope JSON, same shape as
    /// `ChatItem.content`.
    async fn send_chat_item(
        &self,
        ctx: &Context<'_>,
        target: String,
        content: String,
    ) -> async_graphql::Result<Vec<ChatItem>> {
        let row =
            super::chat_actions::send(ctx.data::<super::server::ServerState>()?, &target, &content)
                .await?;
        Ok(vec![item(&row)])
    }

    /// Always true: an id that is already gone is the state the caller asked
    /// for, and the contract has no way to say "nothing was there".
    async fn delete_chat_item(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
    ) -> async_graphql::Result<bool> {
        super::chat_actions::delete(
            ctx.data::<super::server::ServerState>()?,
            vec![id.to_string()],
        )?;
        Ok(true)
    }

    async fn delete_chat_items(
        &self,
        ctx: &Context<'_>,
        query: String,
    ) -> async_graphql::Result<ActionResult> {
        let count =
            super::chat_actions::delete_query(ctx.data::<super::server::ServerState>()?, &query)?;
        Ok(ActionResult {
            affected_count: i32::try_from(count)?,
        })
    }

    async fn retry_chat_item(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
    ) -> async_graphql::Result<ChatItem> {
        let row =
            super::chat_actions::retry(ctx.data::<super::server::ServerState>()?, id.to_string())
                .await?;
        Ok(item(&row))
    }

    async fn create_chat_channel(
        &self,
        ctx: &Context<'_>,
        name: String,
    ) -> async_graphql::Result<ChatChannel> {
        action(ctx, "create", &[("name", json!(name))]).await
    }

    async fn update_chat_channel(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
        name: String,
    ) -> async_graphql::Result<ChatChannel> {
        action(
            ctx,
            "rename",
            &[("id", json!(id.as_str())), ("name", json!(name))],
        )
        .await
    }

    async fn delete_chat_channel(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
    ) -> async_graphql::Result<bool> {
        flag(ctx, "delete", &[("id", json!(id.as_str()))]).await
    }

    async fn leave_chat_channel(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
    ) -> async_graphql::Result<bool> {
        flag(ctx, "leave", &[("id", json!(id.as_str()))]).await
    }

    async fn add_chat_channel_member(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
        peer_id: async_graphql::ID,
    ) -> async_graphql::Result<ChatChannel> {
        action(
            ctx,
            "invite",
            &[
                ("id", json!(id.as_str())),
                ("peer", json!(peer_id.as_str())),
            ],
        )
        .await
    }

    async fn remove_chat_channel_member(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
        peer_id: async_graphql::ID,
    ) -> async_graphql::Result<ChatChannel> {
        action(
            ctx,
            "kick",
            &[
                ("id", json!(id.as_str())),
                ("peer", json!(peer_id.as_str())),
            ],
        )
        .await
    }

    async fn accept_chat_channel_invite(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
    ) -> async_graphql::Result<bool> {
        flag(ctx, "accept", &[("id", json!(id.as_str()))]).await
    }

    async fn decline_chat_channel_invite(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
    ) -> async_graphql::Result<bool> {
        flag(ctx, "decline", &[("id", json!(id.as_str()))]).await
    }
}

async fn action(
    ctx: &Context<'_>,
    action: &str,
    fields: &[(&str, Value)],
) -> async_graphql::Result<ChatChannel> {
    let mut params = serde_json::Map::new();
    params.insert("action".to_string(), json!(action));
    for (key, value) in fields {
        params.insert((*key).to_string(), value.clone());
    }
    let request = serde_json::from_value(Value::Object(params))?;
    let result =
        super::channel_runtime::execute(ctx.data::<super::server::ServerState>()?, request).await?;
    let row: crate::db::DChannel = serde_json::from_value(result["channel"].clone())?;
    Ok(channel(&crate::chat::events::channel_to_json(&row)))
}

/// The mutations the contract types as `Boolean!` acknowledge the request
/// rather than report a state — a channel that is already gone is the state
/// the caller wanted.
async fn flag(
    ctx: &Context<'_>,
    action: &str,
    fields: &[(&str, Value)],
) -> async_graphql::Result<bool> {
    let mut params = serde_json::Map::new();
    params.insert("action".to_string(), json!(action));
    for (key, value) in fields {
        params.insert((*key).to_string(), value.clone());
    }
    let request = serde_json::from_value(Value::Object(params))?;
    super::channel_runtime::execute(ctx.data::<super::server::ServerState>()?, request).await?;
    Ok(true)
}

fn item(row: &DChat) -> ChatItem {
    from_json(&crate::chat::service::chat_to_json(row))
}

fn from_json(value: &Value) -> ChatItem {
    ChatItem {
        from_id: super::public_facts::id(value, "fromId"),
        to_id: super::public_facts::id(value, "toId"),
        channel_id: Some(super::public_facts::id(value, "channelId")).filter(|id| !id.is_empty()),
        id: super::public_facts::id(value, "id"),
        content: super::public_facts::text(value, "content"),
        created_at: super::public_facts::stored_instant(&super::public_facts::text(
            value,
            "createdAt",
        )),
        updated_at: super::public_facts::stored_instant(&super::public_facts::text(
            value,
            "updatedAt",
        )),
        status: ChatStatus::parse(&super::public_facts::text(value, "status")),
        status_data: super::public_facts::text(value, "statusData"),
    }
}

fn channel(value: &Value) -> ChatChannel {
    ChatChannel {
        id: super::public_facts::id(value, "id"),
        owner_id: super::public_facts::id(value, "ownerId"),
        name: super::public_facts::text(value, "name"),
        members: super::public_facts::list(value, "members", |member| ChatChannelMember {
            peer_id: super::public_facts::id(member, "peerId"),
            status: match super::public_facts::text(member, "status").as_str() {
                "PENDING" => ChannelMemberStatus::Pending,
                _ => ChannelMemberStatus::Joined,
            },
        }),
        version: crate::content_types::Long(super::public_facts::integer(value, "version")),
        status: match super::public_facts::text(value, "status").as_str() {
            "LEFT" => ChatChannelStatus::Left,
            "KICKED" => ChatChannelStatus::Kicked,
            _ => ChatChannelStatus::Joined,
        },
        created_at: super::public_facts::stored_instant(&super::public_facts::text(
            value,
            "createdAt",
        )),
        updated_at: super::public_facts::stored_instant(&super::public_facts::text(
            value,
            "updatedAt",
        )),
    }
}

fn text_of(query: &str) -> String {
    crate::utils::search_dsl::parse(query)
        .into_iter()
        .find(|field| field.name == "text")
        .map(|field| field.value)
        .unwrap_or_default()
}

fn rows<T>(value: &Value, item: impl Fn(&Value) -> T) -> Vec<T> {
    super::public_facts::rows(value, item)
}

fn db<'a>(ctx: &'a Context<'_>) -> async_graphql::Result<&'a Arc<Db>> {
    ctx.data::<Arc<Db>>()
}

#[cfg(test)]
#[path = "../../tests/unit/content_api/public_chat.rs"]
mod tests;
