use super::bookmark_types::Bookmark;
use crate::{
    db::{Db, bookmark as store},
    feeds::FeedAssets,
};
use async_graphql::{Context, ID, Object};
use std::sync::Arc;

#[derive(Default)]
pub struct BookmarkMetadataMutation;
#[Object]
impl BookmarkMetadataMutation {
    async fn fetch_bookmark_metadata(
        &self,
        ctx: &Context<'_>,
        id: ID,
    ) -> async_graphql::Result<Option<Bookmark>> {
        let db = ctx.data_unchecked::<Arc<Db>>();
        let assets = ctx.data_unchecked::<Arc<FeedAssets>>();
        let Some(original) = store::get_bookmark_by_id(db, id.as_str()) else {
            return Ok(None);
        };
        let url = reqwest::Url::parse(&original.url)?;
        if !["http", "https"].contains(&url.scheme()) {
            return Err(async_graphql::Error::new("unsupported bookmark URL"));
        }
        let html = crate::feeds::fetch_text(url.as_str(), 2 * 1024 * 1024)
            .await
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        let (title, icon_url) = page_metadata(&html, url.as_str());
        let title = title.unwrap_or_else(|| original.title.clone());
        let cached = match icon_url {
            Some(url) => assets.cache_bookmark_icon(&url).await.ok(),
            None => None,
        };
        let icon = cached.as_deref().unwrap_or(&original.favicon_path);
        if title == original.title && icon == original.favicon_path {
            if let Some(path) = cached {
                assets.release(&path);
            }
            return Ok(None);
        }
        let result = store::update_metadata(db, &original, &title, icon);
        if !matches!(result, Ok(1)) {
            if let Some(path) = cached {
                assets.release(&path);
            }
            result?;
            return Ok(None);
        }
        if cached.is_some() {
            assets.release(&original.favicon_path);
        }
        let updated = store::get_bookmark_by_id(db, id.as_str());
        if let Some(b) = &updated {
            super::bookmark::emit_bookmark_updated(ctx, std::slice::from_ref(b));
        }
        Ok(updated.map(Bookmark::from))
    }
}

fn page_metadata(html: &str, base: &str) -> (Option<String>, Option<String>) {
    let dom = crate::utils::html_to_markdown::dom::Dom::parse(html);
    let title = dom
        .0
        .iter()
        .find(|node| node.tag == "meta" && node.attr("property") == "og:title")
        .map(|node| node.attr("content").to_owned())
        .or_else(|| {
            dom.0
                .iter()
                .enumerate()
                .find(|(_, node)| node.tag == "title")
                .map(|(id, _)| dom.text(id))
        })
        .map(|t| t.trim().chars().take(200).collect::<String>())
        .filter(|t| !t.is_empty());
    let icon = dom
        .0
        .iter()
        .filter(|node| node.tag == "link" && node.attr("rel").to_lowercase().contains("icon"))
        .find_map(|node| crate::feeds::assets::absolute_url(base, node.attr("href")))
        .or_else(|| crate::feeds::assets::absolute_url(base, "/favicon.ico"));
    (title, icon)
}
