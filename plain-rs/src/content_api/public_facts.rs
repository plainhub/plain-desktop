//! Host fact decoding for the public `/graphql` schema.
//!
//! The platform layer answers with JSON (`systemPackageFacts`,
//! `systemNotificationFacts`, …); these helpers turn it into the contract
//! types in [`crate::content_types`]. Missing timestamps fall back to the
//! epoch, matching plain-app's `Instant.fromEpochMilliseconds(0)` for facts
//! the platform could not fill in.

use crate::content_types::Instant;
use async_graphql::ID;
use serde_json::Value;

pub(super) fn text(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or_default().to_string()
}

pub(super) fn flag(value: &Value, key: &str) -> bool {
    value[key].as_bool().unwrap_or(false)
}

pub(super) fn id(value: &Value, key: &str) -> ID {
    ID::from(text(value, key))
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

pub(super) fn i64(value: &Value, key: &str) -> i64 {
    value[key].as_i64().unwrap_or_default()
}
