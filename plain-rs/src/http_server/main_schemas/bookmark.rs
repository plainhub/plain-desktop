use async_graphql::{Context, ID, Object};
use serde_json::json;
use std::sync::Arc;

use crate::db::bookmark as bookmark_db;
use crate::db::bookmark::{DBookmark, DBookmarkGroup};
use crate::db::{Db, now_iso};

use super::bookmark_types::{Bookmark, BookmarkGroup, BookmarkInput};
use super::types::ActionResult;
use crate::ws_event::{WS_BOOKMARK_UPDATED, WsEvent};
use tokio::sync::broadcast;

fn bookmark_to_json(b: &DBookmark) -> serde_json::Value {
    json!({
        "id": b.id,
        "url": b.url,
        "title": b.title,
        "faviconPath": b.favicon_path,
        "groupId": b.group_id,
        "pinned": b.pinned,
        "clickCount": b.click_count,
        "lastClickedAt": b.last_clicked_at,
        "sortOrder": b.sort_order,
        "createdAt": b.created_at,
        "updatedAt": b.updated_at,
    })
}

pub(super) fn emit_bookmark_updated(ctx: &Context<'_>, items: &[DBookmark]) {
    if items.is_empty() {
        return;
    }
    let payload = items.iter().map(bookmark_to_json).collect::<Vec<_>>();
    let _ = ctx
        .data_unchecked::<broadcast::Sender<WsEvent>>()
        .send(WsEvent::broadcast(
            WS_BOOKMARK_UPDATED,
            json!(payload).to_string(),
        ));
}

#[derive(Default)]
pub struct BookmarkQuery;

#[Object]
impl BookmarkQuery {
    async fn bookmarks(&self, ctx: &Context<'_>) -> Vec<Bookmark> {
        let db = ctx.data_unchecked::<Arc<Db>>();
        bookmark_db::get_bookmarks(db)
            .into_iter()
            .map(Bookmark::from)
            .collect()
    }

    async fn bookmark_groups(&self, ctx: &Context<'_>) -> Vec<BookmarkGroup> {
        let db = ctx.data_unchecked::<Arc<Db>>();
        let counts = bookmark_db::get_bookmarks(db).into_iter().fold(
            std::collections::HashMap::new(),
            |mut acc, b| {
                *acc.entry(b.group_id).or_insert(0i32) += 1;
                acc
            },
        );
        bookmark_db::get_bookmark_groups(db)
            .into_iter()
            .map(|g| {
                let count = counts.get(&g.id).copied().unwrap_or(0);
                BookmarkGroup::from_group(g, count)
            })
            .collect()
    }
}

#[derive(Default)]
pub struct BookmarkMutation;

#[Object]
impl BookmarkMutation {
    async fn add_bookmarks(
        &self,
        ctx: &Context<'_>,
        urls: Vec<String>,
        group_id: ID,
    ) -> async_graphql::Result<Vec<Bookmark>> {
        let db = ctx.data_unchecked::<Arc<Db>>();
        let created = urls
            .into_iter()
            .map(|url| url.trim().to_string())
            .filter(|url| !url.is_empty())
            .map(|url| {
                let bookmark = DBookmark::new(&url, group_id.as_str());
                bookmark_db::insert_bookmark(db, &bookmark)?;
                Ok(bookmark)
            })
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(created.into_iter().map(Bookmark::from).collect())
    }

    async fn update_bookmark(
        &self,
        ctx: &Context<'_>,
        id: ID,
        input: BookmarkInput,
    ) -> Result<Bookmark, async_graphql::Error> {
        let db = ctx.data_unchecked::<Arc<Db>>();
        let mut bookmark = bookmark_db::get_bookmark_by_id(db, id.as_str())
            .ok_or_else(|| async_graphql::Error::new("bookmark not found"))?;
        bookmark.url = input.url;
        bookmark.title = input.title;
        bookmark.group_id = input.group_id.to_string();
        bookmark.pinned = input.pinned;
        bookmark.sort_order = input.sort_order;
        bookmark.updated_at = now_iso();
        bookmark_db::update_fields(db, &bookmark)?;
        let bookmark = bookmark_db::get_bookmark_by_id(db, id.as_str())
            .ok_or_else(|| async_graphql::Error::new("bookmark not found"))?;
        emit_bookmark_updated(ctx, &[bookmark.clone()]);
        Ok(Bookmark::from(bookmark))
    }

    async fn delete_bookmarks(
        &self,
        ctx: &Context<'_>,
        ids: Vec<ID>,
    ) -> async_graphql::Result<ActionResult> {
        let db = ctx.data_unchecked::<Arc<Db>>();
        let ids = ids.into_iter().map(|id| id.to_string()).collect::<Vec<_>>();
        let removed = bookmark_db::delete_bookmarks_with_rows(db, &ids)?;
        let count = removed.len() as i32;
        if let Ok(assets) = ctx.data::<Arc<crate::feeds::FeedAssets>>() {
            for item in removed {
                assets.release(&item.favicon_path);
            }
        }
        Ok(ActionResult {
            affected_count: count,
        })
    }

    async fn record_bookmark_click(
        &self,
        ctx: &Context<'_>,
        id: ID,
    ) -> async_graphql::Result<bool> {
        bookmark_db::record_click(ctx.data_unchecked::<Arc<Db>>(), id.as_str())?;
        Ok(true)
    }

    async fn create_bookmark_group(
        &self,
        ctx: &Context<'_>,
        name: String,
    ) -> async_graphql::Result<BookmarkGroup> {
        let db = ctx.data_unchecked::<Arc<Db>>();
        let group = DBookmarkGroup::new(name.trim());
        bookmark_db::insert_bookmark_group(db, &group)?;
        Ok(BookmarkGroup::from_group(group, 0))
    }

    async fn update_bookmark_group(
        &self,
        ctx: &Context<'_>,
        id: ID,
        name: String,
        collapsed: bool,
        sort_order: i32,
    ) -> Result<BookmarkGroup, async_graphql::Error> {
        let db = ctx.data_unchecked::<Arc<Db>>();
        let mut group = bookmark_db::get_bookmark_group_by_id(db, id.as_str())
            .ok_or_else(|| async_graphql::Error::new("bookmark group not found"))?;
        group.name = name;
        group.collapsed = collapsed;
        group.sort_order = sort_order;
        group.updated_at = now_iso();
        bookmark_db::update_bookmark_group(db, &group)?;
        let item_count = bookmark_db::get_bookmarks_by_group_id(db, &group.id).len() as i32;
        Ok(BookmarkGroup::from_group(group, item_count))
    }

    async fn delete_bookmark_group(
        &self,
        ctx: &Context<'_>,
        id: ID,
    ) -> async_graphql::Result<bool> {
        let db = ctx.data_unchecked::<Arc<Db>>();
        let affected = bookmark_db::get_bookmarks_by_group_id(db, id.as_str());
        bookmark_db::delete_bookmark_group(db, id.as_str())?;
        if !affected.is_empty() {
            let updated = affected
                .into_iter()
                .map(|mut b| {
                    b.group_id.clear();
                    b.updated_at = now_iso();
                    b
                })
                .collect::<Vec<_>>();
            emit_bookmark_updated(ctx, &updated);
        }
        Ok(true)
    }
}
