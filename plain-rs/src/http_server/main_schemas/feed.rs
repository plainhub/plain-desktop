use super::content_common;
use super::types::{ActionResult, Instant, Tag};
use async_graphql::{ComplexObject, Context, ID, Object, SimpleObject};
use std::sync::Arc;

use crate::api::context::AppCtx;
use crate::enums::DataType;
use crate::feeds;
use crate::db::{
    Db,
    notes_feeds::{FeedEntryRow, FeedRow},
};

type GqlResult<T> = async_graphql::Result<T>;

#[derive(SimpleObject, Clone)]
pub struct Feed {
    pub id: ID,
    pub name: String,
    pub url: String,
    pub fetch_content: bool,
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
    pub published_at: Instant,
    pub created_at: Instant,
    pub updated_at: Instant,
}

#[ComplexObject]
impl FeedEntry {
    async fn tags(&self, ctx: &Context<'_>) -> GqlResult<Vec<Tag>> {
        content_common::tags(ctx, self.id.as_str(), DataType::FeedEntry.kind())
    }

    async fn feed(&self, ctx: &Context<'_>) -> GqlResult<Option<Feed>> {
        let db = ctx.data::<Arc<Db>>()?;
        feeds::get(db, self.feed_id.as_str())?
            .map(feed_model)
            .transpose()
    }
}

#[derive(SimpleObject, Clone)]
pub struct FeedEntryCount {
    pub id: ID,
    pub count: i32,
}

fn feed_model(row: FeedRow) -> GqlResult<Feed> {
    Ok(Feed {
        id: ID(row.id),
        name: row.name,
        url: row.url,
        fetch_content: row.fetch_content,
        created_at: content_common::instant(&row.created_at)?,
        updated_at: content_common::instant(&row.updated_at)?,
    })
}

fn entry_model(row: FeedEntryRow) -> GqlResult<FeedEntry> {
    Ok(FeedEntry {
        id: ID(row.id),
        feed_id: ID(row.feed_id),
        title: row.title,
        url: row.url,
        image: row.image,
        description: row.description,
        author: row.author,
        content: row.content,
        raw_id: row.raw_id,
        published_at: content_common::instant(&row.published_at)?,
        created_at: content_common::instant(&row.created_at)?,
        updated_at: content_common::instant(&row.updated_at)?,
    })
}

#[derive(Default)]
pub struct FeedQuery;

#[Object]
impl FeedQuery {
    async fn feeds(&self, ctx: &Context<'_>) -> GqlResult<Vec<Feed>> {
        feeds::list(ctx.data::<Arc<Db>>()?)?
            .into_iter()
            .map(feed_model)
            .collect()
    }

    async fn feed_entry_counts(&self, ctx: &Context<'_>) -> GqlResult<Vec<FeedEntryCount>> {
        Ok(feeds::entry_counts(ctx.data::<Arc<Db>>()?)?
            .into_iter()
            .map(|(id, count)| FeedEntryCount { id: ID(id), count })
            .collect())
    }

    async fn feed_entry_count(&self, ctx: &Context<'_>, query: String) -> GqlResult<i32> {
        Ok(feeds::count(ctx.data::<Arc<Db>>()?, &query)?)
    }

    async fn feed_entry(&self, ctx: &Context<'_>, id: ID) -> GqlResult<Option<FeedEntry>> {
        feeds::entry_get(ctx.data::<Arc<Db>>()?, id.as_str())?
            .map(entry_model)
            .transpose()
    }

    async fn feed_entries(
        &self,
        ctx: &Context<'_>,
        offset: i32,
        limit: i32,
        query: String,
    ) -> GqlResult<Vec<FeedEntry>> {
        feeds::search(ctx.data::<Arc<Db>>()?, &query, limit, offset)?
            .into_iter()
            .map(entry_model)
            .collect()
    }
}

#[derive(Default)]
pub struct FeedMutation;

#[Object]
impl FeedMutation {
    async fn sync_feeds(&self, ctx: &Context<'_>, id: Option<ID>) -> GqlResult<bool> {
        let db = ctx.data::<Arc<Db>>()?.clone();
        let events = ctx.data::<Arc<AppCtx>>()?.event_tx.clone();
        feeds::queue_sync(db, events, id.map(|id| id.to_string()));
        Ok(true)
    }

    async fn update_feed(
        &self,
        ctx: &Context<'_>,
        id: ID,
        name: String,
        fetch_content: bool,
    ) -> GqlResult<Feed> {
        feed_model(feeds::update(
            ctx.data::<Arc<Db>>()?,
            id.as_str(),
            &name,
            fetch_content,
        )?)
    }

    async fn create_feed(
        &self,
        ctx: &Context<'_>,
        url: String,
        fetch_content: bool,
    ) -> GqlResult<Feed> {
        let db = ctx.data::<Arc<Db>>()?.clone();
        let events = ctx.data::<Arc<AppCtx>>()?.event_tx.clone();
        feed_model(feeds::create(db, &url, fetch_content, events).await?)
    }

    async fn import_feeds(&self, ctx: &Context<'_>, content: String) -> GqlResult<bool> {
        feeds::import_opml(ctx.data::<Arc<Db>>()?, &content)?;
        Ok(true)
    }

    async fn export_feeds(&self, ctx: &Context<'_>) -> GqlResult<String> {
        Ok(feeds::export_opml(ctx.data::<Arc<Db>>()?)?)
    }

    async fn delete_feed(&self, ctx: &Context<'_>, id: ID) -> GqlResult<bool> {
        Ok(feeds::delete(ctx.data::<Arc<Db>>()?, id.as_str())?)
    }

    async fn sync_feed_entry_content(&self, ctx: &Context<'_>, id: ID) -> GqlResult<FeedEntry> {
        entry_model(feeds::sync_entry_content(ctx.data::<Arc<Db>>()?, id.as_str()).await?)
    }

    async fn delete_feed_entries(
        &self,
        ctx: &Context<'_>,
        query: String,
    ) -> GqlResult<ActionResult> {
        Ok(ActionResult {
            affected_count: feeds::delete_entries(ctx.data::<Arc<Db>>()?, &query)? as i32,
        })
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/api/schema/feed.rs"]
mod tests;
