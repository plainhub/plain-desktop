//! Public `/graphql` SMS roots.
//!
//! The reads and the send path already run in Rust for the app's own SMS
//! screen ([`crate::mobile::sms::query`], [`crate::mobile::sms::send`], [`crate::mobile::sms::state`]);
//! these roots only add the contract shape and the web permission gate.
//!
//! plain-app degrades every read to an empty list or 0 when the client was
//! not granted READ_SMS — it never errors — so the gate follows that.

use crate::graphql::resolvers::tags::domain_types::Tag;
use crate::graphql::types::ActionResult;
use crate::graphql::types::facts::flag;
use crate::graphql::types::facts::instant;
use crate::graphql::types::facts::integer;
use crate::graphql::types::facts::list;
use crate::graphql::types::facts::strings;
use crate::graphql::types::facts::text;
use crate::host::bridge::Host;
use crate::http::auth::public_gate;
use crate::mobile::sms::query as sms_query;
use crate::mobile::sms::send as sms_send;
use crate::mobile::sms::state as sms_state;
use async_graphql::{Context, Enum, ID, Object, SimpleObject};
use plain_rs::db::Db;
use plain_rs::prefs::Prefs;
use serde_json::Value;
use std::sync::Arc;

const READ: &str = "READ_SMS";

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug, Default)]
pub enum SmsType {
    Inbox = 1,
    Sent = 2,
    Draft = 3,
    Outbox = 4,
    Failed = 5,
    Queued = 6,
    #[default]
    Unknown,
}

impl SmsType {
    fn from_android(value: i64) -> Self {
        match value {
            1 => Self::Inbox,
            2 => Self::Sent,
            3 => Self::Draft,
            4 => Self::Outbox,
            5 => Self::Failed,
            6 => Self::Queued,
            _ => Self::Unknown,
        }
    }
}

#[derive(SimpleObject, Clone, Debug)]
pub struct SmsAttachment {
    pub path: String,
    #[graphql(name = "contentType")]
    pub content_type: String,
    pub name: String,
}

#[derive(SimpleObject, Clone, Debug)]
#[graphql(complex)]
pub struct Sms {
    pub id: ID,
    pub body: String,
    pub address: String,
    #[graphql(name = "sentAt")]
    pub sent_at: crate::graphql::types::Instant,
    #[graphql(name = "serviceCenter")]
    pub service_center: String,
    pub read: bool,
    #[graphql(name = "threadId")]
    pub thread_id: ID,
    pub r#type: SmsType,
    #[graphql(name = "subscriptionId")]
    pub subscription_id: i32,
    #[graphql(name = "isMms")]
    pub is_mms: bool,
    pub attachments: Vec<SmsAttachment>,
}

#[async_graphql::ComplexObject]
impl Sms {
    async fn tags(&self, ctx: &async_graphql::Context<'_>) -> async_graphql::Result<Vec<Tag>> {
        crate::graphql::resolvers::tags::loader::load(
            ctx,
            plain_rs::enums::DataType::Sms,
            self.id.as_str(),
        )
        .await
    }
}

#[derive(SimpleObject, Clone, Debug)]
pub struct SmsConversation {
    pub id: ID,
    pub address: String,
    pub snippet: String,
    #[graphql(name = "lastMessageAt")]
    pub last_message_at: crate::graphql::types::Instant,
    #[graphql(name = "messageCount")]
    pub message_count: i32,
    pub read: bool,
    pub addresses: Vec<String>,
}

#[derive(SimpleObject, Clone, Debug, Default)]
pub struct SmsCounts {
    pub total: i32,
    pub inbox: i32,
    pub sent: i32,
    pub drafts: i32,
}

#[derive(Default)]
pub struct SmsQuery;

#[Object]
impl SmsQuery {
    async fn sms(
        &self,
        ctx: &Context<'_>,
        query: String,
        offset: i32,
        limit: i32,
    ) -> async_graphql::Result<Vec<Sms>> {
        if !public_gate::require_granted(ctx, &[READ]).await? {
            return Ok(Vec::new());
        }
        let receipt = sms_query::execute(
            ctx.data_unchecked::<Arc<Db>>(),
            ctx.data_unchecked::<Arc<Host>>(),
            sms_query::Request::Search {
                query,
                offset,
                limit,
                include_trashed: false,
            },
        )
        .await
        .map_err(|error| async_graphql::Error::new(error.to_string()))?;
        Ok(receipt["items"]
            .as_array()
            .map(|items| items.iter().map(|item| message(item)).collect())
            .unwrap_or_default())
    }

    async fn sms_count(&self, ctx: &Context<'_>, query: String) -> async_graphql::Result<i32> {
        if !public_gate::require_granted(ctx, &[READ]).await? {
            return Ok(0);
        }
        count(ctx, sms_query::Request::Count { query }).await
    }

    async fn sms_box_counts(&self, ctx: &Context<'_>) -> async_graphql::Result<SmsCounts> {
        if !public_gate::require_granted(ctx, &[READ]).await? {
            return Ok(SmsCounts::default());
        }
        let receipt = sms_query::execute(
            ctx.data_unchecked::<Arc<Db>>(),
            ctx.data_unchecked::<Arc<Host>>(),
            sms_query::Request::Counts,
        )
        .await
        .map_err(|error| async_graphql::Error::new(error.to_string()))?;
        Ok(SmsCounts {
            total: integer(&receipt, "total") as i32,
            inbox: integer(&receipt, "inbox") as i32,
            sent: integer(&receipt, "sent") as i32,
            drafts: integer(&receipt, "drafts") as i32,
        })
    }

    async fn sms_conversations(
        &self,
        ctx: &Context<'_>,
        query: String,
        offset: i32,
        limit: i32,
    ) -> async_graphql::Result<Vec<SmsConversation>> {
        if !public_gate::require_granted(ctx, &[READ]).await? {
            return Ok(Vec::new());
        }
        conversations(ctx, query, offset, limit, false).await
    }

    async fn sms_conversation_count(
        &self,
        ctx: &Context<'_>,
        query: String,
    ) -> async_graphql::Result<i32> {
        if !public_gate::require_granted(ctx, &[READ]).await? {
            return Ok(0);
        }
        count(ctx, sms_query::Request::ConversationCount { query }).await
    }

    async fn archived_sms_conversations(
        &self,
        ctx: &Context<'_>,
        query: String,
        offset: i32,
        limit: i32,
    ) -> async_graphql::Result<Vec<SmsConversation>> {
        if !public_gate::require_granted(ctx, &[READ]).await? {
            return Ok(Vec::new());
        }
        conversations(ctx, query, offset, limit, true).await
    }
}

#[derive(Default)]
pub struct SmsMutation;

#[Object]
impl SmsMutation {
    async fn archive_sms_conversation(
        &self,
        ctx: &Context<'_>,
        id: ID,
    ) -> async_graphql::Result<bool> {
        // The archive date is the conversation's last message, so a new
        // message after archiving brings the conversation back.
        let receipt = sms_query::execute(
            ctx.data_unchecked::<Arc<Db>>(),
            ctx.data_unchecked::<Arc<Host>>(),
            sms_query::Request::ConversationDate { id: id.0.clone() },
        )
        .await
        .map_err(|error| async_graphql::Error::new(error.to_string()))?;
        let date = receipt["date"]
            .as_str()
            .map(Value::from)
            .unwrap_or(Value::Null);
        let date = if date.is_null() {
            chrono::Utc::now().to_rfc3339()
        } else {
            date.as_str().unwrap_or_default().to_owned()
        };
        sms_state::execute(
            ctx.data_unchecked::<Arc<Db>>(),
            sms_state::Request::Archive { id: id.0, date },
        )
        .map_err(|error| async_graphql::Error::new(error.to_string()))?;
        Ok(true)
    }

    async fn unarchive_sms_conversation(
        &self,
        ctx: &Context<'_>,
        id: ID,
    ) -> async_graphql::Result<bool> {
        sms_state::execute(
            ctx.data_unchecked::<Arc<Db>>(),
            sms_state::Request::Unarchive { id: id.0 },
        )
        .map_err(|error| async_graphql::Error::new(error.to_string()))?;
        Ok(true)
    }

    async fn trash_sms(
        &self,
        ctx: &Context<'_>,
        query: String,
    ) -> async_graphql::Result<ActionResult> {
        bulk(ctx, "systemTrashSms", query).await
    }

    async fn restore_sms(
        &self,
        ctx: &Context<'_>,
        query: String,
    ) -> async_graphql::Result<ActionResult> {
        bulk(ctx, "systemRestoreSms", query).await
    }

    async fn delete_sms(
        &self,
        ctx: &Context<'_>,
        query: String,
    ) -> async_graphql::Result<ActionResult> {
        bulk(ctx, "systemDeleteSms", query).await
    }

    /// `subscriptionId` picks the SIM (-1 = system default, see `sims`);
    /// `requestId` is an optional idempotency key that dedupes retried sends.
    async fn send_sms(
        &self,
        ctx: &Context<'_>,
        number: String,
        body: String,
        subscription_id: i32,
        request_id: Option<String>,
    ) -> async_graphql::Result<bool> {
        sms_send::execute(
            ctx.data_unchecked::<Arc<Prefs>>(),
            ctx.data_unchecked::<Arc<Host>>(),
            sms_send::Request {
                number,
                body,
                subscription_id: (subscription_id >= 0).then_some(subscription_id),
                client_id: None,
                client_request_id: request_id,
            },
        )
        .await
        .map_err(|error| async_graphql::Error::new(error.to_string()))?;
        Ok(true)
    }

    /// Compose an MMS in the device's default SMS app and track the send.
    /// Returns a pendingId consumed by the MMS polling flow.
    async fn send_mms(
        &self,
        ctx: &Context<'_>,
        number: String,
        body: String,
        attachment_paths: Vec<String>,
        thread_id: ID,
    ) -> async_graphql::Result<String> {
        ctx.data_unchecked::<Arc<crate::mobile::sms::mms::Runtime>>()
            .send(number, body, attachment_paths, thread_id.0)
            .await
            .map_err(|error| async_graphql::Error::new(error.to_string()))
    }
}

async fn count(ctx: &Context<'_>, request: sms_query::Request) -> async_graphql::Result<i32> {
    let receipt = sms_query::execute(
        ctx.data_unchecked::<Arc<Db>>(),
        ctx.data_unchecked::<Arc<Host>>(),
        request,
    )
    .await
    .map_err(|error| async_graphql::Error::new(error.to_string()))?;
    Ok(integer(&receipt, "count") as i32)
}

async fn conversations(
    ctx: &Context<'_>,
    query: String,
    offset: i32,
    limit: i32,
    archived: bool,
) -> async_graphql::Result<Vec<SmsConversation>> {
    let request = if archived {
        sms_query::Request::ArchivedConversations {
            query,
            offset,
            limit,
        }
    } else {
        sms_query::Request::Conversations {
            query,
            offset,
            limit,
        }
    };
    let receipt = sms_query::execute(
        ctx.data_unchecked::<Arc<Db>>(),
        ctx.data_unchecked::<Arc<Host>>(),
        request,
    )
    .await
    .map_err(|error| async_graphql::Error::new(error.to_string()))?;
    Ok(receipt["items"]
        .as_array()
        .map(|items| items.iter().map(conversation).collect())
        .unwrap_or_default())
}

/// The three bulk SMS mutations are one platform call each: resolving the
/// query into ids stays on the platform, where the provider write is.
async fn bulk(
    ctx: &Context<'_>,
    method: &str,
    query: String,
) -> async_graphql::Result<ActionResult> {
    if query.trim().is_empty() {
        return Err(async_graphql::Error::new(
            "query is required for bulk mutations — pass 'all:true' to explicitly target everything",
        ));
    }
    let affected = ctx
        .data_unchecked::<Arc<Host>>()
        .call(method, serde_json::json!({ "query": query }))
        .await
        .map_err(|error| async_graphql::Error::new(error))?;
    Ok(ActionResult {
        affected_count: affected.as_i64().unwrap_or_default() as i32,
    })
}

fn message(item: &Value) -> Sms {
    let id = text(item, "id");
    Sms {
        id: ID::from(id),
        body: text(item, "body"),
        address: text(item, "address"),
        sent_at: instant(item, "date"),
        service_center: text(item, "serviceCenter"),
        read: flag(item, "read"),
        thread_id: ID::from(text(item, "threadId")),
        r#type: SmsType::from_android(integer(item, "type")),
        subscription_id: integer(item, "subscriptionId") as i32,
        is_mms: flag(item, "isMms"),
        attachments: list(item, "attachments", |attachment| SmsAttachment {
            path: text(attachment, "path"),
            content_type: text(attachment, "contentType"),
            name: text(attachment, "name"),
        }),
    }
}

fn conversation(item: &Value) -> SmsConversation {
    SmsConversation {
        id: ID::from(text(item, "id")),
        address: text(item, "address"),
        snippet: text(item, "snippet"),
        last_message_at: instant(item, "date"),
        message_count: integer(item, "messageCount") as i32,
        read: flag(item, "read"),
        addresses: strings(item, "addresses"),
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/mobile/sms/resolvers.rs"]
mod tests;
