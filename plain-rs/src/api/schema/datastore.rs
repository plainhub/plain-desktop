use async_graphql::{Context, Object};
use std::sync::Arc;

use super::super::context::AppCtx;
use super::types::KeyValuePair;

#[derive(Default)]
pub struct DataStoreQuery;

#[Object]
impl DataStoreQuery {
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

#[derive(Default)]
pub struct DataStoreMutation;

#[Object]
impl DataStoreMutation {
    async fn delete_data_store_entry(&self, ctx: &Context<'_>, key: String) -> bool {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        // Synchronous disk write inside async context: bounded (few KB
        // file, atomic tmp+rename) and the same contract plain-nas uses.
        c.prefs.remove(&key).is_ok()
    }
}
