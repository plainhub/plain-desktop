use async_graphql::{Context, ID, Object};
use serde_json::json;
use std::sync::Arc;

use crate::local::db::{DBookmark, DBookmarkGroup, now_iso};

use super::super::context::{AppCtx, WS_BOOKMARK_UPDATED, WsEvent};
use super::types::{ActionResult, Bookmark, BookmarkGroup, BookmarkInput};

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

fn emit_bookmark_updated(ctx: &AppCtx, items: &[DBookmark]) {
    if items.is_empty() {
        return;
    }
    let payload = items.iter().map(bookmark_to_json).collect::<Vec<_>>();
    let _ = ctx.event_tx.send(WsEvent {
        event_type: WS_BOOKMARK_UPDATED,
        payload: json!(payload).to_string(),
    });
}

#[derive(Default)]
pub struct BookmarkQuery;

#[Object]
impl BookmarkQuery {
    async fn bookmarks(&self, ctx: &Context<'_>) -> Vec<Bookmark> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        c.db.get_bookmarks()
            .into_iter()
            .map(Bookmark::from)
            .collect()
    }

    async fn bookmark_groups(&self, ctx: &Context<'_>) -> Vec<BookmarkGroup> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        let counts = c
            .db
            .get_bookmarks()
            .into_iter()
            .fold(std::collections::HashMap::new(), |mut acc, b| {
                *acc.entry(b.group_id).or_insert(0i32) += 1;
                acc
            });
        c.db.get_bookmark_groups()
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
        group_id: String,
    ) -> Vec<Bookmark> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        let created = urls
            .into_iter()
            .map(|url| url.trim().to_string())
            .filter(|url| !url.is_empty())
            .map(|url| {
                let bookmark = DBookmark::new(&url, &group_id);
                c.db.insert_bookmark(&bookmark);
                bookmark
            })
            .collect::<Vec<_>>();
        created.into_iter().map(Bookmark::from).collect()
    }

    async fn update_bookmark(
        &self,
        ctx: &Context<'_>,
        id: ID,
        input: BookmarkInput,
    ) -> Result<Bookmark, async_graphql::Error> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        let mut bookmark = c
            .db
            .get_bookmark_by_id(id.as_str())
            .ok_or_else(|| async_graphql::Error::new("bookmark not found"))?;
        bookmark.url = input.url;
        bookmark.title = input.title;
        bookmark.group_id = input.group_id;
        bookmark.pinned = input.pinned;
        bookmark.sort_order = input.sort_order;
        bookmark.updated_at = now_iso();
        c.db.update_bookmark(&bookmark);
        emit_bookmark_updated(c, &[bookmark.clone()]);
        Ok(Bookmark::from(bookmark))
    }

    async fn delete_bookmarks(&self, ctx: &Context<'_>, ids: Vec<ID>) -> ActionResult {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        let ids = ids.into_iter().map(|id| id.to_string()).collect::<Vec<_>>();
        ActionResult {
            affected_count: c.db.delete_bookmarks(&ids),
        }
    }

    async fn record_bookmark_click(&self, ctx: &Context<'_>, id: ID) -> bool {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        let Some(mut bookmark) = c.db.get_bookmark_by_id(id.as_str()) else {
            return true;
        };
        let now = now_iso();
        bookmark.click_count += 1;
        bookmark.last_clicked_at = Some(now.clone());
        bookmark.updated_at = now;
        c.db.update_bookmark(&bookmark);
        true
    }

    async fn create_bookmark_group(&self, ctx: &Context<'_>, name: String) -> BookmarkGroup {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        let group = DBookmarkGroup::new(name.trim());
        c.db.insert_bookmark_group(&group);
        BookmarkGroup::from_group(group, 0)
    }

    async fn update_bookmark_group(
        &self,
        ctx: &Context<'_>,
        id: ID,
        name: String,
        collapsed: bool,
        sort_order: i32,
    ) -> Result<BookmarkGroup, async_graphql::Error> {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        let mut group = c
            .db
            .get_bookmark_group_by_id(id.as_str())
            .ok_or_else(|| async_graphql::Error::new("bookmark group not found"))?;
        group.name = name;
        group.collapsed = collapsed;
        group.sort_order = sort_order;
        group.updated_at = now_iso();
        c.db.update_bookmark_group(&group);
        let item_count = c.db.get_bookmarks_by_group_id(&group.id).len() as i32;
        Ok(BookmarkGroup::from_group(group, item_count))
    }

    async fn delete_bookmark_group(&self, ctx: &Context<'_>, id: ID) -> bool {
        let c = ctx.data_unchecked::<Arc<AppCtx>>();
        let affected = c.db.get_bookmarks_by_group_id(id.as_str());
        c.db.delete_bookmark_group(id.as_str());
        if !affected.is_empty() {
            let updated = affected
                .into_iter()
                .map(|mut b| {
                    b.group_id.clear();
                    b.updated_at = now_iso();
                    b
                })
                .collect::<Vec<_>>();
            emit_bookmark_updated(c, &updated);
        }
        true
    }
}
