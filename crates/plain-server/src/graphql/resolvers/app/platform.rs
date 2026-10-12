use crate::graphql::resolvers::app::capability_types::{AppUpdate, Disk};
use crate::graphql::types::Long;
use crate::host::bridge::Host;
use crate::host::platform::{
    AppUpdateFacts, DisksFacts, SYSTEM_APP_UPDATE_FACTS, SYSTEM_DISKS_FACTS,
};
use async_graphql::{Context, Object, Result};
use std::sync::Arc;

#[derive(Default)]
pub struct PlatformQuery;

#[Object]
impl PlatformQuery {
    async fn disks(&self, ctx: &Context<'_>) -> Result<Vec<Disk>> {
        let value = ctx
            .data::<Arc<Host>>()?
            .call(SYSTEM_DISKS_FACTS, serde_json::json!({}))
            .await
            .map_err(async_graphql::Error::new)?;
        let facts: DisksFacts = serde_json::from_value(value)
            .map_err(|error| async_graphql::Error::new(error.to_string()))?;
        Ok(facts
            .disks
            .into_iter()
            .map(|disk| Disk {
                id: disk.id.into(),
                name: disk.name,
                path: disk.path,
                size_bytes: Long(disk.size_bytes),
                removable: disk.removable,
                model: disk.model,
            })
            .collect())
    }

    async fn app_update(&self, ctx: &Context<'_>) -> Result<AppUpdate> {
        let value = ctx
            .data::<Arc<Host>>()?
            .call(SYSTEM_APP_UPDATE_FACTS, serde_json::json!({}))
            .await
            .map_err(async_graphql::Error::new)?;
        let facts: AppUpdateFacts = serde_json::from_value(value)
            .map_err(|error| async_graphql::Error::new(error.to_string()))?;
        Ok(AppUpdate {
            current_version: facts.current_version,
            latest_version: facts.latest_version,
            has_update: facts.has_update,
            url: facts.url,
        })
    }
}
