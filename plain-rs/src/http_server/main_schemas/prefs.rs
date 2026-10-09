use async_graphql::{Context, Error, Json, Object, Result};
use serde_json::{Map, Value};
use std::sync::Arc;

use crate::api::context::AppCtx;

fn validate_pref_key(key: &str) -> Result<()> {
    if key.is_empty()
        || key.len() > 128
        || !key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
    {
        return Err(Error::new("invalid_pref_key"));
    }
    Ok(())
}

#[derive(Default)]
pub struct PrefsQuery;

#[Object]
impl PrefsQuery {
    async fn user_prefs(&self, ctx: &Context<'_>) -> Json<Value> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        Json(Value::Object(
            c.prefs.user_entries().into_iter().collect::<Map<_, _>>(),
        ))
    }

    async fn system_prefs(&self, ctx: &Context<'_>) -> Json<Value> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        Json(Value::Object(
            c.prefs.entries().into_iter().collect::<Map<_, _>>(),
        ))
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/api/prefs.rs"]
mod tests;

#[derive(Default)]
pub struct PrefsMutation;

#[Object]
impl PrefsMutation {
    async fn set_user_pref(
        &self,
        ctx: &Context<'_>,
        key: String,
        value: Json<Value>,
    ) -> Result<bool> {
        validate_pref_key(&key)?;
        if value.0.to_string().len() > 65536 {
            return Err(Error::new("invalid_pref_value"));
        }
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        c.prefs
            .set_user(&key, value.0)
            .map_err(|e| Error::new(e.to_string()))?;
        Ok(true)
    }

    async fn remove_user_pref(&self, ctx: &Context<'_>, key: String) -> Result<bool> {
        validate_pref_key(&key)?;
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        c.prefs
            .remove_user(&key)
            .map_err(|e| Error::new(e.to_string()))?;
        Ok(true)
    }
}
