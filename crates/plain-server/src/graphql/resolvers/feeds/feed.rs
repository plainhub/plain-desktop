use crate::graphql::resolvers::common::content_common;
use crate::graphql::resolvers::tags::domain_types::Tag;
use crate::graphql::types::ActionResult;
use crate::graphql::types::Instant;
use async_graphql::{ComplexObject, Context, ID, Object, SimpleObject};
use std::sync::Arc;

#[cfg(feature = "api")]
use crate::assembly::context::AppCtx;
use crate::services::feeds;
use plain_rs::db::Db;
use plain_rs::db::notes_feeds::FeedEntryRow;
use plain_rs::db::notes_feeds::FeedRow;
use plain_rs::enums::DataType;

type GqlResult<T> = async_graphql::Result<T>;

#[derive(SimpleObject, Clone)]
pub struct FeedError {
    pub code: String,
    pub detail: String,
}

#[derive(SimpleObject, Clone)]
pub struct Feed {
    pub id: ID,
    pub name: String,
    pub url: String,
    pub fetch_content: bool,
    pub logo: String,
    pub last_sync_at: Option<Instant>,
    pub last_error: FeedError,
    pub created_at: Instant,
    pub updated_at: Instant,
}

#[derive(SimpleObject, Clone)]
#[graphql(complex)]
pub struct FeedEntry {
    pub id: ID,
    pub title: String,
    pub url: String,
    pub image: String,
    pub description: String,
    pub author: String,
    pub content: String,
    pub feed_id: ID,
    pub raw_id: String,
    pub read: bool,
    pub published_at: Instant,
    pub created_at: Instant,
    pub updated_at: Instant,
}

#[ComplexObject]
impl FeedEntry {
    async fn tags(&self, ctx: &Context<'_>) -> GqlResult<Vec<Tag>> {
        #[cfg(feature = "core")]
        if ctx
            .data_opt::<async_graphql::dataloader::DataLoader<
                crate::graphql::resolvers::tags::loader::TagsLoader,
            >>()
            .is_some()
        {
            return crate::graphql::resolvers::tags::loader::load(
                ctx,
                DataType::FeedEntry,
                self.id.as_str(),
            )
            .await;
        }
        content_common::tags(ctx, self.id.as_str(), DataType::FeedEntry.kind())
    }

    async fn feed(&self, ctx: &Context<'_>) -> GqlResult<Option<Feed>> {
        let db = ctx.data::<Arc<Db>>()?;
        plain_rs::feeds::get(db, self.feed_id.as_str())?
            .map(|row| feed_model(ctx, row))
            .transpose()
    }
}

#[derive(SimpleObject, Clone)]
pub struct FeedEntryCount {
    pub id: ID,
    pub count: i32,
}

fn feed_model(_ctx: &Context<'_>, row: FeedRow) -> GqlResult<Feed> {
    Ok(Feed {
        id: ID(row.id),
        name: row.name,
        url: row.url,
        fetch_content: row.fetch_content,
        logo: row.logo,
        last_sync_at: row
            .last_sync_at
            .as_deref()
            .map(content_common::instant)
            .transpose()?,
        last_error: {
            let value: serde_json::Value =
                serde_json::from_str(&row.last_error).unwrap_or_default();
            FeedError {
                code: value["code"].as_str().unwrap_or_default().into(),
                detail: value["detail"].as_str().unwrap_or_default().into(),
            }
        },
        created_at: content_common::instant(&row.created_at)?,
        updated_at: content_common::instant(&row.updated_at)?,
    })
}

fn entry_model(ctx: &Context<'_>, row: FeedEntryRow) -> GqlResult<FeedEntry> {
    Ok(FeedEntry {
        id: ID(row.id),
        feed_id: ID(row.feed_id),
        title: row.title,
        url: row.url,
        image: entry_image(ctx, &row.image)?,
        description: row.description,
        author: row.author,
        content: row.content,
        raw_id: row.raw_id,
        read: row.read,
        published_at: content_common::instant(&row.published_at)?,
        created_at: content_common::instant(&row.created_at)?,
        updated_at: content_common::instant(&row.updated_at)?,
    })
}

#[derive(Default)]
pub struct FeedQuery;

#[Object]
impl FeedQuery {
    async fn feed_sync_states(&self, ctx: &Context<'_>) -> GqlResult<Vec<feeds::FeedSyncState>> {
        Ok(sync_service(ctx)?.states())
    }
    async fn feed(&self, ctx: &Context<'_>, id: ID) -> GqlResult<Option<Feed>> {
        plain_rs::feeds::get(ctx.data::<Arc<Db>>()?, id.as_str())?
            .map(|row| feed_model(ctx, row))
            .transpose()
    }
    async fn feeds(&self, ctx: &Context<'_>) -> GqlResult<Vec<Feed>> {
        plain_rs::feeds::list(ctx.data::<Arc<Db>>()?)?
            .into_iter()
            .map(|row| feed_model(ctx, row))
            .collect()
    }

    async fn feed_entry_counts(&self, ctx: &Context<'_>) -> GqlResult<Vec<FeedEntryCount>> {
        Ok(plain_rs::feeds::entry_counts(ctx.data::<Arc<Db>>()?)?
            .into_iter()
            .map(|(id, count)| FeedEntryCount { id: ID(id), count })
            .collect())
    }

    async fn feed_entry_count(&self, ctx: &Context<'_>, query: String) -> GqlResult<i32> {
        Ok(plain_rs::feeds::count(ctx.data::<Arc<Db>>()?, &query)?)
    }

    async fn feed_entry(&self, ctx: &Context<'_>, id: ID) -> GqlResult<Option<FeedEntry>> {
        plain_rs::feeds::entry_get(ctx.data::<Arc<Db>>()?, id.as_str())?
            .map(|row| entry_model(ctx, row))
            .transpose()
    }

    async fn feed_entries(
        &self,
        ctx: &Context<'_>,
        query: String,
        offset: i32,
        limit: i32,
    ) -> GqlResult<Vec<FeedEntry>> {
        plain_rs::feeds::search(ctx.data::<Arc<Db>>()?, &query, limit, offset)?
            .into_iter()
            .map(|row| entry_model(ctx, row))
            .collect()
    }
}

#[derive(Default)]
pub struct FeedMutation;

#[Object]
impl FeedMutation {
    async fn update_feed_url(&self, ctx: &Context<'_>, id: ID, url: String) -> GqlResult<Feed> {
        feed_model(
            ctx,
            plain_rs::feeds::update_url(ctx.data::<Arc<Db>>()?, id.as_str(), &url)?,
        )
    }
    async fn mark_feed_entries_read(
        &self,
        ctx: &Context<'_>,
        query: String,
        read: bool,
    ) -> GqlResult<ActionResult> {
        Ok(ActionResult {
            affected_count: plain_rs::feeds::mark_read(ctx.data::<Arc<Db>>()?, &query, read)?
                as i32,
        })
    }

    async fn sync_feeds(&self, ctx: &Context<'_>, id: Option<ID>) -> GqlResult<bool> {
        sync_service(ctx)?.queue(id.map(|id| id.to_string()));
        Ok(true)
    }

    async fn update_feed(
        &self,
        ctx: &Context<'_>,
        id: ID,
        name: String,
        fetch_content: bool,
    ) -> GqlResult<Feed> {
        feed_model(
            ctx,
            plain_rs::feeds::update(ctx.data::<Arc<Db>>()?, id.as_str(), &name, fetch_content)?,
        )
    }

    async fn create_feed(
        &self,
        ctx: &Context<'_>,
        url: String,
        fetch_content: bool,
    ) -> GqlResult<Feed> {
        let db = ctx.data::<Arc<Db>>()?.clone();
        let feed = feeds::create_without_sync(&db, &url, fetch_content).await?;
        sync_service(ctx)?.queue(Some(feed.id.clone()));
        feed_model(ctx, feed)
    }

    async fn import_feeds(&self, ctx: &Context<'_>, content: String) -> GqlResult<bool> {
        plain_rs::feeds::import_opml(ctx.data::<Arc<Db>>()?, &content)?;
        Ok(true)
    }

    async fn export_feeds(&self, ctx: &Context<'_>) -> GqlResult<String> {
        Ok(plain_rs::feeds::export_opml(ctx.data::<Arc<Db>>()?)?)
    }

    async fn delete_feed(&self, ctx: &Context<'_>, id: ID) -> GqlResult<bool> {
        let db = ctx.data::<Arc<Db>>()?;
        let feed = plain_rs::feeds::get(db, id.as_str())?;
        let images = plain_rs::feeds::search(db, &format!("feed_id:{}", id.as_str()), i32::MAX, 0)?;
        let deleted = plain_rs::feeds::delete(db, id.as_str())?;
        if deleted {
            if let Some(assets) = feed_assets(ctx) {
                if let Some(feed) = feed {
                    assets.release(&feed.logo);
                }
                for entry in images {
                    assets.release(&entry.image);
                }
            }
        }
        Ok(deleted)
    }

    async fn sync_feed_entry_content(&self, ctx: &Context<'_>, id: ID) -> GqlResult<FeedEntry> {
        let assets = ctx.data_opt::<Arc<feeds::FeedAssets>>().cloned();
        #[cfg(feature = "api")]
        let assets = assets.or_else(|| {
            ctx.data_opt::<Arc<AppCtx>>().map(|app| {
                Arc::new(feeds::FeedAssets {
                    db: app.db.clone(),
                    directory: app.data_dir.clone(),
                })
            })
        });
        entry_model(
            ctx,
            feeds::sync_entry_content_with_assets(
                ctx.data::<Arc<Db>>()?,
                id.as_str(),
                assets.as_deref(),
            )
            .await?,
        )
    }

    async fn delete_feed_entries(
        &self,
        ctx: &Context<'_>,
        query: String,
    ) -> GqlResult<ActionResult> {
        let db = ctx.data::<Arc<Db>>()?;
        if query.trim().is_empty() {
            return Err("query is required".into());
        }
        let entries = plain_rs::feeds::search(db, &query, i32::MAX, 0)?;
        let affected_count = plain_rs::feeds::delete_entries(db, &query)? as i32;
        if let Some(assets) = feed_assets(ctx) {
            for entry in entries {
                assets.release(&entry.image);
            }
        }
        Ok(ActionResult { affected_count })
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/graphql/resolvers/feeds/feed.rs"]
mod tests;

fn sync_service(ctx: &Context<'_>) -> GqlResult<Arc<feeds::SyncService>> {
    if let Some(service) = ctx.data_opt::<Arc<feeds::SyncService>>() {
        return Ok(service.clone());
    }
    #[cfg(feature = "api")]
    if let Some(app) = ctx.data_opt::<Arc<AppCtx>>() {
        return Ok(app.feed_sync.clone());
    }
    Err(async_graphql::Error::new("feed sync service unavailable"))
}

fn entry_image(ctx: &Context<'_>, uri: &str) -> GqlResult<String> {
    #[cfg(feature = "api")]
    if let Some(app) = ctx.data_opt::<Arc<AppCtx>>() {
        if uri.is_empty() || uri.starts_with("http://") || uri.starts_with("https://") {
            return Ok(uri.into());
        }
        return plain_rs::crypto::xchacha_encrypt(&app.token, uri.as_bytes())
            .map(|bytes| plain_rs::utils::base64::base64_encode(&bytes))
            .ok_or_else(|| "invalid file token".into());
    }
    let _ = ctx;
    Ok(uri.into())
}

fn feed_assets(ctx: &Context<'_>) -> Option<Arc<feeds::FeedAssets>> {
    let assets = ctx.data_opt::<Arc<feeds::FeedAssets>>().cloned();
    #[cfg(feature = "api")]
    let assets = assets.or_else(|| {
        ctx.data_opt::<Arc<AppCtx>>().map(|app| {
            Arc::new(feeds::FeedAssets {
                db: app.db.clone(),
                directory: app.data_dir.clone(),
            })
        })
    });
    assets
}
