//! Public `/graphql` feed and feed-entry roots.
//!
//! The rows are Rust SQLite, but fetching an entry's content, releasing the
//! images a deleted entry owned and queueing a sync are all real work, so
//! the roots below call the same `feeds` library the app does rather than
//! re-implementing any of it.
//!
//! `FeedEntry` is declared here instead of reused: the app's version reaches
//! the desktop `Tag` through its `tags` field, and that type carries a
//! numeric kind the contract does not have.
//!
//! `image` and `logo` carry the stored reference verbatim. The app's own
//! roots encrypt them into a url token, but a client builds that token from
//! its own secret, so handing out a pre-encrypted one would be a token the
//! client cannot use.

use super::public_contact_types::Tag;
use super::public_notes::tags_by_key;
use super::schema::feed::{Feed, FeedEntryCount, FeedError};
use crate::content_types::{ActionResult, Instant};
use crate::db::{
    Db,
    notes_feeds::{FeedEntryRow, FeedRow},
};
use crate::enums::DataType;
use crate::feeds;
use async_graphql::{Context, Object, SimpleObject};
use std::sync::Arc;

#[derive(SimpleObject, Clone)]
pub struct FeedEntry {
    #[graphql(name = "feedId")]
    pub feed_id: async_graphql::ID,
    pub id: async_graphql::ID,
    pub title: String,
    pub url: String,
    pub image: String,
    pub description: String,
    pub author: String,
    pub content: String,
    #[graphql(name = "rawId")]
    pub raw_id: String,
    pub read: bool,
    #[graphql(name = "publishedAt")]
    pub published_at: Instant,
    #[graphql(name = "createdAt")]
    pub created_at: Instant,
    #[graphql(name = "updatedAt")]
    pub updated_at: Instant,
    pub tags: Vec<Tag>,
    pub feed: Option<Feed>,
}

#[derive(Default)]
pub struct FeedsQuery;

#[Object]
impl FeedsQuery {
    async fn feed_sync_states(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Vec<feeds::FeedSyncState>> {
        Ok(sync_service(ctx)?.states())
    }

    async fn feed(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
    ) -> async_graphql::Result<Option<Feed>> {
        Ok(feeds::get(db(ctx)?, id.as_str())?.map(feed_model))
    }

    async fn feeds(&self, ctx: &Context<'_>) -> async_graphql::Result<Vec<Feed>> {
        Ok(feeds::list(db(ctx)?)?.into_iter().map(feed_model).collect())
    }

    async fn feed_entry_counts(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<Vec<FeedEntryCount>> {
        Ok(feeds::entry_counts(db(ctx)?)?
            .into_iter()
            .map(|(id, count)| FeedEntryCount {
                id: id.into(),
                count,
            })
            .collect())
    }

    async fn feed_entry_count(
        &self,
        ctx: &Context<'_>,
        query: String,
    ) -> async_graphql::Result<i32> {
        Ok(feeds::count(db(ctx)?, &query)?)
    }

    async fn feed_entry(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
    ) -> async_graphql::Result<Option<FeedEntry>> {
        let library = db(ctx)?;
        let Some(row) = feeds::entry_get(library, id.as_str())? else {
            return Ok(None);
        };
        Ok(Some(entry(library, row, Vec::new()).await?))
    }

    async fn feed_entries(
        &self,
        ctx: &Context<'_>,
        offset: i32,
        limit: i32,
        query: String,
    ) -> async_graphql::Result<Vec<FeedEntry>> {
        let library = db(ctx)?;
        let rows = feeds::search(library, &query, limit, offset)?;
        let tags = tags_by_key(
            library,
            &rows.iter().map(|row| row.id.clone()).collect::<Vec<_>>(),
            DataType::FeedEntry,
        )?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let tagged = tags.get(&row.id).cloned().unwrap_or_default();
            out.push(entry(library, row, tagged).await?);
        }
        Ok(out)
    }
}

#[derive(Default)]
pub struct FeedsMutation;

#[Object]
impl FeedsMutation {
    async fn update_feed_url(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
        url: String,
    ) -> async_graphql::Result<Feed> {
        Ok(feed_model(feeds::update_url(db(ctx)?, id.as_str(), &url)?))
    }

    async fn mark_feed_entries_read(
        &self,
        ctx: &Context<'_>,
        query: String,
        read: bool,
    ) -> async_graphql::Result<ActionResult> {
        Ok(ActionResult {
            affected_count: feeds::mark_read(db(ctx)?, &query, read)? as i32,
        })
    }

    /// Starts the sync and returns immediately; the web client watches the
    /// websocket for `FEEDS_FETCHED` rather than blocking on this call.
    async fn sync_feeds(
        &self,
        ctx: &Context<'_>,
        id: Option<async_graphql::ID>,
    ) -> async_graphql::Result<bool> {
        sync_service(ctx)?.queue(id.map(|id| id.to_string()));
        Ok(true)
    }

    async fn update_feed(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
        name: String,
        fetch_content: bool,
    ) -> async_graphql::Result<Feed> {
        Ok(feed_model(feeds::update(
            db(ctx)?,
            id.as_str(),
            &name,
            fetch_content,
        )?))
    }

    async fn create_feed(
        &self,
        ctx: &Context<'_>,
        url: String,
        fetch_content: bool,
    ) -> async_graphql::Result<Feed> {
        let row = feeds::create_without_sync(db(ctx)?, &url, fetch_content).await?;
        sync_service(ctx)?.queue(Some(row.id.clone()));
        Ok(feed_model(row))
    }

    async fn import_feeds(
        &self,
        ctx: &Context<'_>,
        content: String,
    ) -> async_graphql::Result<bool> {
        feeds::import_opml(db(ctx)?, &content)?;
        Ok(true)
    }

    async fn export_feeds(&self, ctx: &Context<'_>) -> async_graphql::Result<String> {
        Ok(feeds::export_opml(db(ctx)?)?)
    }

    async fn delete_feed(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
    ) -> async_graphql::Result<bool> {
        let library = db(ctx)?;
        let row = feeds::get(library, id.as_str())?;
        let entries = feeds::search(library, &format!("feed_id:{}", id.as_str()), i32::MAX, 0)?;
        let deleted = feeds::delete(library, id.as_str())?;
        if deleted {
            let assets = assets(ctx);
            if let Some(assets) = assets {
                if let Some(row) = row {
                    assets.release(&row.logo);
                }
                for entry in entries {
                    assets.release(&entry.image);
                }
            }
        }
        Ok(deleted)
    }

    async fn sync_feed_entry_content(
        &self,
        ctx: &Context<'_>,
        id: async_graphql::ID,
    ) -> async_graphql::Result<FeedEntry> {
        let library = db(ctx)?;
        let row =
            feeds::sync_entry_content_with_assets(library, id.as_str(), assets(ctx).as_deref())
                .await?;
        let mut tags = tags_by_key(library, &[row.id.clone()], DataType::FeedEntry)?;
        let tagged = tags.remove(&row.id).unwrap_or_default();
        entry(library, row, tagged).await
    }

    /// Blank query refused for the same reason as `deleteNotes`: a bulk
    /// delete must never be aimable at the whole library by accident.
    async fn delete_feed_entries(
        &self,
        ctx: &Context<'_>,
        query: String,
    ) -> async_graphql::Result<ActionResult> {
        if query.trim().is_empty() {
            return Err("query is required".into());
        }
        let library = db(ctx)?;
        let entries = feeds::search(library, &query, i32::MAX, 0)?;
        let affected_count = feeds::delete_entries(library, &query)? as i32;
        if let Some(assets) = assets(ctx) {
            for entry in entries {
                assets.release(&entry.image);
            }
        }
        Ok(ActionResult { affected_count })
    }
}

fn feed_model(row: FeedRow) -> Feed {
    Feed {
        id: row.id.into(),
        name: row.name,
        url: row.url,
        fetch_content: row.fetch_content,
        logo: row.logo,
        last_sync_at: row
            .last_sync_at
            .as_deref()
            .map(super::public_facts::stored_instant),
        last_error: FeedError {
            code: serde_json::from_str::<serde_json::Value>(&row.last_error)
                .ok()
                .and_then(|value| value["code"].as_str().map(str::to_string))
                .unwrap_or_default(),
            detail: serde_json::from_str::<serde_json::Value>(&row.last_error)
                .ok()
                .and_then(|value| value["detail"].as_str().map(str::to_string))
                .unwrap_or_default(),
        },
        created_at: super::public_facts::stored_instant(&row.created_at),
        updated_at: super::public_facts::stored_instant(&row.updated_at),
    }
}

async fn entry(
    library: &Arc<Db>,
    row: FeedEntryRow,
    tags: Vec<Tag>,
) -> async_graphql::Result<FeedEntry> {
    Ok(FeedEntry {
        feed_id: row.feed_id.clone().into(),
        id: row.id.clone().into(),
        title: row.title,
        url: row.url,
        image: row.image,
        description: row.description,
        author: row.author,
        content: row.content,
        raw_id: row.raw_id,
        read: row.read,
        published_at: super::public_facts::stored_instant(&row.published_at),
        created_at: super::public_facts::stored_instant(&row.created_at),
        updated_at: super::public_facts::stored_instant(&row.updated_at),
        tags,
        feed: match feeds::get(library, &row.feed_id)? {
            Some(row) => Some(feed_model(row)),
            None => None,
        },
    })
}

fn assets(ctx: &Context<'_>) -> Option<Arc<feeds::FeedAssets>> {
    ctx.data_opt::<Arc<feeds::FeedAssets>>().cloned()
}

fn db<'a>(ctx: &'a Context<'_>) -> async_graphql::Result<&'a Arc<Db>> {
    ctx.data::<Arc<Db>>()
}

fn sync_service(ctx: &Context<'_>) -> async_graphql::Result<Arc<feeds::SyncService>> {
    // Let async-graphql's own "Data `...` does not exist" through: rewriting
    // it as a flat message told a caller the sync service was merely off when
    // the real fault was that it was never registered at all.
    Ok(ctx.data::<Arc<feeds::SyncService>>()?.clone())
}

#[cfg(test)]
#[path = "../../tests/unit/content_api/public_feeds.rs"]
mod tests;
