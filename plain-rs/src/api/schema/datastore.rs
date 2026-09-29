use async_graphql::{Context, Error, Object, Result};
use std::sync::Arc;

use super::super::context::AppCtx;
use super::types::KeyValuePair;
use crate::prefs::Prefs;

const PREF_PREFIX: &str = "admin.";

fn pref_key(key: &str) -> Result<String> {
    if key.is_empty()
        || key.len() > 128
        || !key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
    {
        return Err(Error::new("invalid_pref_key"));
    }
    Ok(format!("{PREF_PREFIX}{key}"))
}

fn list_prefs(prefs: &Prefs) -> Vec<KeyValuePair> {
    prefs
        .entries()
        .into_iter()
        .filter_map(|(key, value)| {
            let key = key.strip_prefix(PREF_PREFIX)?;
            Some(KeyValuePair {
                key: key.to_string(),
                value: value.as_str()?.to_string(),
            })
        })
        .collect()
}

#[derive(Default)]
pub struct DataStoreQuery;

#[Object]
impl DataStoreQuery {
    async fn prefs(&self, ctx: &Context<'_>) -> Vec<KeyValuePair> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        list_prefs(&c.prefs)
    }

    async fn data_store_path(&self, ctx: &Context<'_>) -> String {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        c.prefs.path().display().to_string()
    }

    async fn data_store_entries(&self, ctx: &Context<'_>) -> Vec<KeyValuePair> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        c.prefs
            .entries_sorted()
            .into_iter()
            .map(|(key, value)| KeyValuePair { key, value })
            .collect()
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/api/datastore.rs"]
mod tests;

#[derive(Default)]
pub struct DataStoreMutation;

#[Object]
impl DataStoreMutation {
    async fn set_pref(
        &self,
        ctx: &Context<'_>,
        key: String,
        value: String,
    ) -> Result<KeyValuePair> {
        if value.len() > 65536 {
            return Err(Error::new("invalid_pref_value"));
        }
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        c.prefs
            .set(&pref_key(&key)?, &value)
            .map_err(|e| Error::new(e.to_string()))?;
        Ok(KeyValuePair { key, value })
    }

    async fn delete_pref(&self, ctx: &Context<'_>, key: String) -> Result<bool> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        c.prefs
            .remove(&pref_key(&key)?)
            .map_err(|e| Error::new(e.to_string()))?;
        Ok(true)
    }

    async fn delete_data_store_entry(&self, ctx: &Context<'_>, key: String) -> bool {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        // Synchronous disk write inside async context: bounded (few KB
        // file, atomic tmp+rename) and the same contract plain-nas uses.
        c.prefs.remove(&key).is_ok()
    }
}
