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
    let document = scraper::Html::parse_document(html);
    let title = document
        .select(&scraper::Selector::parse("meta[property='og:title']").unwrap())
        .find_map(|e| e.value().attr("content").map(str::to_owned))
        .or_else(|| {
            document
                .select(&scraper::Selector::parse("title").unwrap())
                .next()
                .map(|e| e.text().collect::<String>())
        })
        .map(|t| t.trim().chars().take(200).collect::<String>())
        .filter(|t| !t.is_empty());
    let icon = document
        .select(&scraper::Selector::parse("link[rel][href]").unwrap())
        .filter(|e| {
            e.value()
                .attr("rel")
                .unwrap_or_default()
                .to_lowercase()
                .contains("icon")
        })
        .find_map(|e| crate::feeds::assets::absolute_url(base, e.value().attr("href")?))
        .or_else(|| crate::feeds::assets::absolute_url(base, "/favicon.ico"));
    (title, icon)
}
