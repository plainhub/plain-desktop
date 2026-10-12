//! Public `/graphql` call-log roots.
//!
//! Reads reuse the same provider search the contact roots use, so a call and
//! a contact from the same query DSL behave alike.

use crate::graphql::resolvers::tags::domain_types::Tag;
use crate::graphql::types::ActionResult;
use crate::graphql::types::facts::host_json;
use crate::graphql::types::facts::instant;
use crate::graphql::types::facts::integer;
use crate::graphql::types::facts::text;
use crate::host::bridge::Host;
use crate::host::query as provider_plan;
use crate::host::query::Provider;
use crate::http::auth::public_gate;
use async_graphql::{Context, Enum, ID, Object, SimpleObject};
use plain_rs::db::Db;
use plain_rs::prefs::Prefs;
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
#[graphql(complex)]
pub struct Call {
    pub id: ID,
    pub number: String,
    pub name: String,
    #[graphql(name = "photoId")]
    pub photo_id: String,
    #[graphql(name = "startedAt")]
    pub started_at: crate::graphql::types::Instant,
    #[graphql(name = "durationSec")]
    pub duration_sec: i32,
    pub r#type: CallType,
    #[graphql(name = "accountId")]
    pub account_id: ID,
    pub geo: Option<PhoneGeo>,
}

#[async_graphql::ComplexObject]
impl Call {
    async fn tags(&self, ctx: &async_graphql::Context<'_>) -> async_graphql::Result<Vec<Tag>> {
        crate::graphql::resolvers::tags::loader::load(
            ctx,
            plain_rs::enums::DataType::Call,
            self.id.as_str(),
        )
        .await
    }
}

#[derive(Default)]
pub struct CallsQuery;

#[Object]
impl CallsQuery {
    async fn calls(
        &self,
        ctx: &Context<'_>,
        query: String,
        offset: i32,
        limit: i32,
    ) -> async_graphql::Result<Vec<Call>> {
        public_gate::require(ctx, ctx.data_unchecked::<Arc<Prefs>>(), &[READ])?;
        let facts = host_json(
            ctx,
            "systemCallFacts",
            serde_json::json!({"query": query, "offset": offset, "limit": limit}),
        )
        .await?;
        let host = ctx.data_unchecked::<Arc<Host>>();
        let locale = host
            .call("systemPhoneLocaleFacts", serde_json::json!({}))
            .await
            .ok()
            .and_then(|facts| {
                serde_json::from_value::<crate::mobile::phone_calls::geo::LocaleFacts>(facts).ok()
            });
        let runtime = ctx.data_unchecked::<Arc<crate::mobile::phone_calls::geo::Runtime>>();
        let prefs = ctx.data_unchecked::<Arc<Prefs>>();
        let mut calls = Vec::with_capacity(facts.len());
        for fact in &facts {
            let mut row = call(fact);
            let path = text(fact, "photoUri");
            row.photo_id = if path.is_empty()
                || path.to_ascii_lowercase().starts_with("http://")
                || path.to_ascii_lowercase().starts_with("https://")
            {
                path
            } else {
                plain_rs::chat::messages::content::make_file_id(
                    &path,
                    &prefs.get_or(plain_rs::prefs::system::keys::URL_TOKEN, String::new()),
                )
            };
            if let Some(locale) = &locale {
                row.geo = runtime.lookup(host, &row.number, locale).await;
            }
            calls.push(row);
        }
        Ok(calls)
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
                "query is required for bulk mutations — pass 'all:true' to explicitly target everything",
            ));
        }
        public_gate::require(ctx, ctx.data_unchecked::<Arc<Prefs>>(), &[WRITE])?;
        // Call contributes no clause of its own, so a query carrying only
        // fields it does not act on resolves to the entire call log.
        crate::mobile::query::require_narrowing(
            ctx.data_unchecked::<Arc<Db>>(),
            ctx.data_unchecked::<Arc<Host>>(),
            Provider::Call,
            &query,
        )
        .await
        .map_err(|error| async_graphql::Error::new(error.to_string()))?;
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
        public_gate::require(ctx, ctx.data_unchecked::<Arc<Prefs>>(), &["CALL_PHONE"])?;
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

fn call(fact: &Value) -> Call {
    let id = text(fact, "id");
    Call {
        id: ID::from(id),
        number: text(fact, "number"),
        name: text(fact, "name"),
        photo_id: String::new(),
        started_at: instant(fact, "startedAt"),
        duration_sec: integer(fact, "durationSec") as i32,
        r#type: CallType::from_android(integer(fact, "type")),
        account_id: ID::from(text(fact, "accountId")),
        geo: None,
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/mobile/phone_calls/resolvers.rs"]
mod tests;
