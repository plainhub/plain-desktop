//! Public `/graphql` preference roots.
//!
//! Reads come straight from the Rust store the app already persists to.
//! Writes go back through the platform instead, because the platform keeps
//! an in-memory copy of every pref and fans the change out to the live
//! `StateFlow`s the UI collects. Writing the file from here would persist
//! the value and leave every observer showing the old one.

use super::host::Host;
use crate::prefs::Prefs;
use async_graphql::{Context, Json, Object};
use serde_json::Value;
use std::sync::Arc;

/// The same bounds the platform enforces: a pref key ends up in a file name
/// and a JSON object, so it is restricted to characters that survive both.
fn validate_key(key: &str) -> async_graphql::Result<()> {
    if key.is_empty()
        || key.len() > 128
        || !key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
    {
        return Err(async_graphql::Error::new("invalid_pref_key"));
    }
    Ok(())
}

/// Measured in bytes, not characters: the limit is what the store has to
/// hold, and a multi-byte value would slip past a character count.
fn validate_value(value: &Value) -> async_graphql::Result<()> {
    if value.to_string().len() > 65536 {
        return Err(async_graphql::Error::new("invalid_pref_value"));
    }
    Ok(())
}

#[derive(Default)]
pub struct PrefsQuery;

#[Object]
impl PrefsQuery {
    async fn user_prefs(&self, ctx: &Context<'_>) -> Json<Value> {
        Json(Value::Object(
            prefs(ctx).user_entries().into_iter().collect(),
        ))
    }

    async fn system_prefs(&self, ctx: &Context<'_>) -> Json<Value> {
        Json(Value::Object(prefs(ctx).entries().into_iter().collect()))
    }
}

#[derive(Default)]
pub struct PrefsMutation;

#[Object]
impl PrefsMutation {
    async fn set_user_pref(
        &self,
        ctx: &Context<'_>,
        key: String,
        value: Json<Value>,
    ) -> async_graphql::Result<bool> {
        validate_key(&key)?;
        validate_value(&value.0)?;
        host_call(
            ctx,
            "systemSetUserPref",
            serde_json::json!({ "key": key, "value": value.0 }),
        )
        .await?;
        Ok(true)
    }

    async fn remove_user_pref(
        &self,
        ctx: &Context<'_>,
        key: String,
    ) -> async_graphql::Result<bool> {
        validate_key(&key)?;
        host_call(
            ctx,
            "systemRemoveUserPref",
            serde_json::json!({ "key": key }),
        )
        .await?;
        Ok(true)
    }
}

fn prefs<'a>(ctx: &'a Context<'_>) -> &'a Arc<Prefs> {
    ctx.data_unchecked::<Arc<Prefs>>()
}

async fn host_call(ctx: &Context<'_>, method: &str, params: Value) -> async_graphql::Result<Value> {
    ctx.data_unchecked::<Arc<Host>>()
        .call(method, params)
        .await
        .map_err(async_graphql::Error::new)
}

#[cfg(test)]
#[path = "../../tests/unit/content_api/public_prefs.rs"]
mod tests;
