use super::public::{ChatItem, item};
use crate::assembly::server::ServerState;
use crate::graphql::types::ActionResult;
use crate::services::chat::chat_actions;
use async_graphql::{Context, ID, Object, SimpleObject};
use plain_rs::chat::messages::{message_commands, message_lifecycle};

#[derive(SimpleObject)]
pub struct ChatConversation {
    pub id: ID,
    pub chat: ChatItem,
}
#[derive(Default)]
pub struct ChatHostQuery;
#[Object]
impl ChatHostQuery {
    async fn chat_item(
        &self,
        ctx: &Context<'_>,
        id: ID,
    ) -> async_graphql::Result<Option<ChatItem>> {
        let state = ctx.data::<ServerState>()?;
        Ok(
            plain_rs::db::chat_store::messages::get(&state.db, id.as_str())?
                .as_ref()
                .map(item),
        )
    }
    async fn chat_conversations(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Vec<ChatConversation>> {
        Ok(
            plain_rs::chat::cacher::ChatCacher::snapshot(&ctx.data::<ServerState>()?.db)?
                .into_iter()
                .map(|(id, chat)| ChatConversation {
                    id: ID::from(id),
                    chat: item(&chat),
                })
                .collect(),
        )
    }
    async fn chat_records(
        &self,
        ctx: &Context<'_>,
        query: String,
        offset: i32,
        limit: i32,
    ) -> async_graphql::Result<Vec<ChatItem>> {
        let state = ctx.data::<ServerState>()?;
        let value = plain_rs::db::chat_store::messages::list(
            &state.db,
            &filter(&query, offset, limit, false),
        )?;
        let rows: Vec<plain_rs::db::DChat> = serde_json::from_value(value)?;
        Ok(rows.iter().map(item).collect())
    }
    async fn chat_count(&self, ctx: &Context<'_>, query: String) -> async_graphql::Result<i32> {
        let state = ctx.data::<ServerState>()?;
        let value =
            plain_rs::db::chat_store::messages::list(&state.db, &filter(&query, 0, 0, true))?;
        Ok(i32::try_from(value.as_i64().unwrap_or_default())?)
    }
    async fn chat_ids(&self, ctx: &Context<'_>, query: String) -> async_graphql::Result<Vec<ID>> {
        Ok(
            plain_rs::db::chat_store::messages::ids(&ctx.data::<ServerState>()?.db, &query)?
                .into_iter()
                .map(ID::from)
                .collect(),
        )
    }
}
#[derive(SimpleObject)]
pub struct ChatTextEdit {
    pub changed: bool,
    pub chat: ChatItem,
}
#[derive(Default)]
pub struct ChatHostMutation;
#[Object]
impl ChatHostMutation {
    async fn send_chat_text(
        &self,
        ctx: &Context<'_>,
        target: String,
        text: String,
    ) -> async_graphql::Result<ChatItem> {
        let rows = chat_actions::send_text(ctx.data::<ServerState>()?, vec![target], &text).await?;
        Ok(item(rows.first().ok_or_else(|| {
            async_graphql::Error::new("Message unavailable")
        })?))
    }
    async fn forward_chat_item(
        &self,
        ctx: &Context<'_>,
        id: ID,
        target: String,
    ) -> async_graphql::Result<ChatItem> {
        let state = ctx.data::<ServerState>()?;
        let row = plain_rs::db::chat_store::messages::get(&state.db, id.as_str())?
            .ok_or_else(|| async_graphql::Error::new("Message unavailable"))?;
        Ok(item(
            &chat_actions::send(state, &target, &row.content).await?,
        ))
    }
    async fn deliver_chat_item(
        &self,
        ctx: &Context<'_>,
        id: ID,
        recipients: Option<Vec<ID>>,
    ) -> async_graphql::Result<ChatItem> {
        let recipients = recipients.map(|ids| ids.into_iter().map(|id| id.to_string()).collect());
        let receipt = crate::services::chat::chat_delivery::deliver(
            ctx.data::<ServerState>()?,
            id.as_str(),
            recipients,
        )
        .await?;
        Ok(item(&receipt.chat.ok_or_else(|| {
            async_graphql::Error::new("Message unavailable")
        })?))
    }
    async fn create_chat_item(
        &self,
        ctx: &Context<'_>,
        target: String,
        content: String,
    ) -> async_graphql::Result<ChatItem> {
        let state = ctx.data::<ServerState>()?;
        let (to, channel) = message_commands::target(&target)?;
        let chat = message_lifecycle::create(&state.db, &to, &channel, &content)?;
        chat_actions::committed(state, &chat, plain_rs::chat::events::WS_MESSAGE_CREATED);
        Ok(item(&chat))
    }
    async fn replace_chat_files(
        &self,
        ctx: &Context<'_>,
        id: ID,
        content: String,
    ) -> async_graphql::Result<Option<ChatItem>> {
        let state = ctx.data::<ServerState>()?;
        let content: serde_json::Value = serde_json::from_str(&content)?;
        let items = content["value"]["items"]
            .as_array()
            .ok_or_else(|| async_graphql::Error::new("File items required"))?
            .clone();
        let Some(chat) =
            message_commands::replace_files(&state.db, &state.directory, id.as_str(), items)?
        else {
            return Ok(None);
        };
        chat_actions::committed(state, &chat, plain_rs::chat::events::WS_MESSAGE_UPDATED);
        Ok(Some(item(&chat_actions::deliver(state, chat).await?)))
    }
    async fn clear_chat_items(
        &self,
        ctx: &Context<'_>,
        target: String,
    ) -> async_graphql::Result<ActionResult> {
        use plain_rs::chat::attachments::app_file_store::chat_deletion::{self, Selection};
        let state = ctx.data::<ServerState>()?;
        let (peer, channel) = message_commands::target(&target)?;
        let count = chat_deletion::delete(
            &state.db,
            &state.directory,
            if channel.is_empty() {
                Selection::Peer(&peer)
            } else {
                Selection::Channel(&channel)
            },
        )?;
        let _ = state.events.send(plain_rs::events::WsEvent::broadcast(
            plain_rs::chat::events::WS_MESSAGE_DELETED,
            serde_json::json!(target).to_string(),
        ));
        Ok(ActionResult {
            affected_count: count as i32,
        })
    }
    async fn edit_chat_text(
        &self,
        ctx: &Context<'_>,
        id: ID,
        text: String,
    ) -> async_graphql::Result<ChatTextEdit> {
        let state = ctx.data::<ServerState>()?;
        let result =
            crate::services::link_preview::edit(&state.db, &state.directory, id.as_str(), &text)?;
        state.previews.request(id.as_str());
        if result.changed {
            chat_actions::committed(
                state,
                &result.chat,
                plain_rs::chat::events::WS_MESSAGE_UPDATED,
            );
        }
        Ok(ChatTextEdit {
            changed: result.changed,
            chat: item(&result.chat),
        })
    }
}

fn filter(
    query: &str,
    offset: i32,
    limit: i32,
    count_only: bool,
) -> plain_rs::db::chat_store::messages::Filter {
    let fields = plain_rs::utils::search_dsl::parse(query);
    let field = |name: &str| {
        fields
            .iter()
            .find(|field| field.name == name)
            .map(|field| field.value.clone())
    };
    plain_rs::db::chat_store::messages::Filter {
        peer: field("peer"),
        channel: field("channel"),
        text: field("text").unwrap_or_default(),
        offset: i64::from(offset),
        limit: Some(i64::from(limit)),
        descending: field("sort").as_deref() == Some("desc"),
        latest: field("latest").as_deref() == Some("true"),
        count_only,
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/graphql/resolvers/chat/host.rs"]
mod tests;
