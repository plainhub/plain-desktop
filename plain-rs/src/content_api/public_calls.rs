//! Public `/graphql` call-log roots.
//!
//! Reads reuse the same provider search the contact roots use, so a call and
//! a contact from the same query DSL behave alike.

use super::public_contact_types::Tag;
use super::public_facts::{host_json, instant, integer, text};
use super::public_gate;
use crate::content_api::host::Host;
use crate::content_types::ActionResult;
use crate::{db::Db, enums::DataType, prefs::Prefs};
use async_graphql::{Context, Enum, ID, Object, SimpleObject};
use serde_json::Value;
use std::sync::Arc;

const READ: &str = "READ_CALL_LOG";
const WRITE: &str = "WRITE_CALL_LOG";

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug, Default)]
pub enum CallType {
    Incoming = 1,
    Outgoing = 2,
    Missed = 3,
    Voicemail = 4,
    Rejected = 5,
    Blocked = 6,
    AnsweredExternally = 7,
    #[default]
    Unknown,
}

impl CallType {
    fn from_android(value: i64) -> Self {
        match value {
            1 => Self::Incoming,
            2 => Self::Outgoing,
            3 => Self::Missed,
            4 => Self::Voicemail,
            5 => Self::Rejected,
            6 => Self::Blocked,
            7 => Self::AnsweredExternally,
            _ => Self::Unknown,
        }
    }
}

#[derive(SimpleObject, Clone, Debug)]
pub struct PhoneGeo {
    pub country: String,
    #[graphql(name = "numberType")]
    pub number_type: String,
    pub carrier: String,
    pub description: String,
}

#[derive(SimpleObject, Clone, Debug)]
pub struct Call {
    pub id: ID,
    pub number: String,
    pub name: String,
    #[graphql(name = "photoId")]
    pub photo_id: String,
    #[graphql(name = "startedAt")]
    pub started_at: crate::content_types::Instant,
    #[graphql(name = "durationSec")]
    pub duration_sec: i32,
    pub r#type: CallType,
    #[graphql(name = "accountId")]
    pub account_id: ID,
    pub geo: Option<PhoneGeo>,
    pub tags: Vec<Tag>,
}

#[derive(Default)]
pub struct CallsQuery;

#[Object]
impl CallsQuery {
    async fn calls(
        &self,
        ctx: &Context<'_>,
        offset: i32,
        limit: i32,
        query: String,
    ) -> async_graphql::Result<Vec<Call>> {
        public_gate::require(ctx.data_unchecked::<Arc<Prefs>>(), &[READ])?;
        let facts = host_json(
            ctx,
            "systemCallFacts",
            serde_json::json!({"query": query, "offset": offset, "limit": limit}),
        )
        .await?;
        let db = ctx.data_unchecked::<Arc<Db>>();
        Ok(facts.iter().map(|fact| call(db, fact)).collect())
    }

    /// plain-app degrades this to 0 rather than erroring.
    async fn call_count(&self, ctx: &Context<'_>, query: String) -> async_graphql::Result<i32> {
        if !public_gate::require_granted(ctx, &[WRITE]).await? {
            return Ok(0);
        }
        let count = ctx
            .data_unchecked::<Arc<Host>>()
            .call("systemCallCount", serde_json::json!({ "query": query }))
            .await
            .map_err(|error| async_graphql::Error::new(error))?;
        Ok(count.as_i64().unwrap_or_default() as i32)
    }
}

#[derive(Default)]
pub struct CallsMutation;

#[Object]
impl CallsMutation {
    async fn delete_calls(
        &self,
        ctx: &Context<'_>,
        query: String,
    ) -> async_graphql::Result<ActionResult> {
        if query.trim().is_empty() {
            return Err(async_graphql::Error::new(
                "query is required for bulk mutations — pass 'all:true' to explicitly target everything (API_SPEC §5)",
            ));
        }
        public_gate::require(ctx.data_unchecked::<Arc<Prefs>>(), &[WRITE])?;
        let ids: Vec<Value> =
            host_json(ctx, "systemCallIds", serde_json::json!({ "query": query }))
                .await?
                .iter()
                .filter_map(Value::as_str)
                .map(Value::from)
                .collect();
        let deleted: Vec<Value> = serde_json::from_value(
            ctx.data_unchecked::<Arc<Host>>()
                .call(
                    "systemDeleteRecords",
                    serde_json::json!({"provider": "CALL", "ids": ids}),
                )
                .await
                .map_err(|error| async_graphql::Error::new(error))?,
        )
        .map_err(|error| async_graphql::Error::new(error.to_string()))?;
        Ok(ActionResult {
            affected_count: deleted.len() as i32,
        })
    }

    /// `showDialer` hands the number to the dialer instead of placing the
    /// call outright.
    async fn call(
        &self,
        ctx: &Context<'_>,
        number: String,
        show_dialer: bool,
    ) -> async_graphql::Result<bool> {
        public_gate::require(ctx.data_unchecked::<Arc<Prefs>>(), &["CALL_PHONE"])?;
        ctx.data_unchecked::<Arc<Host>>()
            .call(
                "systemMakeCall",
                serde_json::json!({"number": number, "showDialer": show_dialer}),
            )
            .await
            .map_err(|error| async_graphql::Error::new(error))?;
        Ok(true)
    }
}

fn call(db: &Arc<Db>, fact: &Value) -> Call {
    let id = text(fact, "id");
    Call {
        tags: crate::db::tag::tags_for_key_of_kind(db, &id, DataType::Call.kind())
            .unwrap_or_default()
            .into_iter()
            .map(|tag| Tag {
                id: ID::from(tag.id),
                name: tag.name,
                count: tag.count,
            })
            .collect(),
        id: ID::from(id),
        number: text(fact, "number"),
        name: text(fact, "name"),
        photo_id: text(fact, "photoId"),
        started_at: instant(fact, "startedAt"),
        duration_sec: integer(fact, "durationSec") as i32,
        r#type: CallType::from_android(integer(fact, "type")),
        account_id: ID::from(text(fact, "accountId")),
        geo: fact["geo"].as_object().map(|geo| PhoneGeo {
            country: text(&Value::Object(geo.clone()), "country"),
            number_type: text(&Value::Object(geo.clone()), "numberType"),
            carrier: text(&Value::Object(geo.clone()), "carrier"),
            description: text(&Value::Object(geo.clone()), "description"),
        }),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/content_api/public_calls.rs"]
mod tests;
