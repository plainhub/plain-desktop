//! Public `/graphql` image-search roots.
//!
//! The embedding model and the index itself are platform state; these roots
//! report it and forward the start/cancel/enable commands. Nothing here is
//! gated — plain-app lets any web client drive the index.

use serde::Serialize;

use async_graphql::{Context, Enum, Object, SimpleObject};
use std::sync::Arc;

#[derive(Enum, Copy, Clone, Eq, PartialEq, Debug, Default, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ImageSearchStatusType {
    #[default]
    Unavailable,
    Downloading,
    Loading,
    Ready,
    Error,
}

#[derive(SimpleObject, Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageSearchStatus {
    pub status: ImageSearchStatusType,
    #[graphql(name = "downloadProgress")]
    pub download_progress: i32,
    #[graphql(name = "errorMessage")]
    pub error_message: String,
    #[graphql(name = "modelSize")]
    pub model_size: crate::content_types::Long,
    #[graphql(name = "modelDir")]
    pub model_dir: String,
    #[graphql(name = "isIndexing")]
    pub is_indexing: bool,
    #[graphql(name = "totalImages")]
    pub total_images: i32,
    #[graphql(name = "indexedImages")]
    pub indexed_images: i32,
}

#[derive(Default)]
pub struct ImageIndexQuery;

#[Object]
impl ImageIndexQuery {
    async fn image_search_status(
        &self,
        ctx: &Context<'_>,
    ) -> async_graphql::Result<ImageSearchStatus> {
        Ok(ctx
            .data_unchecked::<Arc<super::image_models::Runtime>>()
            .snapshot()
            .status)
    }
}

#[derive(Default)]
pub struct ImageIndexMutation;

#[Object]
impl ImageIndexMutation {
    async fn enable_image_search(&self, ctx: &Context<'_>) -> async_graphql::Result<bool> {
        ctx.data_unchecked::<Arc<super::image_models::Runtime>>()
            .enable(false)
            .await
            .map_err(async_graphql::Error::new)?;
        Ok(true)
    }

    async fn disable_image_search(&self, ctx: &Context<'_>) -> async_graphql::Result<bool> {
        ctx.data_unchecked::<Arc<super::image_models::Runtime>>()
            .cancel(true)
            .await
            .map_err(async_graphql::Error::new)?;
        Ok(true)
    }

    async fn cancel_image_model_download(&self, ctx: &Context<'_>) -> async_graphql::Result<bool> {
        ctx.data_unchecked::<Arc<super::image_models::Runtime>>()
            .cancel(false)
            .await
            .map_err(async_graphql::Error::new)?;
        Ok(true)
    }

    /// `force` re-scans images that already have an embedding.
    async fn start_image_index(
        &self,
        ctx: &Context<'_>,
        force: Option<bool>,
    ) -> async_graphql::Result<bool> {
        ctx.data_unchecked::<Arc<super::image_index::ImageIndex>>()
            .start(force.unwrap_or(false))
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        Ok(true)
    }

    async fn cancel_image_index(&self, ctx: &Context<'_>) -> async_graphql::Result<bool> {
        ctx.data_unchecked::<Arc<super::image_index::ImageIndex>>()
            .cancel()
            .map_err(|e| async_graphql::Error::new(e.to_string()))?;
        Ok(true)
    }
}

#[cfg(test)]
#[path = "../../tests/unit/content_api/public_image_index.rs"]
mod tests;
