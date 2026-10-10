//! Public `/graphql` clipboard roots.
//!
//! The app's own clipboard view ([`super::schema::clipboard`]) is not
//! reusable here: the public roots are gated on the `CLIPBOARD` web
//! permission and clamp the page window, while the app talks to itself
//! without a gate.

use super::public_gate;
use super::schema::clipboard::ClipboardItem;
use crate::content_api::host::Host;
use crate::content_types::ActionResult;
use crate::{db::Db, prefs::Prefs};
use async_graphql::{Context, Object};
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
        offset: i32,
        limit: i32,
        query: String,
    ) -> async_graphql::Result<Vec<ClipboardItem>> {
        enabled(ctx)?;
        ctx.data::<Arc<Db>>()?
            .clipboard_page(
                &query,
                limit.clamp(1, MAX_LIMIT).into(),
                offset.max(0).into(),
            )?
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
    /// send `all:true` to target everything (API_SPEC §5).
    async fn delete_clipboard_items(
        &self,
        ctx: &Context<'_>,
        query: String,
    ) -> async_graphql::Result<ActionResult> {
        enabled(ctx)?;
        if query.trim().is_empty() {
            return Err(async_graphql::Error::new(
                "query is required for bulk mutations — pass 'all:true' to explicitly target everything (API_SPEC §5)",
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
    if public_gate::is_enabled(ctx.data_unchecked::<Arc<Prefs>>(), &[PERMISSION]) {
        Ok(())
    } else {
        Err(async_graphql::Error::new("clipboard_sync_disabled"))
    }
}

#[cfg(test)]
#[path = "../../tests/unit/content_api/public_clipboard.rs"]
mod tests;
