//! Host access and fact decoding for the public `/graphql` schema.
//!
//! The platform layer answers with JSON (`systemPackageFacts`,
//! `systemContactFacts`, …); these helpers turn it into the contract types
//! in [`crate::content_types`]. Missing timestamps fall back to the epoch,
//! matching plain-app's `Instant.fromEpochMilliseconds(0)` for facts the
//! platform could not fill in.

use crate::content_api::host::Host;
use crate::content_types::Instant;
use async_graphql::{Context, ID};
use serde_json::Value;
use std::sync::Arc;

/// One host round trip returning a JSON array of facts.
pub(super) async fn host_json(
    ctx: &Context<'_>,
    method: &str,
    params: Value,
) -> async_graphql::Result<Vec<Value>> {
    let facts = ctx
        .data_unchecked::<Arc<Host>>()
        .call(method, params)
        .await
        .map_err(|error| async_graphql::Error::new(error))?;
    serde_json::from_value(facts).map_err(|error| async_graphql::Error::new(error.to_string()))
}

pub(super) fn text(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or_default().to_string()
}

pub(super) fn flag(value: &Value, key: &str) -> bool {
    value[key].as_bool().unwrap_or(false)
}

pub(super) fn id(value: &Value, key: &str) -> ID {
    ID::from(text(value, key))
}

pub(super) fn integer(value: &Value, key: &str) -> i64 {
    value[key].as_i64().unwrap_or_default()
}

pub(super) fn instant(value: &Value, key: &str) -> Instant {
    optional_instant(value, key).unwrap_or(epoch())
}

pub(super) fn optional_instant(value: &Value, key: &str) -> Option<Instant> {
    value[key]
        .as_str()
        .and_then(|raw| chrono::DateTime::parse_from_rfc3339(raw).ok())
        .map(|parsed| Instant(parsed.with_timezone(&chrono::Utc)))
}

fn epoch() -> Instant {
    Instant(chrono::DateTime::from_timestamp(0, 0).unwrap_or_default())
}

pub(super) fn strings(value: &Value, key: &str) -> Vec<String> {
    value[key]
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|item| item.as_str().unwrap_or_default().to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// Maps a nested array of raw facts, dropping anything that is not an object
/// so one malformed row cannot fail the whole page.
pub(super) fn list<T>(value: &Value, key: &str, item: impl Fn(&Value) -> T) -> Vec<T> {
    value[key]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter(|item| item.is_object())
                .map(item)
                .collect()
        })
        .unwrap_or_default()
}
