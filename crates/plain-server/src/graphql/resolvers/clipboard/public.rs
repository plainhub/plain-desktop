use crate::graphql::resolvers::clipboard::host::ClipboardItem;
use crate::graphql::types::ActionResult;
use crate::host::bridge::Host;
use crate::http::auth::public_gate;
use async_graphql::{Context, Object};
use plain_rs::db::Db;
use plain_rs::prefs::Prefs;
use std::sync::Arc;

const PERMISSION: &str = "CLIPBOARD";
const MAX_LIMIT: i32 = 200;

#[derive(Default)]
pub struct ClipboardQuery;

#[Object]
impl ClipboardQuery {
    /// Paged clipboard history, newest first.
    async fn clipboard_items(
        &self,
        ctx: &Context<'_>,
        query: String,
        offset: i32,
        limit: i32,
    ) -> async_graphql::Result<Vec<ClipboardItem>> {
        enabled(ctx)?;
        let (limit, offset) = if public_gate::is_local(ctx) {
            (limit, offset)
        } else {
            (limit.clamp(1, MAX_LIMIT), offset.max(0))
        };
        ctx.data::<Arc<Db>>()?
            .clipboard_page(&query, limit.into(), offset.into())?
            .into_iter()
            .map(ClipboardItem::try_from)
            .collect()
    }

    async fn clipboard_item_count(
        &self,
        ctx: &Context<'_>,
        query: String,
    ) -> async_graphql::Result<i32> {
        enabled(ctx)?;
        Ok(ctx.data::<Arc<Db>>()?.clipboard_count(&query)?.try_into()?)
    }
}

#[derive(Default)]
pub struct ClipboardMutation;

#[Object]
impl ClipboardMutation {
    /// Write text into the system clipboard on behalf of the calling client.
    /// The write is recorded with the caller as source; the local watcher
    /// sees it but skips re-broadcasting (hash dedup), so there is no loop.
    async fn set_clipboard(&self, ctx: &Context<'_>, text: String) -> async_graphql::Result<bool> {
        enabled(ctx)?;
        ctx.data::<Arc<Host>>()?
            .call("systemSetClipboard", serde_json::json!({ "text": text }))
            .await
            .map_err(|error| async_graphql::Error::new(error))?;
        Ok(true)
    }

    /// Delete clipboard history entries matching the query DSL (`ids:` /
    /// `text:`; a bare word is a text LIKE). A blank query is rejected —
    /// send `all:true` to target everything.
    async fn delete_clipboard_items(
        &self,
        ctx: &Context<'_>,
        query: String,
    ) -> async_graphql::Result<ActionResult> {
        enabled(ctx)?;
        if query.trim().is_empty() {
            return Err(async_graphql::Error::new(
                "query is required for bulk mutations — pass 'all:true' to explicitly target everything",
            ));
        }
        Ok(ActionResult {
            affected_count: ctx
                .data::<Arc<Db>>()?
                .clipboard_delete_query(&query)
                .map_err(async_graphql::Error::new)?
                .try_into()?,
        })
    }
}

fn enabled(ctx: &Context<'_>) -> async_graphql::Result<()> {
    // plain-app's clipboard roots raise their own error here, not `no_permission`.
    if public_gate::is_local(ctx)
        || public_gate::is_enabled(ctx.data_unchecked::<Arc<Prefs>>(), &[PERMISSION])
    {
        Ok(())
    } else {
        Err(async_graphql::Error::new("clipboard_sync_disabled"))
    }
}

#[cfg(test)]
#[path = "../../../../tests/unit/graphql/resolvers/clipboard/public.rs"]
mod tests;
