use super::types::{Instant, parse_instant};
use crate::{
    db::{Db, ShareRow},
    shares::{Root, Service},
};
use async_graphql::{Context, ID, Object, SimpleObject};
use std::sync::Arc;

#[derive(SimpleObject)]
struct ShareRecord {
    id: ID,
    name: String,
    password: String,
    url_token: String,
    read_only: bool,
    expires_at: Option<Instant>,
    created_at: Instant,
    updated_at: Instant,
    roots: Vec<Root>,
}
fn record(row: ShareRow) -> async_graphql::Result<ShareRecord> {
    let roots = Service::roots(&row).map_err(|e| async_graphql::Error::new(e.to_string()))?;
    Ok(ShareRecord {
        id: ID(row.id),
        name: row.name,
        password: row.password,
        url_token: row.url_token,
        read_only: row.read_only,
        expires_at: row.expires_at.as_deref().map(parse_instant).transpose()?,
        created_at: parse_instant(&row.created_at)?,
        updated_at: parse_instant(&row.updated_at)?,
        roots,
    })
}
#[derive(SimpleObject)]
struct ShareAuthRecord {
    share: ShareRecord,
    token: String,
}
#[derive(SimpleObject)]
struct ShareListing {
    share: ShareRecord,
    entries: Vec<Root>,
}
#[derive(Default)]
pub struct ShareHostQuery;
#[Object]
impl ShareHostQuery {
    async fn browse_share(
        &self,
        ctx: &Context<'_>,
        id: ID,
        virtual_path: String,
    ) -> async_graphql::Result<ShareListing> {
        let service = ctx.data::<Arc<Service>>()?.clone();
        let (row, entries) = blocking(move || service.browse(&id, &virtual_path)).await?;
        Ok(ShareListing {
            share: record(row)?,
            entries,
        })
    }
    async fn share_zip_entries(
        &self,
        ctx: &Context<'_>,
        id: ID,
        file_id: String,
    ) -> async_graphql::Result<Vec<Root>> {
        let service = ctx.data::<Arc<Service>>()?.clone();
        blocking(move || service.archive(&id, &file_id)).await
    }
    async fn share_record(
        &self,
        ctx: &Context<'_>,
        id: ID,
    ) -> async_graphql::Result<Option<ShareRecord>> {
        ctx.data::<Arc<Db>>()?
            .share_get(&id)?
            .map(record)
            .transpose()
    }
    async fn share_records(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<ShareRecord>> {
        ctx.data::<Arc<Db>>()?
            .share_list()?
            .into_iter()
            .map(record)
            .collect()
    }
    async fn share_token(&self, ctx: &Context<'_>, id: ID) -> async_graphql::Result<String> {
        ctx.data::<Arc<Service>>()?
            .token(&id)
            .map_err(|e| async_graphql::Error::new(e.to_string()))
    }
    async fn share_auth(
        &self,
        ctx: &Context<'_>,
        id: ID,
    ) -> async_graphql::Result<Option<ShareAuthRecord>> {
        let service = ctx.data::<Arc<Service>>()?;
        let Some(row) = service
            .active(&id, true)
            .map_err(|e| async_graphql::Error::new(e.to_string()))?
        else {
            return Ok(None);
        };
        let token = service
            .token(&id)
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        Ok(Some(ShareAuthRecord {
            share: record(row)?,
            token,
        }))
    }
    async fn resolve_share_path(
        &self,
        ctx: &Context<'_>,
        id: ID,
        virtual_path: String,
    ) -> async_graphql::Result<Option<String>> {
        ctx.data::<Arc<Service>>()?
            .resolve(&id, &virtual_path, true)
            .map_err(|e| async_graphql::Error::new(e.to_string()))
    }
    async fn resolve_share_file(
        &self,
        ctx: &Context<'_>,
        id: ID,
        file_id: String,
    ) -> async_graphql::Result<Option<String>> {
        ctx.data::<Arc<Service>>()?
            .resolve_file(&id, &file_id)
            .map_err(|e| async_graphql::Error::new(e.to_string()))
    }
}
#[derive(Default)]
pub struct ShareHostMutation;
#[Object]
impl ShareHostMutation {
    async fn create_share(
        &self,
        ctx: &Context<'_>,
        name: String,
        real_paths: Vec<String>,
        url_token: String,
        read_only: bool,
        expires_at: Option<Instant>,
    ) -> async_graphql::Result<ShareRecord> {
        let service = ctx.data::<Arc<Service>>()?.clone();
        record(
            blocking(move || {
                service.create(
                    name,
                    real_paths,
                    url_token,
                    read_only,
                    expires_at.map(|i| i.0.to_rfc3339()),
                )
            })
            .await?,
        )
    }
    async fn update_share(
        &self,
        ctx: &Context<'_>,
        id: ID,
        name: String,
        expires_at: Option<Instant>,
        real_paths: Option<Vec<String>>,
    ) -> async_graphql::Result<ShareRecord> {
        let service = ctx.data::<Arc<Service>>()?.clone();
        record(
            blocking(move || {
                service.update(&id, &name, expires_at.map(|i| i.0.to_rfc3339()), real_paths)
            })
            .await?,
        )
    }
    async fn delete_share(&self, ctx: &Context<'_>, id: ID) -> async_graphql::Result<bool> {
        Ok(ctx.data::<Arc<Db>>()?.share_delete(&id)? > 0)
    }
}

async fn blocking<T: Send + 'static>(
    operation: impl FnOnce() -> anyhow::Result<T> + Send + 'static,
) -> async_graphql::Result<T> {
    tokio::task::spawn_blocking(operation)
        .await?
        .map_err(|e| async_graphql::Error::new(e.to_string()))
}
