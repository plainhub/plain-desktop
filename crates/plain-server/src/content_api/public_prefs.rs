//! Public preference roots share the local Rust store and notify UI projections.

use crate::prefs::Prefs;
use crate::ws_event::WsEvent;
use async_graphql::{Context, Json, Object};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::broadcast;

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
        super::preferences::set(
            prefs(ctx),
            ctx.data_unchecked::<broadcast::Sender<WsEvent>>(),
            true,
            &key,
            value.0,
        )
        .map_err(async_graphql::Error::new)?;
        Ok(true)
    }

    async fn remove_user_pref(
        &self,
        ctx: &Context<'_>,
        key: String,
    ) -> async_graphql::Result<bool> {
        validate_key(&key)?;
        super::preferences::remove(
            prefs(ctx),
            ctx.data_unchecked::<broadcast::Sender<WsEvent>>(),
            true,
            &key,
        )
        .map_err(async_graphql::Error::new)?;
        Ok(true)
    }
}

fn prefs<'a>(ctx: &'a Context<'_>) -> &'a Arc<Prefs> {
    ctx.data_unchecked::<Arc<Prefs>>()
}

#[cfg(test)]
#[path = "../../tests/unit/content_api/public_prefs.rs"]
mod tests;
